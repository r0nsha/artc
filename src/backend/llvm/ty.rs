use super::{
    abi::{self, AbiFunction, AbiType},
    codegen::Generator,
};
use crate::{
    infer::{display::DisplayType, normalize::Normalize},
    types::{size_of::SizeOf, *},
};
use inkwell::{
    types::{AnyType, BasicMetadataTypeEnum, BasicType, BasicTypeEnum, PointerType},
    AddressSpace,
};
use std::cmp::Ordering;

pub(super) trait IntoLlvmType<'g, 'ctx> {
    fn llvm_type(&self, generator: &mut Generator<'g, 'ctx>) -> BasicTypeEnum<'ctx>;
}

impl<'g, 'ctx> IntoLlvmType<'g, 'ctx> for TypeId {
    fn llvm_type(&self, generator: &mut Generator<'g, 'ctx>) -> BasicTypeEnum<'ctx> {
        let kind = self.normalize(generator.tcx);
        kind.llvm_type(generator)
    }
}

impl<'g, 'ctx> IntoLlvmType<'g, 'ctx> for Type {
    fn llvm_type(&self, generator: &mut Generator<'g, 'ctx>) -> BasicTypeEnum<'ctx> {
        match self {
            Type::Bool => generator.context.bool_type().into(),
            Type::Int(inner) => match inner {
                IntType::I8 => generator.context.i8_type().into(),
                IntType::I16 => generator.context.i16_type().into(),
                IntType::I32 => generator.context.i32_type().into(),
                IntType::I64 => generator.context.i64_type().into(),
                IntType::Int => generator.ptr_sized_int_type.into(),
            },
            Type::Uint(inner) => match inner {
                UintType::U8 => generator.context.i8_type().into(),
                UintType::U16 => generator.context.i16_type().into(),
                UintType::U32 => generator.context.i32_type().into(),
                UintType::U64 => generator.context.i64_type().into(),
                UintType::Uint => generator.ptr_sized_int_type.into(),
            },
            Type::Float(inner) => match inner {
                FloatType::F16 => generator.context.f16_type().into(),
                FloatType::F32 => generator.context.f32_type().into(),
                FloatType::F64 => generator.context.f64_type().into(),
                FloatType::Float => if generator.target_metrics.word_size == 8 {
                    generator.context.f64_type()
                } else {
                    generator.context.f32_type()
                }
                .into(),
            },
            Type::Pointer(inner, _) => inner.llvm_type(generator).ptr_type(AddressSpace::Generic).into(),
            Type::Slice(inner) | Type::Str(inner) => generator.slice_type(inner).into(),
            Type::Type(_) | Type::Unit | Type::Module { .. } => generator.unit_type(),
            Type::Never => generator.never_type(),
            Type::Function(func) => generator
                .abi_compliant_fn_type(func)
                .ptr_type(AddressSpace::Generic)
                .into(),
            Type::Array(inner, size) => inner.llvm_type(generator).array_type(*size as u32).into(),
            Type::Tuple(tys) => generator
                .context
                .struct_type(
                    &tys.iter()
                        .map(|ty| ty.llvm_type(generator))
                        .collect::<Vec<BasicTypeEnum>>(),
                    false,
                )
                .into(),
            Type::Struct(struct_type) => {
                let struct_type = if struct_type.id.is_some() {
                    generator.get_or_create_named_struct_type(struct_type)
                } else {
                    generator.create_anonymous_struct_type(struct_type)
                };

                struct_type.into()
            }
            _ => {
                panic!("bug: type `{}` in llvm codegen", self.display(generator.tcx))
            }
        }
    }
}

