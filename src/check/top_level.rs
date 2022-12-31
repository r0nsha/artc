use super::{sym, Check, CheckResult, CheckSess, QueuedModule};
use crate::{
    ast,
    error::diagnostic::{Diagnostic, Label},
    hir::{self, const_value::ConstValue},
    infer::substitute::substitute_node,
    span::Span,
    types::{Type, TypeId},
    workspace::{BindingId, ModuleId, ModuleInfo},
};
use std::collections::HashSet;
use ustr::{Ustr, UstrMap};

#[derive(Debug, Clone, Copy)]
pub struct CallerInfo {
    pub module_id: ModuleId,
    pub span: Span,
}

impl<'s> CheckSess<'s> {
    pub fn check_top_level_name(
        &mut self,
        name: Ustr,
        module_id: ModuleId,
        caller_info: CallerInfo,
        is_other_module: bool,
    ) -> CheckResult {
        // In general, top level names are searched in this order:
        // > 1. A binding in current module
        // > 2. The `self` module
        // > 3. The `super` module
        // > 4. A library name
        // > 5. A built-in type name
        // > 6. A binding in `std` prelude

        if let Some(result) = self.find_checked_top_level_name(name, module_id, caller_info) {
            result
        } else {
            let module = self
                .modules
                .iter()
                .find(|m| m.id == module_id)
                .unwrap_or_else(|| panic!("{:?}", module_id));

            // Top level name in the searched module
            match self.check_name_in_module(name, module, caller_info) {
                Some(Ok(node)) => Ok(node),
                Some(Err(diag)) => Err(diag),
                None => {
                    match name.as_str() {
                        // Top level `self` module
                        sym::SELF => Ok(self.module_node(module_id, caller_info.span)),
                        // Top level `super` module
                        sym::SUPER => self.super_node_module(&module.info, caller_info),
                        _ => {
                            if is_other_module {
                                return Err(self.name_not_found_error(module_id, name, caller_info));
                            }

                            // A used library name
                            let find_library_result = self
                                .workspace
                                .libraries
                                .iter()
                                .find(|(_, library)| library.name == name);

                            if let Some((_, library)) = find_library_result {
                                let module_type = self.check_module_by_id(library.root_module_id)?;

                                Ok(hir::Node::Const(hir::Const {
                                    value: ConstValue::Unit(()),
                                    ty: module_type,
                                    span: caller_info.span,
                                }))
                            } else if let Some(ty) = self.get_builtin_type(&name) {
                                // A built-in type
                                let value = ConstValue::Type(ty);
                                let ty = self.tcx.bound_maybe_spanned(ty.as_kind().create_type(), None);

                                Ok(hir::Node::Const(hir::Const {
                                    value,
                                    ty,
                                    span: caller_info.span,
                                }))
                            } else if let Some(result) = self.check_name_in_std_prelude(name, caller_info) {
                                // Top level name in the `std` prelude
                                result
                            } else {
                                Err(self.name_not_found_error(module_id, name, caller_info))
                            }
                        }
                    }
                }
            }
        }
    }

    // Note(Ron): This function is pretty weird, maybe we should yeet it?
    fn find_checked_top_level_name(
        &mut self,
        name: Ustr,
        module_id: ModuleId,
        caller_info: CallerInfo,
    ) -> Option<CheckResult> {
        if let Some(id) = self.get_global_binding_id(module_id, name) {
            self.workspace.add_binding_info_use(id, caller_info.span);

            if let Err(diag) = self.validate_item_vis(id, caller_info) {
                Some(Err(diag))
            } else {
                Some(Ok(self.id_or_const_by_id(id, caller_info.span)))
            }
        } else {
            None
        }
    }

