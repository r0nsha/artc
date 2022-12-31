use super::CheckSess;
use crate::{
    error::diagnostic::{Diagnostic, Label},
    infer::{display::DisplayType, normalize::Normalize},
    span::Span,
};

impl<'s> CheckSess<'s> {
    pub(super) fn check_entry_point_function_exists(&mut self) {
        if self.workspace.build_options.need_entry_point_function() {
            if let Some(function) = self.cache.entry_point_function() {
                let ty = function.ty.normalize(&self.tcx).into_function();

                // Validate its type is fn() -> ()
                if !(ty.return_type.is_unit() || ty.return_type.is_never())
                    || !ty.params.is_empty()
                    || ty.has_c_varargs()
                {
                    self.workspace.diagnostics.push(
                        Diagnostic::error()
                            .with_message(format!(
                                "entry point function `{}` has type `{}`, expected `fn() -> ()`",
                                function.name,
                                ty.display(&self.tcx)
                            ))
                            .with_label(Label::primary(function.span, "invalid entry point function type")),
                    )
                }
            } else {
                self.workspace.diagnostics.push(
                    Diagnostic::error()
                        .with_message("entry point function is not defined")
                        .with_label(Label::primary(
                            Span::initial(self.workspace.get_root_module_info().file_id),
                            "",
                        ))
                        .with_note("define function `fn main = ()` in your root file"),
                )
            }
        }
    }
}
