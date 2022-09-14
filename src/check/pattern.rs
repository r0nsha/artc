use super::{
    env::{Env, Scope, ScopeKind},
    top_level::CallerInfo,
    CheckSess,
};
use crate::{
    ast::{
        self,
        pattern::{
            NamePattern, Pattern, StructUnpackPattern, StructUnpackSubPattern, TupleUnpackPattern, UnpackPatternKind,
        },
    },
    error::{
        diagnostic::{Diagnostic, Label},
        DiagnosticResult, SyntaxError, TypeError,
    },
    hir,
    infer::{display::DisplayType, normalize::Normalize},
    span::Span,
    types::{Type, TypeId},
    workspace::{BindingId, BindingInfoFlags, BindingInfoKind, ModuleId, PartialBindingInfo, ScopeLevel},
};
use ustr::{ustr, Ustr, UstrMap};

impl<'s> CheckSess<'s> {
    pub fn get_global_binding_id(&self, module_id: ModuleId, name: Ustr) -> Option<BindingId> {
        self.global_scopes
            .get(&module_id)
            .and_then(|scope| scope.bindings.get(&name).copied())
    }

    pub fn insert_global_binding_id(&mut self, module_id: ModuleId, name: Ustr, id: BindingId) {
        self.global_scopes
            .entry(module_id)
            .or_insert({
                let module_name = self.workspace.module_infos.get(module_id).unwrap().name;
                Scope::new(module_name, ScopeKind::Global)
            })
            .bindings
            .insert(name, id);
    }

    pub fn get_binding_id(&self, env: &Env, name: Ustr) -> Option<BindingId> {
        env.find_binding(name)
            .or_else(|| self.get_global_binding_id(env.module_id(), name))
    }

    pub fn bind_name(
        &mut self,
        env: &mut Env,
        name: Ustr,
        visibility: ast::Visibility,
        ty: TypeId,
        value: Option<hir::Node>,
        is_mutable: bool,
        kind: BindingInfoKind,
        span: Span,
        flags: BindingInfoFlags,
    ) -> DiagnosticResult<(BindingId, hir::Node)> {
        let module_id = env.module_id();
        let scope_level = env.scope_level();

        let partial_binding_info = PartialBindingInfo {
            module_id,
            name,
            visibility,
            ty,
            const_value: if is_mutable || flags.contains(BindingInfoFlags::NO_CONST_FOLD) {
                None
            } else {
                value.as_ref().map(|v| v.as_const_value().cloned()).flatten()
            },
            is_mutable,
            kind,
            scope_level,
            qualified_name: get_qualified_name(env.scope_name(), name),
            span,
            flags,
        };

        let id = self
            .workspace
            .binding_infos
            .insert_with_id(partial_binding_info.clone().into_binding_info());

        match scope_level {
            // check if there's already a binding with this symbol
            ScopeLevel::Global => {
                if let Some(defined_id) = self.get_global_binding_id(module_id, name) {
                    let defined_binding_info = self.workspace.binding_infos.get(defined_id).unwrap();

                    if defined_binding_info.span != span {
                        return Err(SyntaxError::duplicate_binding(
                            defined_binding_info.name,
                            span,
                            defined_binding_info.span,
                        ));
                    }
                } else {
                    if is_mutable && !matches!(kind, BindingInfoKind::LetStatic) {
                        self.workspace.diagnostics.push(
                            Diagnostic::error()
                                .with_message(format!("top level let binding `{}` cannot be mutable", name))
                                .with_label(Label::primary(span, "cannot be mutable"))
                                .with_note("try prefix the binding with `static`"),
                        );
                    }

                    // insert the symbol into its module's global scope
                    self.insert_global_binding_id(module_id, name, id);
                }
            }
            ScopeLevel::Scope(_) => {
                // insert the symbol into local scope
                env.insert_binding(name, id);
            }
        }

        let node = if let Some(value) = value {
            hir::Node::Binding(hir::Binding {
                module_id,
                id,
                name,
                value: Box::new(value),
                ty: self.tcx.common_types.unit,
                span,
            })
        } else {
            hir::Node::Id(hir::Id { id, ty, span })
        };

        Ok((id, node))
    }

