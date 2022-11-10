use super::*;
use crate::{
    ast::{self, Ast, Call, Cast, UnaryOp},
    error::{diagnostic::Label, *},
    span::{EndPosition, Position},
    token::TokenKind::*,
    types::FunctionTypeKind,
};
use ustr::ustr;

impl Parser {
    pub fn parse_operand_postfix_operator(&mut self, mut expr: Ast) -> DiagnosticResult<Ast> {
        // postfix expressions (recursive)
        loop {
            let last_index = self.current;

            // Special case `.` to be allowed in a new line.
            // We skip all new lines and then check if the next token is a `.`.
            // If it is, we parse a member access, else, we return to the last index.
            self.skip_newlines();

            if eat!(self, Dot) {
                expr = self.parse_member_access(expr)?;
                continue;
            } else {
                self.current = last_index;

                expr = if eat!(self, Dot) {
                    self.parse_member_access(expr)?
                } else if eat!(self, OpenParen) {
                    self.parse_call(expr)?
                } else if eat!(self, OpenBracket) {
                    self.parse_subscript_or_slice(expr)?
                } else if !self.restrictions.contains(Restrictions::NO_CAST) && eat!(self, As) {
                    self.parse_cast(expr)?
                } else if eat!(self, Fn) {
                    let start_span = expr.span();

                    let fn_arg = self.parse_function_expr(None, FunctionTypeKind::Orphan)?;
                    let span = start_span.to(self.previous_span());

                    match &mut expr {
                        Ast::Call(call) => {
                            // map(x) fn ...
                            call.args.push(ast::CallArg {
                                value: fn_arg,
                                spread: false,
                            });

                            expr
                        }
                        _ => {
                            // map fn ...
                            Ast::Call(Call {
                                callee: Box::new(expr),
                                args: vec![ast::CallArg {
                                    value: fn_arg,
                                    spread: false,
                                }],
                                named_args: vec![],
                                span,
                            })
                        }
                    }
                } else {
                    break;
                }
            }
        }

        // named struct literal
        if !self.restrictions.contains(Restrictions::NO_STRUCT_LITERAL) && is!(self, OpenCurly) {
            return self.parse_struct_literal(Some(Box::new(expr)));
        }

        Ok(expr)
    }

    fn parse_cast(&mut self, expr: Ast) -> DiagnosticResult<Ast> {
        let start_span = expr.span();

        let target_type = self.parse_expression_res(Restrictions::NO_CAST, false, true)?;

        Ok(Ast::Cast(Cast {
            expr: Box::new(expr),
            target_type: Box::new(target_type),
            span: start_span.to(self.previous_span()),
        }))
    }

    fn parse_member_access(&mut self, expr: Ast) -> DiagnosticResult<Ast> {
        let start_span = expr.span();

        self.skip_newlines();

        let token = self.bump();

        let expr = match token.kind {
            Ident(id) => Ast::MemberAccess(ast::MemberAccess {
                expr: Box::new(expr),
                member: id,
                member_span: token.span,
                span: start_span.to(token.span),
            }),

            Int(i) => Ast::MemberAccess(ast::MemberAccess {
                expr: Box::new(expr),
                member: ustr(&i.to_string()),
                member_span: token.span,
                span: start_span.to(token.span),
            }),

            Float(_) => {
                // this is for chained tuple access like `tuple.0.1`
                let components = token.lexeme.split('.').collect::<Vec<&str>>();

                let first_component_span = token.span.with_end(EndPosition {
                    index: token.span.end.index - components[0].len() - 1,
                });

                let first_access = Ast::MemberAccess(ast::MemberAccess {
                    expr: Box::new(expr),
                    member: ustr(components[0]),
                    member_span: first_component_span,
                    span: start_span.to(first_component_span),
                });

                let second_component_span = token.span.with_start(Position {
                    index: token.span.start.index + components[0].len() + 1,
                    line: token.span.start.line,
                    column: token.span.start.column,
                });

                Ast::MemberAccess(ast::MemberAccess {
                    expr: Box::new(first_access),
                    member: ustr(components[0]),
                    member_span: second_component_span,
                    span: start_span.to(token.span),
                })
            }

            Star => Ast::Unary(ast::Unary {
                op: UnaryOp::Deref,
                value: Box::new(expr),
                span: start_span.to(token.span),
            }),

            _ => return Err(SyntaxError::expected(self.span(), "an identifier, number or *")),
        };

        Ok(expr)
    }

    fn parse_call(&mut self, callee: Ast) -> DiagnosticResult<Ast> {
        let start_span = callee.span();

        let mut args: Vec<ast::CallArg> = vec![];
        let mut named_args: Vec<ast::CallNamedArg> = vec![];

        fn parse_arg_value(parser: &mut Parser) -> DiagnosticResult<(ast::Ast, bool)> {
            let value = parser.parse_expression(false, true)?;
            let spread = eat!(parser, DotDot);
            Ok((value, spread))
        }

        while !eat!(self, CloseParen) && !self.eof() {
            self.skip_newlines();

            if eat!(self, Ident(_)) {
                let id_token = *self.previous();

                let name = ast::NameAndSpan {
                    name: id_token.name(),
                    span: id_token.span,
                };

                if eat!(self, Colon) {
                    let (value, spread) = parse_arg_value(self)?;
                    named_args.push(ast::CallNamedArg { name, value, spread });
                } else if !named_args.is_empty() {
                    return Err(Diagnostic::error()
                        .with_message("positional arguments must come before named arguments")
                        .with_label(Label::primary(
                            name.span,
                            "positional argument comes after named arguments",
                        )));
                } else {
                    self.revert(1);
                    let (value, spread) = parse_arg_value(self)?;
                    args.push(ast::CallArg { value, spread });
                }
            } else {
                let (value, spread) = parse_arg_value(self)?;
                args.push(ast::CallArg { value, spread });
            }

            if eat!(self, Comma) {
                self.skip_newlines();
                continue;
            } else if eat!(self, CloseParen) {
                break;
            } else {
                let span = self.previous_span().after();
                return Err(SyntaxError::expected(
                    span,
                    &format!("{}, got {}", ", or )", self.peek().kind.lexeme()),
                ));
            }
        }

        Ok(Ast::Call(Call {
            callee: Box::new(callee),
            args,
            named_args,
            span: start_span.to(self.previous_span()),
        }))
    }

    fn parse_subscript_or_slice(&mut self, expr: Ast) -> DiagnosticResult<Ast> {
        let start_span = expr.span();

        if eat!(self, DotDot) {
            let high = if eat!(self, CloseBracket) {
                None
            } else {
                let high = self.parse_expression(false, true)?;
                require!(self, CloseBracket, "]")?;
                Some(Box::new(high))
            };

            Ok(Ast::Slice(ast::Slice {
                expr: Box::new(expr),
                low: None,
                high,
                span: start_span.to(self.previous_span()),
            }))
        } else {
            let index = self.parse_expression(false, true)?;

            if eat!(self, DotDot) {
                let high = if eat!(self, CloseBracket) {
                    None
                } else {
                    let high = self.parse_expression(false, true)?;
                    require!(self, CloseBracket, "]")?;
                    Some(Box::new(high))
                };

                Ok(Ast::Slice(ast::Slice {
                    expr: Box::new(expr),
                    low: Some(Box::new(index)),
                    high,
                    span: start_span.to(self.previous_span()),
                }))
            } else {
                require!(self, CloseBracket, "]")?;

                Ok(Ast::Subscript(ast::Subscript {
                    expr: Box::new(expr),
                    index: Box::new(index),
                    span: start_span.to(self.previous_span()),
                }))
            }
        }
    }
}