impl<'g, 'ctx> Generator<'g, 'ctx> {
    pub(super) fn unit_type(&self) -> BasicTypeEnum<'ctx> {
        self.context.struct_type(&[], false).into()
    }

    pub(super) fn never_type(&self) -> BasicTypeEnum<'ctx> {
        self.unit_type()
    }

    pub(super) fn raw_pointer_type(&self) -> PointerType<'ctx> {
        self.context.i8_type().ptr_type(AddressSpace::Generic)
    }

    pub(super) fn slice_type(&mut self, elem_type: &Type) -> inkwell::types::StructType<'ctx> {
        self.fat_pointer_type(elem_type, &Type::uint())
    }

    pub(super) fn fat_pointer_type(
        &mut self,
        elem_type: &Type,
        metadata_type: &Type,
    ) -> inkwell::types::StructType<'ctx> {
        self.context.struct_type(
            &[
                elem_type.llvm_type(self).ptr_type(AddressSpace::Generic).into(),
                metadata_type.llvm_type(self),
            ],
            false,
        )
    }

    pub(super) fn fn_type(&mut self, f: &FunctionType) -> inkwell::types::FunctionType<'ctx> {
        let mut params: Vec<BasicMetadataTypeEnum> = f.params.iter().map(|p| p.ty.llvm_type(self).into()).collect();
        let ret = f.return_type.llvm_type(self);

        match &f.varargs {
            Some(varargs) => match &varargs.ty {
                Some(ty) => {
                    params.push(self.slice_type(ty).ptr_type(AddressSpace::Generic).into());
                    ret.fn_type(&params, false)
                }
                None => ret.fn_type(&params, true),
            },
            None => ret.fn_type(&params, false),
        }
    }

    pub(super) fn get_abi_compliant_fn(&mut self, f: &FunctionType) -> AbiFunction<'ctx> {
        let fn_type = self.fn_type(f);
        abi::get_abi_compliant_fn(self.context, &self.target_metrics, fn_type)
    }

    pub(super) fn abi_compliant_fn_type(&mut self, f: &FunctionType) -> inkwell::types::FunctionType<'ctx> {
        let abi_compliant_fn_ty = self.get_abi_compliant_fn(f);
        self.abi_fn_to_type(&abi_compliant_fn_ty)
    }

    pub(super) fn abi_fn_to_type(&mut self, abi_fn: &AbiFunction<'ctx>) -> inkwell::types::FunctionType<'ctx> {
        let ret = match abi_fn.ret.kind {
            AbiType::Direct => match abi_fn.ret.cast_to {
                Some(cast_to) => cast_to,
                None => abi_fn.ret.ty,
            }
            .as_any_type_enum(),
            AbiType::Indirect | AbiType::Ignore => self.context.void_type().into(),
        };

        let mut params: Vec<BasicMetadataTypeEnum> = vec![];

        if abi_fn.ret.kind.is_indirect() {
            params.push(abi_fn.ret.ty.ptr_type(AddressSpace::Generic).into());
        }

        for param in abi_fn.params.iter() {
            let ty = match &param.kind {
                AbiType::Direct => match param.cast_to {
                    Some(cast_to) => cast_to,
                    None => param.ty,
                },
                AbiType::Indirect => param.ty.ptr_type(AddressSpace::Generic).into(),
                AbiType::Ignore => unimplemented!("ignore '{:?}'", param.ty),
            };

            params.push(ty.into());
        }

        if ret.is_void_type() {
            ret.into_void_type().fn_type(&params, abi_fn.variadic)
        } else {
            let ret: BasicTypeEnum = ret.try_into().unwrap();
            ret.fn_type(&params, abi_fn.variadic)
        }
    }

    fn get_or_create_named_struct_type(&mut self, struct_type: &StructType) -> inkwell::types::StructType<'ctx> {
        match self.types.get(&struct_type.id.unwrap()) {
            Some(t) => t.into_struct_type(),
            None => self.create_named_struct_type(struct_type),
        }
    }

    fn create_named_struct_type(&mut self, struct_ty: &StructType) -> inkwell::types::StructType<'ctx> {
        let struct_type = self.context.opaque_struct_type(&struct_ty.name);

        self.types.insert(struct_ty.id.unwrap(), struct_type.into());

        let fields = self.create_struct_type_fields(struct_ty);
        struct_type.set_body(&fields, struct_ty.is_packed_struct());
        struct_type
    }

    fn create_anonymous_struct_type(&mut self, struct_ty: &StructType) -> inkwell::types::StructType<'ctx> {
        let fields = self.create_struct_type_fields(struct_ty);
        self.context.struct_type(&fields, struct_ty.is_packed_struct())
    }

    fn create_struct_type_fields(&mut self, struct_ty: &StructType) -> Vec<BasicTypeEnum<'ctx>> {
        if struct_ty.fields.is_empty() {
            vec![]
        } else if struct_ty.is_union() {
            let largest_field = struct_ty
                .fields
                .iter()
                .max_by(|f1, f2| {
                    let s1 = f1.ty.size_of(self.target_metrics.word_size);
                    let s2 = f2.ty.size_of(self.target_metrics.word_size);
                    if s1 > s2 {
                        Ordering::Greater
                    } else if s1 == s2 {
                        Ordering::Equal
                    } else {
                        Ordering::Less
                    }
                })
                .unwrap();

            let field_ty = largest_field.ty.llvm_type(self);

            vec![field_ty]
        } else {
            struct_ty
                .fields
                .iter()
                .map(|f| f.ty.llvm_type(self))
                .collect::<Vec<BasicTypeEnum>>()
        }
    }
}
