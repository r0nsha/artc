use chilic_ir::{
    expr::{Expr, ExprKind},
    func::{Fn, FnParam, Proto},
    pattern::{Pattern, SymbolPattern},
    stmt::Stmt,
};
use chilic_span::Span;
use chilic_ty::Ty;
use ustr::{ustr, Ustr};

use chilic_error::{DiagnosticResult, SyntaxError};

use crate::*;

impl Parser {
    pub(crate) fn parse_fn(&mut self) -> DiagnosticResult<Expr> {
        let name = self.get_decl_name();
        let start_span = self.previous().span.clone();

        let proto = self.parse_fn_proto(name, ParseProtoKind::Value)?;

        let body = self.parse_fn_body()?;

        Ok(Expr::new(
            ExprKind::Fn(Fn {
                proto,
                body,
                deferred: vec![],
                is_startup: false,
            }),
            Span::merge(&start_span, self.previous_span_ref()),
        ))
    }

    pub(crate) fn parse_fn_proto(
        &mut self,
        name: Ustr,
        kind: ParseProtoKind,
    ) -> DiagnosticResult<Proto> {
        let (params, variadic) = self.parse_fn_params(kind)?;

        let ret_ty = if match_token!(self, RightArrow) {
            Some(Box::new(self.parse_ty()?))
        } else {
            None
        };

        Ok(Proto {
            lib_name: None,
            name,
            params,
            variadic,
            ret: ret_ty,
            ty: Ty::Unknown,
        })
    }

    // TODO: this function is a hot mess, i need to refactor this
    pub(crate) fn parse_fn_params(
        &mut self,
        kind: ParseProtoKind,
    ) -> DiagnosticResult<(Vec<FnParam>, bool)> {
        let mut variadic = false;
        let mut params = vec![];

        if !match_token!(self, OpenParen) {
            return Ok((params, variadic));
        }

        while !match_token!(self, CloseParen) && !self.is_end() {
            if match_token!(self, DotDot) {
                require!(self, CloseParen, ")")?;
                variadic = true;
                break;
            }

            match kind {
                ParseProtoKind::Value => {
                    let pattern = self.parse_pattern()?;

                    let ty = if match_token!(self, Colon) {
                        let ty = self.parse_ty()?;
                        Some(Box::new(ty))
                    } else {
                        None
                    };

                    params.push(FnParam { pattern, ty });
                }
                ParseProtoKind::Type => {
                    // the parameter's name is optional, so we are checking for
                    // ambiguity here
                    if match_token!(self, Id(_))
                        || match_token!(self, Placeholder)
                    {
                        if match_token!(self, Colon) {
                            // (a: {type}, ..)
                            self.revert(2);
                        } else {
                            // ({type}, ..)
                            self.revert(1);
                            let pattern = Pattern::Single(SymbolPattern {
                                symbol: ustr(""),
                                alias: None,
                                span: Span::empty(),
                                is_mutable: false,
                                ignore: true,
                            });

                            let ty = Some(Box::new(self.parse_ty()?));
                            params.push(FnParam { pattern, ty });
                        }
                    }

                    // (a: {type}, ..)
                    let pattern = Pattern::Single(self.parse_symbol_pattern()?);
                    require!(self, Colon, ":")?;
                    let ty = Some(Box::new(self.parse_ty()?));
                    params.push(FnParam { pattern, ty });
                }
            }

            if match_token!(self, Comma) {
                continue;
            } else if match_token!(self, CloseParen) {
                break;
            } else {
                return Err(SyntaxError::expected(self.span_ref(), ", or )"));
            }
        }

        Ok((params, variadic))
    }

    pub(crate) fn parse_fn_body(&mut self) -> DiagnosticResult<Vec<Stmt>> {
        require!(self, OpenCurly, "{")?;
        let block = self.parse_block()?;

        Ok(match block.kind {
            ExprKind::Block { stmts, deferred: _ } => stmts,
            _ => {
                unreachable!()
            }
        })
    }
}

pub(crate) enum ParseProtoKind {
    Value,
    Type,
}