    pub fn bind_name_pattern(
        &mut self,
        env: &mut Env,
        pattern: &NamePattern,
        visibility: ast::Visibility,
        ty: TypeId,
        value: Option<hir::Node>,
        kind: BindingInfoKind,
        flags: BindingInfoFlags,
    ) -> DiagnosticResult<(BindingId, hir::Node)> {
        self.bind_name(
            env,
            pattern.name,
            visibility,
            ty,
            value,
            pattern.is_mutable,
            kind,
            pattern.span,
            if pattern.ignore {
                flags | BindingInfoFlags::IGNORE
            } else {
                flags
            },
        )
    }

    pub fn bind_pattern(
        &mut self,
        env: &mut Env,
        pattern: &Pattern,
        visibility: ast::Visibility,
        ty: TypeId,
        value: Option<hir::Node>,
        kind: BindingInfoKind,
        ty_origin_span: Span,
        flags: BindingInfoFlags,
    ) -> DiagnosticResult<(BindingId, hir::Node)> {
        match pattern {
            Pattern::Name(pattern) => self.bind_name_pattern(env, pattern, visibility, ty, value, kind, flags),
            Pattern::StructUnpack(pattern) => {
                let mut statements = vec![];

                let (id, id_node) = self.bind_temp_name_for_unpack_pattern(
                    env,
                    visibility,
                    ty,
                    value,
                    kind,
                    pattern.span,
                    &mut statements,
                    flags,
                )?;

                self.bind_struct_unpack_pattern(
                    &mut statements,
                    env,
                    pattern,
                    visibility,
                    ty,
                    id_node,
                    kind,
                    ty_origin_span,
                    flags,
                )?;

                Ok((
                    id,
                    hir::Node::Sequence(hir::Sequence {
                        statements,
                        ty: self.tcx.common_types.unit,
                        span: pattern.span,
                        is_scope: false,
                    }),
                ))
            }
            Pattern::TupleUnpack(pattern) => {
                let mut statements = vec![];

                let (id, id_node) = self.bind_temp_name_for_unpack_pattern(
                    env,
                    visibility,
                    ty,
                    value,
                    kind,
                    pattern.span,
                    &mut statements,
                    flags,
                )?;

                self.bind_tuple_unpack_pattern(
                    &mut statements,
                    env,
                    pattern,
                    visibility,
                    id_node,
                    kind,
                    ty_origin_span,
                    flags,
                )?;

                Ok((
                    id,
                    hir::Node::Sequence(hir::Sequence {
                        statements,
                        ty: self.tcx.common_types.unit,
                        span: pattern.span,
                        is_scope: false,
                    }),
                ))
            }
            Pattern::Hybrid(pattern) => {
                let mut statements = vec![];

                let (id, bound_node) =
                    self.bind_name_pattern(env, &pattern.name_pattern, visibility, ty, value.clone(), kind, flags)?;

                let id_node = self.get_id_node_for_unpack_pattern(bound_node, &mut statements);

                match &pattern.unpack_pattern {
                    UnpackPatternKind::Struct(pattern) => self.bind_struct_unpack_pattern(
                        &mut statements,
                        env,
                        pattern,
                        visibility,
                        ty,
                        id_node,
                        kind,
                        ty_origin_span,
                        flags,
                    )?,
                    UnpackPatternKind::Tuple(pattern) => self.bind_tuple_unpack_pattern(
                        &mut statements,
                        env,
                        pattern,
                        visibility,
                        id_node,
                        kind,
                        ty_origin_span,
                        flags,
                    )?,
                }

                Ok((
                    id,
                    hir::Node::Sequence(hir::Sequence {
                        statements,
                        ty: self.tcx.common_types.unit,
                        span: pattern.span,
                        is_scope: false,
                    }),
                ))
            }
        }
    }

