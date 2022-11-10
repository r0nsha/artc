use crate::{
    ast,
    error::{
        diagnostic::{Diagnostic, Label},
        DiagnosticResult, TypeError,
    },
    hir::const_value::ConstValue,
    infer::type_ctx::TypeCtx,
    span::Span,
};

pub fn is_valid_binary_op(op: ast::BinaryOp) -> bool {
    matches!(
        op,
        ast::BinaryOp::Add
            | ast::BinaryOp::Sub
            | ast::BinaryOp::Mul
            | ast::BinaryOp::Div
            | ast::BinaryOp::Rem
            | ast::BinaryOp::Eq
            | ast::BinaryOp::Ne
            | ast::BinaryOp::Lt
            | ast::BinaryOp::Le
            | ast::BinaryOp::Gt
            | ast::BinaryOp::Ge
            | ast::BinaryOp::And
            | ast::BinaryOp::Or
            | ast::BinaryOp::Shl
            | ast::BinaryOp::Shr
            | ast::BinaryOp::BitAnd
            | ast::BinaryOp::BitOr
            | ast::BinaryOp::BitXor
    )
}

pub fn binary(
    lhs: &ConstValue,
    rhs: &ConstValue,
    op: ast::BinaryOp,
    span: Span,
    tcx: &TypeCtx,
) -> DiagnosticResult<ConstValue> {
    fn int_overflow(action: &str, lhs: &ConstValue, rhs: &ConstValue, span: Span, tcx: &TypeCtx) -> Diagnostic {
        Diagnostic::error()
            .with_message(format!(
                "integer overflowed while {} {} and {} at compile-time",
                action,
                lhs.display(tcx),
                rhs.display(tcx)
            ))
            .with_label(Label::primary(span, "integer overflow"))
    }

    let int_overflow = |action: &str| int_overflow(action, lhs, rhs, span, tcx);

    match op {
        ast::BinaryOp::Add => lhs.add(rhs).ok_or_else(|| int_overflow("adding")),
        ast::BinaryOp::Sub => lhs.sub(rhs).ok_or_else(|| int_overflow("subtracting")),
        ast::BinaryOp::Mul => lhs.mul(rhs).ok_or_else(|| int_overflow("multiplying")),
        ast::BinaryOp::Div => match rhs {
            ConstValue::Int(0) => Err(TypeError::divide_by_zero(span)),
            _ => lhs.div(rhs).ok_or_else(|| int_overflow("dividing")),
        },
        ast::BinaryOp::Rem => match rhs {
            ConstValue::Int(0) => Err(TypeError::divide_by_zero(span)),
            _ => lhs.rem(rhs).ok_or_else(|| int_overflow("taking the remainder of")),
        },
        ast::BinaryOp::Eq => Ok(lhs.eq(rhs)),
        ast::BinaryOp::Ne => Ok(lhs.ne(rhs)),
        ast::BinaryOp::Lt => Ok(lhs.lt(rhs)),
        ast::BinaryOp::Le => Ok(lhs.le(rhs)),
        ast::BinaryOp::Gt => Ok(lhs.gt(rhs)),
        ast::BinaryOp::Ge => Ok(lhs.ge(rhs)),
        ast::BinaryOp::And => Ok(lhs.and(rhs)),
        ast::BinaryOp::Or => Ok(lhs.or(rhs)),
        ast::BinaryOp::Shl => lhs.shl(rhs).ok_or_else(|| int_overflow("shifting left")),
        ast::BinaryOp::Shr => lhs.shr(rhs).ok_or_else(|| int_overflow("shifting right")),
        ast::BinaryOp::BitAnd => Ok(lhs.bitand(rhs)),
        ast::BinaryOp::BitOr => Ok(lhs.bitor(rhs)),
        ast::BinaryOp::BitXor => Ok(lhs.bitxor(rhs)),
        _ => unreachable!("{}", op),
    }
}
