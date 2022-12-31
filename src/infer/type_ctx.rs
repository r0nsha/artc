use super::{display::DisplayType, inference_value::InferenceValue, normalize::Normalize};
use crate::{
    common::id_cache::IdCache,
    span::Span,
    types::{InferType, StructType, Type, TypeId},
};

pub struct TypeCtx {
    pub bindings: IdCache<TypeId, InferenceValue>,
    pub binding_spans: IdCache<TypeId, Option<Span>>,
    pub common_types: CommonTypes,
}

impl Default for TypeCtx {
    fn default() -> Self {
        let mut bindings = IdCache::new();
        let mut binding_spans = IdCache::new();
        let common_types = CommonTypes::new(&mut bindings, &mut binding_spans);
        Self {
            bindings,
            binding_spans,
            common_types,
        }
    }
}

impl TypeCtx {
    #[inline]
    fn insert(&mut self, binding: InferenceValue, span: Option<Span>) -> TypeId {
        self.binding_spans.insert(span);
        self.bindings.insert(binding)
    }

    #[inline]
    pub fn var(&mut self, span: Span) -> TypeId {
        self.insert(InferenceValue::Unbound, Some(span))
    }

    #[inline]
    pub fn anyint(&mut self, span: Span) -> TypeId {
        self.insert(InferenceValue::AnyInt, Some(span))
    }

    #[inline]
    pub fn anyfloat(&mut self, span: Span) -> TypeId {
        self.insert(InferenceValue::AnyFloat, Some(span))
    }

    #[inline]
    pub fn bound_maybe_spanned(&mut self, kind: Type, span: Option<Span>) -> TypeId {
        match kind {
            Type::Var(ty) | Type::Infer(ty, _) => ty,
            _ => self.insert(InferenceValue::Bound(kind), span),
        }
    }

    #[inline]
    pub fn bound(&mut self, kind: Type, span: Span) -> TypeId {
        match kind {
            Type::Var(ty) | Type::Infer(ty, _) => ty,
            _ => self.insert(InferenceValue::Bound(kind), Some(span)),
        }
    }

    #[inline]
    pub fn value_of(&self, id: TypeId) -> &InferenceValue {
        match self.bindings.get(id) {
            Some(value) => value,
            None => &InferenceValue::Unbound,
        }
    }

    #[allow(unused)]
    #[inline]
    pub fn normalize(&self, ty: TypeId) -> Type {
        ty.normalize(self)
    }

    #[inline]
    pub fn ty_span(&self, ty: TypeId) -> Option<Span> {
        self.binding_spans.get(ty).cloned().flatten()
    }

    #[inline]
    pub fn bind_ty(&mut self, id: TypeId, ty: Type) {
        self.bind_value(id, InferenceValue::Bound(ty))
    }

    #[inline]
    pub fn bind_value(&mut self, id: TypeId, value: InferenceValue) {
        *self
            .bindings
            .get_mut(id)
            .unwrap_or_else(|| panic!("type id not found: {:?}", id)) = value;
    }

    #[allow(unused)]
    pub fn make_concrete(&mut self, ty: &mut Type) {
        match ty {
            Type::Infer(id, InferType::AnyInt) => {
                let concrete = Type::int();
                self.bind_ty(*id, concrete.clone());
                *ty = concrete;
            }

            Type::Infer(id, InferType::AnyFloat) => {
                let concrete = Type::float();
                self.bind_ty(*id, concrete.clone());
                *ty = concrete;
            }

            Type::Function(f) => {
                f.params.iter_mut().for_each(|p| self.make_concrete(&mut p.ty));

                self.make_concrete(&mut f.return_type);

                if let Some(ty) = f.varargs.as_mut().and_then(|v| v.ty.as_mut()) {
                    self.make_concrete(ty);
                }
            }

            Type::Pointer(ty, _) | Type::Array(ty, _) | Type::Slice(ty) | Type::Str(ty) | Type::Type(ty) => {
                self.make_concrete(ty)
            }

            Type::Tuple(tys) => tys.iter_mut().for_each(|t| self.make_concrete(t)),

            Type::Struct(StructType { fields, .. }) => {
                fields.iter_mut().for_each(|f| self.make_concrete(&mut f.ty));
            }

            Type::Never
            | Type::Unit
            | Type::Bool
            | Type::Int(_)
            | Type::Uint(_)
            | Type::Float(_)
            | Type::Module(_)
            | Type::AnyType
            | Type::Var(_) => (),
        }
    }

    #[allow(unused)]
    pub fn print_all_bindings(&self, only_concrete: bool) {
        for (i, b) in self.bindings.iter() {
            if !only_concrete || b.is_concrete() {
                println!("'{} :: {}", i, b.display(self))
            }
        }
    }

    #[allow(unused)]
    pub fn print_binding(&self, ty: TypeId) {
        println!("'{} :: {}", ty.inner(), self.bindings[ty].display(self));
    }
}

pub struct CommonTypes {
    pub unit: TypeId,
    pub bool: TypeId,
    pub i8: TypeId,
    pub i16: TypeId,
    pub i32: TypeId,
    pub i64: TypeId,
    pub int: TypeId,
    pub u8: TypeId,
    pub u16: TypeId,
    pub u32: TypeId,
    pub u64: TypeId,
    pub uint: TypeId,
    pub f16: TypeId,
    pub f32: TypeId,
    pub f64: TypeId,
    pub float: TypeId,
    pub str: TypeId,
    pub str_pointer: TypeId,
    pub never: TypeId,
    pub anytype: TypeId,
}

impl CommonTypes {
    pub fn new(
        bindings: &mut IdCache<TypeId, InferenceValue>,
        binding_spans: &mut IdCache<TypeId, Option<Span>>,
    ) -> Self {
        let mut mk = |kind| {
            binding_spans.insert(None);
            bindings.insert(InferenceValue::Bound(kind))
        };

        Self {
            unit: mk(Type::Unit),
            bool: mk(Type::Bool),
            i8: mk(Type::i8()),
            i16: mk(Type::i16()),
            i32: mk(Type::i32()),
            i64: mk(Type::i64()),
            int: mk(Type::int()),
            u8: mk(Type::u8()),
            u16: mk(Type::u16()),
            u32: mk(Type::u32()),
            u64: mk(Type::u64()),
            uint: mk(Type::uint()),
            f16: mk(Type::f16()),
            f32: mk(Type::f32()),
            f64: mk(Type::f64()),
            float: mk(Type::float()),
            str: mk(Type::str()),
            str_pointer: mk(Type::str_pointer()),
            never: mk(Type::Never),
            anytype: mk(Type::AnyType),
        }
    }
}
