use crate::{
    normalize::NormalizeTy,
    ty_ctx::{InferenceValue, TyCtx},
};
use chili_ast::ty::*;

pub trait UnifyTy<T>
where
    Self: Sized,
    T: Sized,
{
    fn unify(&self, other: &T, tycx: &mut TyCtx) -> UnifyTyResult;
}

impl UnifyTy<Ty> for Ty {
    fn unify(&self, other: &Ty, tycx: &mut TyCtx) -> UnifyTyResult {
        let t1 = TyKind::Var(*self);
        let t2 = TyKind::Var(*other);
        t1.unify(&t2, tycx)
    }
}

impl UnifyTy<TyKind> for Ty {
    fn unify(&self, other: &TyKind, tycx: &mut TyCtx) -> UnifyTyResult {
        let ty = TyKind::Var(*self);
        ty.unify(other, tycx)
    }
}

impl UnifyTy<Ty> for TyKind {
    fn unify(&self, other: &Ty, tycx: &mut TyCtx) -> UnifyTyResult {
        let other = TyKind::Var(*other);
        self.unify(&other, tycx)
    }
}

impl UnifyTy<TyKind> for TyKind {
    fn unify(&self, other: &TyKind, tycx: &mut TyCtx) -> UnifyTyResult {
        match (self, other) {
            (TyKind::Unit, TyKind::Unit) => Ok(()),
            (TyKind::Bool, TyKind::Bool) => Ok(()),

            (TyKind::AnyInt(_), TyKind::AnyInt(_))
            | (TyKind::AnyInt(_), TyKind::AnyFloat(_))
            | (TyKind::AnyFloat(_), TyKind::AnyInt(_))
            | (TyKind::AnyFloat(_), TyKind::AnyFloat(_)) => Ok(()),

            (TyKind::Int(t1), TyKind::Int(t2)) if t1 == t2 => Ok(()),
            (TyKind::UInt(t1), TyKind::UInt(t2)) if t1 == t2 => Ok(()),
            (TyKind::Float(t1), TyKind::Float(t2)) if t1 == t2 => Ok(()),

            (TyKind::Pointer(t1, a1), TyKind::Pointer(t2, a2))
            | (TyKind::MultiPointer(t1, a1), TyKind::MultiPointer(t2, a2))
            | (TyKind::Slice(t1, a1), TyKind::Slice(t2, a2)) => {
                if !can_coerce_mut(*a1, *a2) {
                    Err(UnifyTyErr::Mismatch)
                } else {
                    t1.unify(t2.as_ref(), tycx)?;
                    Ok(())
                }
            }

            (TyKind::Fn(f1), TyKind::Fn(f2)) => {
                f1.ret.unify(f2.ret.as_ref(), tycx)?;

                if f1.params.len() != f2.params.len() && !f1.variadic && !f2.variadic {
                    Err(UnifyTyErr::Mismatch)
                } else {
                    for (p1, p2) in f1.params.iter().zip(f2.params.iter()) {
                        p1.unify(p2, tycx)?;
                    }
                    Ok(())
                }
            }

            (TyKind::Array(t1, s1), TyKind::Array(t2, s2)) => {
                if *s1 != *s2 {
                    Err(UnifyTyErr::Mismatch)
                } else {
                    t1.unify(t2.as_ref(), tycx)?;
                    Ok(())
                }
            }

            (TyKind::Tuple(t1), TyKind::Tuple(t2)) => {
                if t1.len() != t2.len() {
                    Err(UnifyTyErr::Mismatch)
                } else {
                    for (t1, t2) in t1.iter().zip(t2.iter()) {
                        t1.unify(t2, tycx)?;
                    }
                    Ok(())
                }
            }

            (TyKind::Struct(t1), TyKind::Struct(t2)) => {
                if t1.binding_info_id == t2.binding_info_id {
                    Ok(())
                } else if t1.fields.len() != t2.fields.len() || t1.kind != t2.kind {
                    Err(UnifyTyErr::Mismatch)
                } else {
                    for (f1, f2) in t1.fields.iter().zip(t2.fields.iter()) {
                        f1.ty.unify(&f2.ty, tycx)?;
                    }
                    Ok(())
                }
            }

            (TyKind::Type(t1), TyKind::Type(t2)) => t1.unify(t2.as_ref(), tycx),
            // (TyKind::Type(t1), t2) => t1.unify(t2, tycx),
            // (t1, TyKind::Type(t2)) => t1.unify(t2.as_ref(), tycx),
            (TyKind::Var(var), _) => unify_var_ty(*var, other, tycx),
            (_, TyKind::Var(var)) => unify_var_ty(*var, self, tycx),

            (TyKind::Never, _) | (_, TyKind::Never) => Ok(()),

            _ => Err(UnifyTyErr::Mismatch),
        }
    }
}

