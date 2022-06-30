use super::{
    codegen::{FunctionState, Generator},
    ty::IntoLlvmType,
};
use crate::{
    ast::{
        self,
        pattern::{HybridPattern, Pattern, UnpackPattern, UnpackPatternKind},
    },
    infer::normalize::Normalize,
    types::*,
};
use inkwell::{
    module::Linkage,
    values::{BasicValue, PointerValue},
    AddressSpace,
};
use ustr::ustr;

impl<'g, 'ctx> Generator<'g, 'ctx> {
    pub fn gen_entry_point_function(&mut self) {
        let entry_point_func_id = self.workspace.entry_point_function_id.unwrap();

        let entry_point_func_info = self
            .workspace
            .binding_infos
            .get(entry_point_func_id)
            .unwrap();

        let entry_point_func = self
            .global_decls
            .get(&entry_point_func_id)
            .unwrap()
            .into_function_value();

        let fn_ty = entry_point_func_info
            .ty
            .normalize(self.tycx)
            .into_function();

        let name = self
            .workspace
            .build_options
            .entry_point_function_name()
            .unwrap();

        let linkage = Some(Linkage::External);

        // let function = if os == Windows && BuildMode == DLL {
        //     self.module.add_function(
        //         "DllMain",
        //         ret_type.fn_type(&[
        //             "hinstDLL" -> rawptr,
        //             "fdwReason" -> u32,
        //             "lpReserved" -> rawptr,
        //         ], false),
        //         linkage,
        //     )
        // } else if (build_context.metrics.os == TargetOs_windows &&
        // (build_context.metrics.arch == TargetArch_386 ||
        // build_context.no_crt)) {     self.module.add_function(
        //         "mainCRTStartup",
        //         ret_type.fn_type(&[], false),
        //         linkage,
        //     )
        // } else if (is_arch_wasm()) {
        //     self.module.add_function(
        //         "_start",
        //         ret_type.fn_type(&[], false),
        //         linkage,
        //     )
        // } else {
        //     self.module.add_function(
        //         "main",
        //         ret_type.fn_type(&[
        //             self.context.i32_type(),
        //
        // self.context.i8_type().ptr_type(AddressSpace::Generic).
        // ptr_type(AddressSpace::Generic)         ], false),
        //         linkage,
        //     )
        // };
        let startup_fn_type = FunctionType {
            params: vec![
                FunctionTypeParam {
                    name: ustr("argc"),
                    ty: Type::Uint(UintType::U32),
                },
                FunctionTypeParam {
                    name: ustr("argv"),
                    ty: Type::Uint(UintType::U8)
                        .pointer_type(false)
                        .pointer_type(false),
                },
            ],
            return_type: Box::new(Type::Uint(UintType::U32)),
            varargs: None,
            kind: FunctionTypeKind::Orphan,
        };

        let function = self.module.add_function(
            name,
            self.context.i32_type().fn_type(
                &[
                    self.context.i32_type().into(),
                    self.context
                        .i8_type()
                        .ptr_type(AddressSpace::Generic)
                        .ptr_type(AddressSpace::Generic)
                        .into(),
                ],
                false,
            ),
            linkage,
        );

        function.get_nth_param(0).unwrap().set_name("argc");
        function.get_nth_param(1).unwrap().set_name("argv");

        let decl_block = self.context.append_basic_block(function, "decls");
        let entry_block = self.context.append_basic_block(function, "entry");

        let root_module_info = self.workspace.get_root_module_info();

        let mut state = FunctionState::new(
            *root_module_info,
            function,
            startup_fn_type,
            None,
            decl_block,
            entry_block,
        );

        state.push_scope();

        self.start_block(&mut state, entry_block);

        // we initialize the runtime known global bindings at the start of the program
        self.initialize_globals(&mut state);

        self.gen_fn_call(
            &mut state,
            entry_point_func,
            &fn_ty,
            vec![],
            &fn_ty.return_type,
        );

        // TODO: if this is DLL Main, return 1 instead of 0

        if self.current_block().get_terminator().is_none() {
            self.builder
                .build_return(Some(&self.context.i32_type().const_zero()));
        }

        self.start_block(&mut state, decl_block);

        state.pop_scope();

        self.builder.build_unconditional_branch(entry_block);
    }

