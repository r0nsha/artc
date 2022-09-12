use super::*;
use crate::error::{
    diagnostic::{Diagnostic, Label},
    DiagnosticResult, SyntaxError,
};

impl Parser {
    pub fn parse_import(&mut self) -> DiagnosticResult<ast::Ast> {
        let start_span = require!(self, Import, "import")?.span;

        require!(self, OpenParen, "(")?;

        let id_token = require!(self, Ident(_), "an identifier")?;
        let name = id_token.name();

        require!(self, CloseParen, ")")?;

        let span = start_span.to(self.previous_span());

        let mut search_notes = vec![];

        match self.search_for_child_module(name) {
            Ok(module_path) => self.finish_parse_import(module_path, span),
            Err(path) => {
                search_notes.push(format!("searched path: {}", path.display()));

                match self.search_for_neighbor_module(name) {
                    Ok(module_path) => self.finish_parse_import(module_path, span),
                    Err(path) => {
                        search_notes.push(format!("searched path: {}", path.display()));

                        match self.cache.lock().libraries.get(&name) {
                            Some(library) => {
                                let module_path =
                                    ModulePath::new(library.clone(), vec![ustr(library.root_file_stem())]);
                                self.finish_parse_import(module_path, span)
                            }
                            None => {
                                search_notes.push(format!("searched for a library named `{}`", name));

                                let mut diagnostic = Diagnostic::error()
                                    .with_message(format!("could not find module or library `{}`", name))
                                    .with_label(Label::primary(span, "undefined module or library"));

                                for note in search_notes {
                                    diagnostic.add_note(note);
                                }

                                Err(diagnostic)
                            }
                        }
                    }
                }
            }
        }
    }

    fn finish_parse_import(&self, module_path: ModulePath, span: Span) -> DiagnosticResult<ast::Ast> {
        let path = module_path.path();

        spawn_parser(
            self.thread_pool.clone(),
            self.tx.clone(),
            Arc::clone(&self.cache),
            module_path,
        );

        Ok(ast::Ast::Import(ast::Import { path, span }))
    }

    fn search_for_child_module(&self, name: Ustr) -> Result<ModulePath, PathBuf> {
        let mut module_path = self.module_path.clone();
        module_path.push(name);

        let path = module_path.path();

        if path.exists() {
            Ok(module_path)
        } else {
            Err(path)
        }
    }

    fn search_for_neighbor_module(&self, name: Ustr) -> Result<ModulePath, PathBuf> {
        let mut module_path = self.module_path.clone();
        module_path.pop();
        module_path.push(name);

        let path = module_path.path();

        if path.exists() {
            Ok(module_path)
        } else {
            Err(path)
        }
    }
}
