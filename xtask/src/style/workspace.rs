use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bray_source::LineIndex;
use ra_ap_syntax::{Edition, SourceFile};

use super::diagnostic::{Diagnostic, Severity};
use super::{blank_line, exemption, source, structure};
use crate::{command, workspace as repository_workspace};

pub(crate) fn run(mut arguments: impl Iterator<Item = String>) -> ExitCode {
    let result = match arguments.next().as_deref() {
        None => fix_workspace(),
        Some("check") => {
            command::reject_trailing_argument(arguments).and_then(|()| check_workspace())
        }
        Some(action) => Err(format!("unexpected style command: {action}")),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");

            ExitCode::FAILURE
        }
    }
}

fn fix_workspace() -> Result<(), String> {
    let root = repository_workspace::root()?;
    let paths = source::rust_source_paths(&root)?;
    let fix_count = fix_sources(&paths)?;

    if fix_count > 0 {
        eprintln!("fixed {fix_count} style violation(s)");
    }

    validate_workspace(&root, &paths)
}

pub(super) fn check_workspace() -> Result<(), String> {
    let root = repository_workspace::root()?;
    let paths = source::rust_source_paths(&root)?;

    validate_workspace(&root, &paths)
}

fn fix_sources(paths: &[PathBuf]) -> Result<usize, String> {
    let mut fix_count = 0;

    for path in paths {
        let source = read_source(path)?;

        let (fixed, source_fix_count) = blank_line::fix_source(&source)?;

        if source_fix_count == 0 {
            continue;
        }

        std::fs::write(path, fixed)
            .map_err(|error| repository_workspace::io_error("write", path, error))?;

        fix_count += source_fix_count;
    }

    Ok(fix_count)
}

fn validate_workspace(root: &Path, paths: &[PathBuf]) -> Result<(), String> {
    let mut error_count = 0;

    for path in paths {
        let source = read_source(path)?;
        let diagnostics = source_diagnostics(path, &source);
        let relative_path = path.strip_prefix(root).unwrap_or(path);
        let line_index = source_line_index(path, &source)?;

        for diagnostic in diagnostics {
            eprintln!(
                "{}",
                format_diagnostic(relative_path, &line_index, &diagnostic)?
            );

            if diagnostic.rule.severity() == Severity::Error {
                error_count += 1;
            }
        }
    }

    if error_count == 0 {
        Ok(())
    } else {
        Err(format!(
            "style validation failed with {error_count} error(s)"
        ))
    }
}

fn source_diagnostics(path: &Path, source: &str) -> Vec<Diagnostic> {
    let file = SourceFile::parse(source, Edition::Edition2024).tree();
    let mut diagnostics = blank_line::check_source(source);

    diagnostics.extend(structure::check(path, source, &file));

    let mut diagnostics = exemption::apply(source, &file, diagnostics);
    diagnostics.sort_by_key(|diagnostic| (diagnostic.offset, diagnostic.rule));
    diagnostics.dedup();

    diagnostics
}

fn read_source(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path)
        .map_err(|error| repository_workspace::io_error("read", path, error))
}

fn source_line_index(path: &Path, source: &str) -> Result<LineIndex, String> {
    LineIndex::new(source).map_err(|error| {
        format!(
            "failed to index {}: {} bytes exceed source offset capacity",
            path.display(),
            error.bytes()
        )
    })
}

fn format_diagnostic(
    path: &Path,
    line_index: &LineIndex,
    diagnostic: &Diagnostic,
) -> Result<String, String> {
    let Some((line, column)) = source::source_location(line_index, diagnostic.offset) else {
        return Err(format!(
            "style diagnostic offset is outside {}",
            path.display()
        ));
    };

    let mut rendered = format!(
        "{}:{line}:{column}: {}[style/{}]: {}",
        path.display(),
        diagnostic.rule.severity().label(),
        diagnostic.rule.identifier(),
        diagnostic.message
    );

    if let Some(help) = &diagnostic.help {
        rendered.push_str("\n  help: ");
        rendered.push_str(help);
    }

    Ok(rendered)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use bray_source::LineIndex;
    use ra_ap_syntax::TextSize;

    use super::{check_workspace, format_diagnostic};
    use crate::style::diagnostic::{Diagnostic, Rule};

    #[test]
    fn diagnostics_use_compiler_style_locations_severity_and_help() {
        let source = "fn example() {}\n";

        let line_index = match LineIndex::new(source) {
            Ok(index) => index,
            Err(error) => panic!("test source should fit in TextSize: {error:?}"),
        };

        let diagnostic = Diagnostic::new(Rule::LegacyModRs, TextSize::new(3))
            .with_help("use the modern module layout");

        let rendered = match format_diagnostic(Path::new("src/mod.rs"), &line_index, &diagnostic) {
            Ok(rendered) => rendered,
            Err(error) => panic!("diagnostic should render: {error}"),
        };

        assert_eq!(
            rendered,
            "src/mod.rs:1:4: error[style/legacy-mod-rs]: legacy mod.rs module layout is not allowed\n  help: use the modern module layout"
        );
    }

    #[test]
    fn workspace_sources_conform() {
        assert_eq!(check_workspace(), Ok(()));
    }
}