    fn initialize_globals(&mut self, state: &mut FunctionState<'ctx>) {
        for (_, binding) in self.cache.bindings.iter() {
            // if all patterns are const, then there is no value to generate at runtime - so we skip
            todo!()
            // if binding.pattern.iter().all(|p| !p.is_mutable) {
            //     continue;
            // }

            // let ty = binding.ty.llvm_type(self);

            // let global_value = match &binding.pattern {
            //     Pattern::Name(pat)
            //     | Pattern::Hybrid(HybridPattern {
            //         name_pattern: pat, ..
            //     }) => self.global_decls.get(&pat.id).unwrap().into_global_value(),
            //     Pattern::StructUnpack(_) | Pattern::TupleUnpack(_) => {
            //         let global_value = self.module.add_global(ty, None, "");
            //         global_value.set_linkage(Linkage::Private);
            //         global_value
            //     }
            // };

            // let old_module_info = state.module_info;
            // state.module_info = *self.workspace.module_infos.get(binding.module_id).unwrap();

            // let value = self.gen_expr(state, &binding.value, true);

            // state.module_info = old_module_info;

            // let is_const = matches!(binding.value.as_ref(), ast::Ast::Const(..));

            // let initializer = if is_const { value } else { ty.const_zero() };

            // global_value.set_initializer(&initializer);

            // let global_ptr = global_value.as_pointer_value();

            // match &binding.pattern {
            //     Pattern::Name(_) => {
            //         if !is_const {
            //             self.build_store(global_ptr, value);
            //         }
            //     }
            //     Pattern::StructUnpack(pattern) => {
            //         self.initialize_global_struct_unpack(binding, pattern, global_ptr);
            //     }
            //     Pattern::TupleUnpack(pattern) => {
            //         self.initialize_global_tuple_unpack(binding, pattern, global_ptr);
            //     }
            //     Pattern::Hybrid(pattern) => {
            //         if !is_const {
            //             self.build_store(global_ptr, value);
            //         }

            //         match &pattern.unpack_pattern {
            //             UnpackPatternKind::Struct(pattern) => {
            //                 self.initialize_global_struct_unpack(binding, pattern, global_ptr)
            //             }
            //             UnpackPatternKind::Tuple(pattern) => {
            //                 self.initialize_global_tuple_unpack(binding, pattern, global_ptr)
            //             }
            //         }
            //     }
            // }
        }
    }

    fn initialize_global_struct_unpack(
        &mut self,
        binding: &ast::Binding,
        pattern: &UnpackPattern,
        global_ptr: PointerValue<'ctx>,
    ) {
        let ty = binding.ty.normalize(self.tycx);
        let struct_ty = ty.maybe_deref_once().as_struct().clone();

        let struct_llvm_type = Some(ty.llvm_type(self));

        for pattern in pattern.symbols.iter() {
            let binding_info = self.workspace.binding_infos.get(pattern.id).unwrap();

            if binding_info.const_value.is_some() {
                continue;
            }

            if let Some(ptr) = self
                .global_decls
                .get(&pattern.id)
                .map(|d| d.into_pointer_value())
            {
                let field_index = struct_ty.find_field_position(pattern.name).unwrap();

                let field_value =
                    self.gen_struct_access(global_ptr.into(), field_index as u32, struct_llvm_type);

                self.build_store(ptr, self.build_load(field_value));
            }
        }
    }

    fn initialize_global_tuple_unpack(
        &mut self,
        binding: &ast::Binding,
        pattern: &UnpackPattern,
        global_ptr: PointerValue<'ctx>,
    ) {
        let ty = binding.ty.normalize(self.tycx);

        let llvm_type = Some(ty.llvm_type(self));

        for (i, pattern) in pattern.symbols.iter().enumerate() {
            let binding_info = self.workspace.binding_infos.get(pattern.id).unwrap();

            if binding_info.const_value.is_some() {
                continue;
            }

            if let Some(ptr) = self
                .global_decls
                .get(&pattern.id)
                .map(|d| d.into_pointer_value())
            {
                let field_value = self.gen_struct_access(global_ptr.into(), i as u32, llvm_type);

                self.build_store(ptr, self.build_load(field_value));
            }
        }
    }
}