    fn bind_temp_name_for_unpack_pattern(
        &mut self,
        env: &mut Env,
        visibility: ast::Visibility,
        ty: TypeId,
        value: Option<hir::Node>,
        kind: BindingInfoKind,
        span: Span,
        statements: &mut Vec<hir::Node>,
        flags: BindingInfoFlags,
    ) -> Result<(BindingId, hir::Node), Diagnostic> {
        let name = self.generate_name("v");

        let (id, bound_node) = self.bind_name(
            env,
            name,
            visibility,
            ty,
            value,
            false,
            kind,
            span,
            flags - BindingInfoFlags::IS_USER_DEFINED,
        )?;

        let id_node = self.get_id_node_for_unpack_pattern(bound_node, statements);

        Ok((id, id_node))
    }

    fn get_id_node_for_unpack_pattern(&mut self, bound_node: hir::Node, statements: &mut Vec<hir::Node>) -> hir::Node {
        match bound_node {
            hir::Node::Binding(ref binding) => {
                let id_node = self.id_or_const_by_id(binding.id, binding.span);
                statements.push(bound_node);
                id_node
            }
            hir::Node::Id(_) => bound_node,
            _ => panic!("got {:?}", bound_node),
        }
    }

    fn bind_struct_unpack_pattern(
        &mut self,
        statements: &mut Vec<hir::Node>,
        env: &mut Env,
        unpack_pattern: &StructUnpackPattern,
        visibility: ast::Visibility,
        ty: TypeId,
        value: hir::Node,
        kind: BindingInfoKind,
        ty_origin_span: Span,
        flags: BindingInfoFlags,
    ) -> DiagnosticResult<()> {
        match ty.normalize(&self.tcx).maybe_deref_once() {
            Type::Module(module_id) => {
                self.check_module_by_id(module_id)?;

                // println!(
                //     "`{}` -> `{}`",
                //     env.module_info().name,
                //     self.workspace.module_infos[module_id].name,
                // );

                let module_bindings = self.global_scopes.get(&module_id).unwrap().bindings.clone();

                fn find_name(bindings: &UstrMap<BindingId>, name: Ustr) -> Option<BindingId> {
                    // TODO: respect `self`
                    // TODO: respect `super`
                    bindings.get(&name).copied()
                }

                let mut unpacked_names = UstrMap::default();

                for pattern in unpack_pattern.sub_patterns.iter() {
                    if let Some(already_unpacked_span) = unpacked_names.insert(pattern.name(), pattern.span()) {
                        return Err(Diagnostic::error()
                            .with_message(format!("symbol `{}` has already been unpacked", pattern.name()))
                            .with_label(Label::primary(pattern.span(), "duplicate unpack"))
                            .with_label(Label::secondary(already_unpacked_span, "already unpacked here")));
                    }

                    match pattern {
                        StructUnpackSubPattern::Name(pattern) => {
                            let caller_info = CallerInfo {
                                module_id: env.module_id(),
                                span: pattern.span,
                            };

                            let id = match find_name(&module_bindings, pattern.name) {
                                Some(id) => id,
                                None => return Err(self.name_not_found_error(module_id, pattern.name, caller_info)),
                            };

                            self.validate_item_visibility(id, caller_info)?;

                            let binding_info = self.workspace.binding_infos.get(id).unwrap();

                            let (_, binding) = self.bind_name_pattern(
                                env,
                                pattern,
                                visibility,
                                binding_info.ty,
                                Some(self.id_or_const(binding_info, pattern.span)),
                                kind,
                                flags | BindingInfoFlags::TYPE_WAS_INFERRED,
                            )?;

                            statements.push(binding);
                        }
                        StructUnpackSubPattern::NameAndPattern(ast::NameAndSpan { name, span }, pattern) => {
                            let (name, span) = (*name, *span);

                            let caller_info = CallerInfo {
                                module_id: env.module_id(),
                                span,
                            };

                            let id = match find_name(&module_bindings, name) {
                                Some(id) => id,
                                None => return Err(self.name_not_found_error(module_id, name, caller_info)),
                            };

                            self.validate_item_visibility(id, caller_info)?;

                            let binding_info = self.workspace.binding_infos.get(id).unwrap();

                            let (_, binding) = self.bind_pattern(
                                env,
                                pattern,
                                visibility,
                                binding_info.ty,
                                Some(self.id_or_const(binding_info, span)),
                                kind,
                                ty_origin_span,
                                flags | BindingInfoFlags::TYPE_WAS_INFERRED,
                            )?;

                            statements.push(binding);
                        }
                    }
                }

                if let Some(wildcard) = &unpack_pattern.wildcard {
                    for (_, &id) in module_bindings.iter() {
                        let binding_info = self.workspace.binding_infos.get(id).unwrap();

                        if binding_info.visibility == ast::Visibility::Private {
                            continue;
                        }

                        // skip explicitly unpacked bindings
                        if unpacked_names.contains_key(&binding_info.name) {
                            continue;
                        }

                        let (_, binding) = self.bind_name(
                            env,
                            binding_info.name,
                            visibility,
                            binding_info.ty,
                            Some(self.id_or_const(binding_info, wildcard.span)),
                            binding_info.is_mutable,
                            binding_info.kind,
                            wildcard.span,
                            flags - BindingInfoFlags::IS_USER_DEFINED,
                        )?;

                        statements.push(binding);
                    }
                }

                Ok(())
            }
            Type::Struct(struct_type) => {
                let mut unpacked_names = UstrMap::default();

                for (index, pattern) in unpack_pattern.sub_patterns.iter().enumerate() {
                    let name = pattern.name();
                    let span = pattern.span();

                    if let Some(already_unpacked_span) = unpacked_names.insert(name, span) {
                        return Err(Diagnostic::error()
                            .with_message(format!("field `{}` has already been unpacked", name))
                            .with_label(Label::primary(span, "duplicate unpack"))
                            .with_label(Label::secondary(already_unpacked_span, "already unpacked here")));
                    } else if let Some(field) = struct_type.field(name) {
                        let ty = self.tcx.bound(field.ty.clone(), span);

                        let field_value = match value.as_const_value() {
                            Some(const_value) if !pattern.is_mutable() => hir::Node::Const(hir::Const {
                                value: const_value.as_struct().unwrap().get(&name).unwrap().clone().value,
                                ty,
                                span,
                            }),
                            _ => hir::Node::MemberAccess(hir::MemberAccess {
                                value: Box::new(value.clone()),
                                member_name: name,
                                member_index: index as _,
                                ty,
                                span,
                            }),
                        };

                        let (_, bound_node) = match pattern {
                            StructUnpackSubPattern::Name(pattern) => self.bind_name_pattern(
                                env,
                                pattern,
                                visibility,
                                ty,
                                Some(field_value),
                                kind,
                                flags | BindingInfoFlags::TYPE_WAS_INFERRED,
                            )?,
                            StructUnpackSubPattern::NameAndPattern(_, pattern) => self.bind_pattern(
                                env,
                                pattern,
                                visibility,
                                ty,
                                Some(field_value),
                                kind,
                                ty_origin_span,
                                flags | BindingInfoFlags::TYPE_WAS_INFERRED,
                            )?,
                        };

                        statements.push(bound_node);
                    } else {
                        return Err(TypeError::invalid_struct_field(
                            pattern.span(),
                            pattern.name(),
                            struct_type.display(&self.tcx),
                        ));
                    }
                }

                if let Some(wildcard) = &unpack_pattern.wildcard {
                    for (index, field) in struct_type.fields.iter().enumerate() {
                        // skip explicitly unpacked fields
                        if unpacked_names.contains_key(&field.name) {
                            continue;
                        }

                        let ty = self.tcx.bound(field.ty.clone(), field.span);

                        let field_value = match value.as_const_value() {
                            Some(const_value) => hir::Node::Const(hir::Const {
                                value: const_value.as_struct().unwrap().get(&field.name).unwrap().value.clone(),
                                ty,
                                span: field.span,
                            }),
                            None => hir::Node::MemberAccess(hir::MemberAccess {
                                value: Box::new(value.clone()),
                                member_name: field.name,
                                member_index: index as _,
                                ty,
                                span: field.span,
                            }),
                        };

                        let (_, bound_node) = self.bind_name(
                            env,
                            field.name,
                            visibility,
                            ty,
                            Some(field_value),
                            false,
                            kind,
                            wildcard.span,
                            flags - BindingInfoFlags::IS_USER_DEFINED,
                        )?;

                        statements.push(bound_node);
                    }
                }

                Ok(())
            }
            _ => Err(Diagnostic::error()
                .with_message(format!("cannot use tuple unpack on type `{}`", ty.display(&self.tcx)))
                .with_label(Label::primary(unpack_pattern.span, "illegal tuple unpack"))),
        }
    }

