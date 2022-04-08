use chili_ast::{
    ast,
    ty::{IntTy, Ty, TyKind, UIntTy},
};
use chili_check::normalize::NormalizeTy;
use chili_error::DiagnosticResult;
use chili_span::Span;
use codespan_reporting::diagnostic::{Diagnostic, Label};
use std::fmt::Display;

use crate::sess::LintSess;

impl<'s> LintSess<'s> {
    pub fn check_type_limits(&self, e: &ast::Expr) -> DiagnosticResult<()> {
        match &e.kind {
            ast::ExprKind::Literal(k) => match k {
                &ast::Literal::Int(value) => match &e.ty.normalize(self.tycx) {
                    TyKind::Int(int_ty) => {
                        let (min, max) = int_ty_range(*int_ty);

                        if value < min || value > max {
                            Err(overflow_err(value, &e.ty, min, max, e.span))
                        } else {
                            Ok(())
                        }
                    }
                    TyKind::UInt(uint_ty) => {
                        let (min, max) = uint_ty_range(*uint_ty);

                        if value.is_negative() {
                            Err(overflow_err(value, &e.ty, min, max, e.span))
                        } else {
                            let value = value as u64;

                            if value < min || value > max {
                                Err(overflow_err(value, &e.ty, min, max, e.span))
                            } else {
                                Ok(())
                            }
                        }
                    }
                    _ => Ok(()),
                },
                ast::Literal::Float(_)
                | ast::Literal::Unit
                | ast::Literal::Nil
                | ast::Literal::Bool(_)
                | ast::Literal::Str(_)
                | ast::Literal::Char(_) => Ok(()),
            },
            _ => Ok(()),
        }
    }
}

fn int_ty_range(int_ty: IntTy) -> (i64, i64) {
    match int_ty {
        IntTy::I8 => (i8::MIN as i64, i8::MAX as i64),
        IntTy::I16 => (i16::MIN as i64, i16::MAX as i64),
        IntTy::I32 => (i32::MIN as i64, i32::MAX as i64),
        IntTy::I64 => (i64::MIN, i64::MAX),
        IntTy::Int => (isize::MIN as i64, isize::MAX as i64),
    }
}

fn uint_ty_range(uint_ty: UIntTy) -> (u64, u64) {
    match uint_ty {
        UIntTy::U8 => (u8::MIN as u64, u8::MAX as u64),
        UIntTy::U16 => (u16::MIN as u64, u16::MAX as u64),
        UIntTy::U32 => (u32::MIN as u64, u32::MAX as u64),
        UIntTy::U64 => (u64::MIN, u64::MAX),
        UIntTy::UInt => (usize::MIN as u64, usize::MAX as u64),
    }
}

fn overflow_err<V: Copy + Display, M: Copy + Display>(
    value: V,
    ty: &Ty,
    min: M,
    max: M,
    span: Span,
) -> Diagnostic<usize> {
    Diagnostic::error()
        .with_message(format!(
            "integer literal of type `{}` must be between {} and {}, but found {}",
            ty, min, max, value
        ))
        .with_labels(vec![
            Label::primary(span.file_id, span.range()).with_message("integer literal overflow")
        ])
}
