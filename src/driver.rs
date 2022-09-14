use crate::{
    astgen::AstGenerationStats,
    common::{
        build_options::{BuildOptions, CodegenOptions},
        path::{resolve_relative_path, RelativeTo},
        Stopwatch,
    },
    error::diagnostic::Diagnostic,
    hir,
    infer::type_ctx::TypeCtx,
    time,
    workspace::{library::Library, LibraryId, ModuleId, Workspace},
};
use colored::Colorize;
use num_format::{Locale, ToFormattedString};
use std::path::PathBuf;
use ustr::ustr;

pub struct StartWorkspaceResult {
    pub workspace: Workspace,
    pub tcx: Option<TypeCtx>,
    pub cache: Option<hir::Cache>,
    pub output_file: Option<PathBuf>,
}

impl StartWorkspaceResult {
    fn new_untyped(workspace: Workspace) -> Self {
        Self {
            workspace,
            tcx: None,
            cache: None,
            output_file: None,
        }
    }

    fn new_typed(workspace: Workspace, tcx: TypeCtx, cache: hir::Cache) -> Self {
        Self {
            workspace,
            tcx: Some(tcx),
            cache: Some(cache),
            output_file: None,
        }
    }

    fn new_typed_with_output(workspace: Workspace, tcx: TypeCtx, cache: hir::Cache, output_file: PathBuf) -> Self {
        Self {
            workspace,
            tcx: Some(tcx),
            cache: Some(cache),
            output_file: Some(output_file),
        }
    }
}

pub fn start_workspace(name: String, build_options: BuildOptions) -> StartWorkspaceResult {
    // Set up workspace
    let source_file = resolve_relative_path(&build_options.source_file, &RelativeTo::Cwd);

    let main_library = Library {
        id: LibraryId::unknown(),
        name: ustr(&name),
        root_file: source_file.clone(),
        root_module_id: ModuleId::unknown(),
        is_main: true,
    };

    let mut workspace = Workspace::new(name, build_options, main_library);

    let all_sw = workspace.build_options.emit_times.then(|| Stopwatch::start_new("time"));

    // Check that root file exists
    if !source_file.exists() {
        workspace
            .diagnostics
            .push(Diagnostic::error().with_message(format!("file `{}` doesn't exist", source_file.display())));

        workspace.emit_diagnostics();

        return StartWorkspaceResult::new_untyped(workspace);
    }

    // Parse all source files into ast's
    let (modules, stats) = time! { workspace.build_options.emit_times, "parse", {
        crate::astgen::generate_ast(&mut workspace)
    }};

    if workspace.diagnostics.has_errors() {
        workspace.emit_diagnostics();
        return StartWorkspaceResult::new_untyped(workspace);
    }

    // Type inference, type checking, static analysis, const folding, etc..
    let (cache, tcx) = time! { workspace.build_options.emit_times, "check", {
        crate::check::check(&mut workspace, modules)
    }};

    if workspace.diagnostics.has_errors() {
        workspace.emit_diagnostics();
        return StartWorkspaceResult::new_typed(workspace, tcx, cache);
    }

    if workspace.build_options.emit_hir {
        hir::pretty::print(&cache, &workspace, &tcx);
    }

    // Lint - does auxillary checks which are not required for compilation
    time! { workspace.build_options.emit_times, "lint",
        crate::lint::lint(&mut workspace, &tcx, &cache)
    }

    if workspace.diagnostics.has_errors() {
        workspace.emit_diagnostics();
        return StartWorkspaceResult::new_typed(workspace, tcx, cache);
    }

    // Code generation
    match &workspace.build_options.codegen_options {
        CodegenOptions::Codegen { .. } => {
            let output_file = crate::backend::llvm::codegen(&workspace, &tcx, &cache);

            if workspace.build_options.emit_times {
                print_stats(stats, all_sw.unwrap().elapsed().as_millis());
            }

            StartWorkspaceResult::new_typed_with_output(workspace, tcx, cache, output_file)
        }
        _ => {
            if workspace.build_options.emit_times {
                print_stats(stats, all_sw.unwrap().elapsed().as_millis());
            }

            StartWorkspaceResult::new_typed(workspace, tcx, cache)
        }
    }
}

fn print_stats(stats: AstGenerationStats, elapsed_ms: u128) {
    println!("------------------------");
    println!(
        "{}\t{}",
        "lines:".cyan().bold(),
        stats.total_lines.to_formatted_string(&Locale::en)
    );
    println!("{}\t{}m", "time:".cyan().bold(), elapsed_ms);
}