    fn check_name_in_module(
        &mut self,
        name: Ustr,
        module: &ast::Module,
        caller_info: CallerInfo,
    ) -> Option<CheckResult> {
        let (index, binding) = module.find_binding(name)?;

        // Check that this binding isn't cyclic
        if !self.encountered_items.insert((module.id, index)) {
            return Some(Err(Diagnostic::error()
                .with_message(format!(
                    "cycle detected while checking `{}` in module `{}`",
                    name, module.info.qualified_name
                ))
                .with_label(Label::primary(caller_info.span, format!("`{}` refers to itself", name)))
                .with_label(Label::secondary(
                    binding.pat_span(),
                    format!("`{}` is defined here", name),
                ))));
        }

        self.queued_modules
            .get_mut(&module.id)
            .unwrap()
            .queued_bindings
            .insert(index);

        match binding.check_top_level(self, module.id) {
            Ok(bound_names) => {
                let desired_id = *bound_names.get(&name).unwrap();

                self.workspace.add_binding_info_use(desired_id, caller_info.span);
                match self.validate_item_vis(desired_id, caller_info) {
                    Ok(_) => {
                        self.encountered_items.remove(&(module.id, index));
                        Some(Ok(self.id_or_const_by_id(desired_id, caller_info.span)))
                    }
                    Err(diag) => Some(Err(diag)),
                }
            }
            Err(diag) => Some(Err(diag)),
        }
    }

    fn check_name_in_std_prelude(&mut self, name: Ustr, caller_info: CallerInfo) -> Option<CheckResult> {
        let std_root_module_id = self.workspace.std_library().root_module_id;

        if let Some(result) = self.find_checked_top_level_name(name, std_root_module_id, caller_info) {
            Some(result)
        } else {
            let std_root_module = self
                .modules
                .iter()
                .find(|m| m.id == std_root_module_id)
                .unwrap_or_else(|| panic!("{:?}", std_root_module_id));

            self.check_name_in_module(name, std_root_module, caller_info)
        }
    }

    pub(super) fn name_not_found_error(&self, module_id: ModuleId, name: Ustr, caller_info: CallerInfo) -> Diagnostic {
        let module_info = self.workspace.module_infos.get(module_id).unwrap();

        let message = if module_info.qualified_name.is_empty() {
            format!("cannot find value `{}` in this scope", name)
        } else {
            format!(
                "cannot find value `{}` in module `{}`",
                name, module_info.qualified_name
            )
        };

        let label_message = if module_info.qualified_name.is_empty() {
            "not found in this scope".to_string()
        } else {
            format!("not found in `{}`", module_info.qualified_name)
        };

        Diagnostic::error()
            .with_message(message)
            .with_label(Label::primary(caller_info.span, label_message))
    }

    pub fn validate_item_vis(&self, id: BindingId, caller_info: CallerInfo) -> CheckResult<()> {
        let binding_info = self.workspace.binding_infos.get(id).unwrap();

        if binding_info.vis == ast::Vis::Private && binding_info.module_id != caller_info.module_id {
            Err(Diagnostic::error()
                .with_message(format!("symbol `{}` is private", binding_info.name))
                .with_label(Label::primary(caller_info.span, "accessed here"))
                .with_label(Label::secondary(binding_info.span, "defined here")))
        } else {
            Ok(())
        }
    }

    pub fn check_module_by_id(&mut self, id: ModuleId) -> CheckResult<TypeId> {
        let module = self
            .modules
            .iter()
            .find(|m| m.id == id)
            .unwrap_or_else(|| panic!("couldn't find {:?}", id));

        self.check_module(module)
    }

    pub fn check_module(&mut self, module: &ast::Module) -> CheckResult<TypeId> {
        if let Some(ty) = self.get_completed_module_type(module.id) {
            Ok(ty)
        } else {
            let module_type = match self.queued_modules.get(&module.id) {
                Some(queued) => queued.module_type,
                None => {
                    let span = Span::initial(module.file_id);
                    let module_type = self.tcx.bound(Type::Module(module.id), span);

                    // Add the module to the queued modules map
                    self.queued_modules.insert(
                        module.id,
                        QueuedModule {
                            module_type,
                            all_complete: false,
                            queued_bindings: HashSet::new(),
                            queued_comptime: HashSet::new(),
                        },
                    );

                    module_type
                }
            };

            for (index, binding) in module.bindings.iter().enumerate() {
                if self
                    .queued_modules
                    .get_mut(&module.id)
                    .unwrap()
                    .queued_bindings
                    .insert(index)
                {
                    binding.check_top_level(self, module.id)?;
                }
            }

            self.queued_modules.get_mut(&module.id).unwrap().all_complete = true;

            for (index, comptime) in module.comptime_blocks.iter().enumerate() {
                if self
                    .queued_modules
                    .get_mut(&module.id)
                    .unwrap()
                    .queued_comptime
                    .insert(index)
                {
                    let node = self.with_env(module.id, |sess, mut env| comptime.check(sess, &mut env, None))?;

                    if !self.workspace.build_options.check_mode {
                        self.eval(&node, module.id, comptime.span)?;
                    }
                }
            }

            Ok(module_type)
        }
    }

