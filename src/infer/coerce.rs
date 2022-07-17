use super::{
    normalize::Normalize,
    type_ctx::TypeCtx,
    unify::{can_coerce_mut, UnifyTypeResult},
};
use crate::hir;
use crate::types::{size_of::SizeOf, *};

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum CoercionResult {
    CoerceToLeft,
    CoerceToRight,
    NoCoercion,
}

trait Coerce {
    fn coerce(&self, to: &Type, word_size: usize) -> CoercionResult;
}

impl Coerce for Type {
    fn coerce(&self, to: &Type, word_size: usize) -> CoercionResult {
        use CoercionResult::*;

        match (self, to) {
            (Type::Infer(_, InferType::AnyInt), Type::Infer(_, InferType::AnyFloat)) => {
                CoerceToRight
            }
            (Type::Infer(_, InferType::AnyFloat), Type::Infer(_, InferType::AnyInt)) => {
                CoerceToLeft
            }

            // * int -> same or bigger int
            (Type::Int(left), Type::Int(right)) => {
                if left.size_of(word_size) <= right.size_of(word_size) {
                    CoerceToRight
                } else {
                    CoerceToLeft
                }
            }

            // * int -> same or bigger uint
            (Type::Int(left), Type::Uint(right)) => {
                if left.size_of(word_size) <= right.size_of(word_size) {
                    CoerceToRight
                } else {
                    CoerceToLeft
                }
            }

            // * uint -> same or bigger uint
            (Type::Uint(left), Type::Uint(right)) => {
                if left.size_of(word_size) <= right.size_of(word_size) {
                    CoerceToRight
                } else {
                    CoerceToLeft
                }
            }

            // * uint -> same or bigger int
            (Type::Uint(left), Type::Int(right)) => {
                if left.size_of(word_size) <= right.size_of(word_size) {
                    CoerceToRight
                } else {
                    CoerceToLeft
                }
            }

            // * float -> same or bigger float
            (Type::Float(left), Type::Float(right)) => {
                if left.size_of(word_size) <= right.size_of(word_size) {
                    CoerceToRight
                } else {
                    CoerceToLeft
                }
            }

            // * array[N] of T -> slice of T
            (Type::Pointer(left, lmut), Type::Pointer(right, rmut))
                if can_coerce_mut(*lmut, *rmut) =>
            {
                match (left.as_ref(), right.as_ref()) {
                    (Type::Array(t_array, _), Type::Slice(t_slice, _)) => {
                        if t_array == t_slice {
                            CoerceToRight
                        } else {
                            NoCoercion
                        }
                    }
                    _ => NoCoercion,
                }
            }

            // * array[N] of T -> pointer of T
            (Type::Pointer(t, lmut), Type::Pointer(t_ptr, rmut))
                if can_coerce_mut(*lmut, *rmut) =>
            {
                match t.as_ref() {
                    Type::Array(t_array, ..) => {
                        if t_array == t_ptr {
                            CoerceToRight
                        } else {
                            NoCoercion
                        }
                    }
                    _ => NoCoercion,
                }
            }

            _ => NoCoercion,
        }
    }
}

fn coerce_node(tcx: &mut TypeCtx, node: &mut hir::Node, to: Type) {
    *node = hir::Node::Cast(hir::Cast {
        value: Box::new(node.clone()),
        ty: tcx.bound(to, node.span()),
        span: node.span(),
    })
}

pub trait OrCoerce {
    fn or_coerce(
        self,
        left: &mut hir::Node,
        right: &mut hir::Node,
        tcx: &mut TypeCtx,
        word_size: usize,
    ) -> UnifyTypeResult;
}

impl OrCoerce for UnifyTypeResult {
    fn or_coerce(
        self,
        left: &mut hir::Node,
        right: &mut hir::Node,
        tcx: &mut TypeCtx,
        word_size: usize,
    ) -> UnifyTypeResult {
        match self {
            Ok(r) => Ok(r),
            Err(e) => {
                let (left_ty, right_ty) = (left.ty().normalize(tcx), right.ty().normalize(tcx));
                match left_ty.coerce(&right_ty, word_size) {
                    CoercionResult::CoerceToLeft => {
                        coerce_node(tcx, right, left_ty);
                        Ok(())
                    }
                    CoercionResult::CoerceToRight => {
                        coerce_node(tcx, left, right_ty);
                        Ok(())
                    }
                    CoercionResult::NoCoercion => Err(e),
                }
            }
        }
    }
}

pub trait OrCoerceIntoTy {
    fn or_coerce_into_ty(
        self,
        node: &mut hir::Node,
        ty: impl Normalize,
        tcx: &mut TypeCtx,
        word_size: usize,
    ) -> UnifyTypeResult;
}

impl OrCoerceIntoTy for UnifyTypeResult {
    fn or_coerce_into_ty(
        self,
        node: &mut hir::Node,
        ty: impl Normalize,
        tcx: &mut TypeCtx,
        word_size: usize,
    ) -> UnifyTypeResult {
        match self {
            Ok(r) => Ok(r),
            Err(e) => {
                let (node_ty, ty) = (node.ty().normalize(tcx), ty.normalize(tcx));
                match node_ty.coerce(&ty, word_size) {
                    CoercionResult::CoerceToRight => {
                        coerce_node(tcx, node, ty);
                        Ok(())
                    }
                    CoercionResult::CoerceToLeft | CoercionResult::NoCoercion => Err(e),
                }
            }
        }
    }
}