    fn bind_tuple_unpack_pattern(
        &mut self,
        statements: &mut Vec<hir::Node>,
        env: &mut Env,
        pattern: &TupleUnpackPattern,
        visibility: ast::Visibility,
        value: hir::Node,
        kind: BindingInfoKind,
        ty_origin_span: Span,
        flags: BindingInfoFlags,
    ) -> DiagnosticResult<()> {
        match value.ty().normalize(&self.tcx) {
            Type::Tuple(elem_types) => {
                if pattern.sub_patterns.len() <= elem_types.len() {
                    let mut pattern_types: Vec<TypeId> = vec![];

                    pattern.sub_patterns.iter().enumerate().for_each(|(index, pattern)| {
                        let ty = match elem_types.get(index) {
                            Some(elem) => self.tcx.bound(elem.clone(), pattern.span()),
                            None => self.tcx.var(pattern.span()),
                        };

                        pattern_types.push(ty)
                    });

                    for ((index, sub_pattern), &ty) in pattern.sub_patterns.iter().enumerate().zip(pattern_types.iter())
                    {
                        let element_value = |pattern: &Pattern| match value.as_const_value() {
                            Some(const_value) if !pattern.is_mutable() => hir::Node::Const(hir::Const {
                                value: const_value.as_tuple().unwrap()[index].value.clone(),
                                ty,
                                span: value.span(),
                            }),
                            _ => hir::Node::MemberAccess(hir::MemberAccess {
                                value: Box::new(value.clone()),
                                member_name: ustr(&index.to_string()),
                                member_index: index as _,
                                ty,
                                span: value.span(),
                            }),
                        };

                        let element_value = element_value(sub_pattern);

                        let (_, bound_node) = self.bind_pattern(
                            env,
                            sub_pattern,
                            visibility,
                            ty,
                            Some(element_value),
                            kind,
                            ty_origin_span,
                            flags | BindingInfoFlags::TYPE_WAS_INFERRED,
                        )?;

                        statements.push(bound_node);
                    }

                    Ok(())
                } else {
                    Err(Diagnostic::error()
                        .with_message(format!(
                            "too many unpacked elements - expected {} elements, got {}",
                            elem_types.len(),
                            pattern.sub_patterns.len()
                        ))
                        .with_label(Label::primary(pattern.span, "too many elements")))
                }
            }
            ty => Err(Diagnostic::error()
                .with_message(format!("cannot use tuple unpack on type `{}`", ty.display(&self.tcx)))
                .with_label(Label::primary(pattern.span, "illegal tuple unpack"))),
        }
    }
}

pub(super) fn get_qualified_name(scope_name: Ustr, name: Ustr) -> Ustr {
    if scope_name.is_empty() {
        name
    } else {
        ustr(&format!("{}.{}", scope_name, name))
    }
}