    fn get_module_type(&self, id: ModuleId) -> TypeId {
        self.queued_modules.get(&id).unwrap().module_type
    }

    fn get_completed_module_type(&self, id: ModuleId) -> Option<TypeId> {
        match self.queued_modules.get(&id) {
            Some(QueuedModule {
                module_type,
                all_complete: true,
                ..
            }) => Some(*module_type),
            _ => None,
        }
    }

    fn get_builtin_type(&self, name: &str) -> Option<TypeId> {
        match name {
            sym::UNIT => Some(self.tcx.common_types.unit),
            sym::BOOL => Some(self.tcx.common_types.bool),

            sym::I8 => Some(self.tcx.common_types.i8),
            sym::I16 => Some(self.tcx.common_types.i16),
            sym::I32 => Some(self.tcx.common_types.i32),
            sym::I64 => Some(self.tcx.common_types.i64),
            sym::INT => Some(self.tcx.common_types.int),

            sym::U8 => Some(self.tcx.common_types.u8),
            sym::U16 => Some(self.tcx.common_types.u16),
            sym::U32 => Some(self.tcx.common_types.u32),
            sym::U64 => Some(self.tcx.common_types.u64),
            sym::UINT => Some(self.tcx.common_types.uint),

            sym::F16 => Some(self.tcx.common_types.f16),
            sym::F32 => Some(self.tcx.common_types.f32),
            sym::F64 => Some(self.tcx.common_types.f64),
            sym::FLOAT => Some(self.tcx.common_types.float),

            sym::NEVER => Some(self.tcx.common_types.never),

            sym::STR => Some(self.tcx.common_types.str),

            _ => None,
        }
    }

    pub fn module_node(&self, module_id: ModuleId, span: Span) -> hir::Node {
        hir::Node::Const(hir::Const {
            value: ConstValue::Unit(()),
            ty: self.get_module_type(module_id),
            span,
        })
    }

    pub fn super_node_module(&mut self, module_info: &ModuleInfo, caller_info: CallerInfo) -> CheckResult {
        if let Some(parent_module_id) = module_info.parent {
            Ok(self.module_node(parent_module_id, caller_info.span))
        } else {
            Err(Diagnostic::error()
                .with_message(format!("module `{}` has no parent", module_info.qualified_name))
                .with_label(Label::primary(caller_info.span, "invalid `super` module")))
        }
    }
}

trait CheckTopLevel
where
    Self: Sized,
{
    fn check_top_level(&self, sess: &mut CheckSess, module_id: ModuleId) -> CheckResult<UstrMap<BindingId>>;
}

impl CheckTopLevel for ast::Binding {
    fn check_top_level(&self, sess: &mut CheckSess, module_id: ModuleId) -> CheckResult<UstrMap<BindingId>> {
        let node = sess.with_env(module_id, |sess, mut env| self.check(sess, &mut env, None))?;

        if let Err(mut diagnostics) = substitute_node(&node, &mut sess.tcx) {
            let last = diagnostics.pop().unwrap();
            sess.workspace.diagnostics.extend(diagnostics);
            return Err(last);
        }

        fn collect_bound_names(node: hir::Node, bound_names: &mut UstrMap<BindingId>, sess: &mut CheckSess) {
            match node {
                hir::Node::Binding(binding) => {
                    let (name, id) = (binding.name, binding.id);
                    sess.cache.bindings.insert(id, binding);
                    bound_names.insert(name, id);
                }
                hir::Node::Sequence(sequence) => {
                    sequence.statements.into_iter().for_each(|statement| {
                        collect_bound_names(statement, bound_names, sess);
                    });
                }
                _ => unreachable!("{:#?}", node),
            }
        }

        let mut bound_names = UstrMap::<BindingId>::default();
        collect_bound_names(node, &mut bound_names, sess);

        Ok(bound_names)
    }
}
