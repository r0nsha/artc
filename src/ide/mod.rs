mod hint;
mod types;
mod util;

use crate::ast::{ast::TypedAst, ty::Type, workspace::Workspace};
use crate::error::diagnostic::DiagnosticSeverity;
use crate::infer::{normalize::Normalize, ty_ctx::TyCtx};
use crate::span::{EndPosition, Position, Span};
use types::*;
use util::*;

use self::hint::{CollectHints, HintSess};

pub fn diagnostics(workspace: &Workspace, tycx: Option<&TyCtx>, typed_ast: Option<&TypedAst>) {
    let mut objects: Vec<IdeObject> = vec![];

    objects.extend(
        workspace
            .diagnostics
            .items()
            .iter()
            .filter(|diag| !diag.labels.is_empty())
            .map(|diag| {
                let label = diag.labels.first().unwrap();
                let file = workspace.diagnostics.get_file(label.span.file_id).unwrap();

                IdeObject::Diagnostic(IdeDiagnostic {
                    severity: match &diag.severity {
                        DiagnosticSeverity::Error => IdeDiagnosticSeverity::Error,
                    },
                    span: IdeSpan::from_span_and_file(label.span, file.name()),
                    message: diag.message.clone().unwrap(),
                })
            }),
    );

    match (tycx, typed_ast) {
        (Some(tycx), Some(typed_ast)) => {
            let mut sess = HintSess {
                workspace,
                tycx,
                hints: vec![],
            };

            typed_ast
                .bindings
                .iter()
                .for_each(|(_, binding)| binding.collect_hints(&mut sess));

            typed_ast
                .functions
                .iter()
                .for_each(|(_, function)| function.collect_hints(&mut sess));

            objects.extend(sess.hints.into_iter().map(IdeObject::Hint));
        }
        _ => (),
    }

    write(&objects);
}

pub fn hover_info(workspace: &Workspace, tycx: Option<&TyCtx>, offset: usize) {
    if let Some(tycx) = tycx {
        let searched_binding_info =
            workspace
                .binding_infos
                .iter()
                .map(|(_, b)| b)
                .find(|binding_info| {
                    binding_info.module_id == workspace.root_module_id
                        && binding_info.span.contains(offset)
                });

        if let Some(binding_info) = searched_binding_info {
            write(&HoverInfo {
                contents: binding_info.ty.normalize(tycx).to_string(),
            });
        }
    } else {
        write_null();
    }
}

pub fn goto_definition(workspace: &Workspace, tycx: Option<&TyCtx>, offset: usize) {
    for (_, binding_info) in workspace.binding_infos.iter() {
        if is_offset_in_span_and_root_module(workspace, offset, binding_info.span) {
            if let Some(tycx) = tycx {
                match binding_info.ty.normalize(tycx) {
                    Type::Module(module_id) => {
                        let module_info = workspace.get_module_info(module_id).unwrap();

                        let span = Span {
                            file_id: module_info.file_id,
                            start: Position::initial(),
                            end: EndPosition::initial(),
                        };

                        write(&IdeSpan::from_span_and_file(
                            span,
                            module_info.file_path.to_string(),
                        ));

                        return;
                    }
                    _ => (),
                }
            }

            write(&IdeSpan::from_span_and_file(
                binding_info.span,
                workspace
                    .get_module_info(binding_info.module_id)
                    .unwrap()
                    .file_path
                    .to_string(),
            ));

            return;
        }

        for &use_span in binding_info.uses.iter() {
            if is_offset_in_span_and_root_module(workspace, offset, use_span) {
                write(&IdeSpan::from_span_and_file(
                    binding_info.span,
                    workspace
                        .get_module_info(binding_info.module_id)
                        .unwrap()
                        .file_path
                        .to_string(),
                ));

                return;
            }
        }
    }

    write_null();
}