fn unify_var_ty(var: Ty, other: &TyKind, tycx: &mut TyCtx) -> UnifyTyResult {
    match tycx.value_of(var) {
        InferenceValue::Bound(kind) => kind.clone().unify(other, tycx),
        InferenceValue::AnyInt => {
            let other_kind = other.normalize(&tycx);
            match other_kind {
                TyKind::Int(_) | TyKind::UInt(_) | TyKind::Float(_) => {
                    tycx.bind(var, other.clone());
                    Ok(())
                }
                TyKind::AnyInt(other) | TyKind::AnyFloat(other) | TyKind::Var(other) => {
                    if other != var {
                        tycx.bind(other, var.into());
                    }
                    Ok(())
                }
                _ => Err(UnifyTyErr::Mismatch),
            }
        }
        InferenceValue::AnyFloat => {
            let other_kind = other.normalize(&tycx);
            match other_kind {
                TyKind::Float(_) => {
                    tycx.bind(var, other.clone());
                    Ok(())
                }
                TyKind::AnyInt(other) | TyKind::AnyFloat(other) | TyKind::Var(other) => {
                    if other != var {
                        tycx.bind(other, var.into());
                    }
                    Ok(())
                }
                _ => Err(UnifyTyErr::Mismatch),
            }
        }
        InferenceValue::Unbound => {
            let other_kind = other.normalize(&tycx);

            if TyKind::Var(var) != other_kind {
                if occurs(var, &other_kind, tycx) {
                    Err(UnifyTyErr::Occurs)
                } else {
                    tycx.bind(var, other.clone());
                    Ok(())
                }
            } else {
                Ok(())
            }
        }
    }
}

fn occurs(var: Ty, kind: &TyKind, tycx: &TyCtx) -> bool {
    match kind {
        TyKind::Var(other) => match tycx.value_of(*other) {
            InferenceValue::Bound(ty) => occurs(var, &ty, tycx),
            InferenceValue::AnyInt | InferenceValue::AnyFloat | InferenceValue::Unbound => {
                var == *other
            }
        },
        TyKind::Fn(f) => f.params.iter().any(|p| occurs(var, p, tycx)) || occurs(var, &f.ret, tycx),
        TyKind::Pointer(ty, _)
        | TyKind::MultiPointer(ty, _)
        | TyKind::Array(ty, _)
        | TyKind::Slice(ty, _) => occurs(var, ty, tycx),
        TyKind::Tuple(tys) => tys.iter().any(|ty| occurs(var, ty, tycx)),
        TyKind::Struct(st) => st.fields.iter().any(|f| occurs(var, &f.ty, tycx)),
        _ => false,
    }
}

pub type UnifyTyResult = Result<(), UnifyTyErr>;

#[derive(Debug)]
pub enum UnifyTyErr {
    Mismatch,
    Occurs,
}

// NOTE (Ron): checks that mutability rules are equal
pub fn can_coerce_mut(from: bool, to: bool) -> bool {
    from == to || (!from && to)
}