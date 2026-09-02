//! Public workspace-wide style operations.

use std::path::{Path, PathBuf};

use ra_ap_syntax::{Edition, SourceFile};

use super::diagnostic::{Diagnostic, Severity};
use super::{blank_line, exemption, failure, source, structure};

/// Applies deterministic fixes and then validates every Rust source file under `root`.
///
/// Diagnostics are written to standard error. The operation fails when source discovery or
/// rewriting fails, or when validation produces any error-level diagnostic.
pub fn fix_workspace(root: &Path) -> Result<(), String> {
    let paths = source::rust_source_paths(root)?;
    let fix_count = fix_sources(&paths)?;

    if fix_count > 0 {
        eprintln!("fixed {fix_count} style violation(s)");
    }

    validate_workspace(root, &paths)
}

/// Validates every Rust source file under `root` without changing it.
///
/// Diagnostics are written to standard error. The operation fails when source discovery fails or
/// validation produces any error-level diagnostic.
pub fn check_workspace(root: &Path) -> Result<(), String> {
    let paths = source::rust_source_paths(root)?;

    validate_workspace(root, &paths)
}

fn fix_sources(paths: &[PathBuf]) -> Result<usize, String> {
    let mut fix_count = 0;

    for path in paths {
        let source = read_source(path)?;

        let (fixed, source_fix_count) = blank_line::fix_source(&source)?;

        if source_fix_count == 0 {
            continue;
        }

        std::fs::write(path, fixed).map_err(|error| source::io_error("write", path, error))?;

        fix_count += source_fix_count;
    }

    Ok(fix_count)
}

fn validate_workspace(root: &Path, paths: &[PathBuf]) -> Result<(), String> {
    let failure_policy = failure::Policy::from_paths(paths)?;
    let mut error_count = 0;

    for path in paths {
        let source = read_source(path)?;
        let diagnostics = source_diagnostics(path, &source, &failure_policy);
        let relative_path = path.strip_prefix(root).unwrap_or(path);

        for diagnostic in diagnostics {
            eprintln!(
                "{}",
                format_diagnostic(relative_path, &source, &diagnostic)?
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

fn source_diagnostics(
    path: &Path,
    source: &str,
    failure_policy: &failure::Policy,
) -> Vec<Diagnostic> {
    let file = SourceFile::parse(source, Edition::Edition2024).tree();
    let mut diagnostics = blank_line::check_source(source);

    diagnostics.extend(structure::check(path, source, &file));
    diagnostics.extend(failure::check(path, source, &file, failure_policy));

    let mut diagnostics = exemption::apply(source, &file, diagnostics);
    diagnostics.sort_by_key(|diagnostic| (diagnostic.offset, diagnostic.rule));
    diagnostics.dedup();

    diagnostics
}

fn read_source(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|error| source::io_error("read", path, error))
}

fn format_diagnostic(
    path: &Path,
    source_text: &str,
    diagnostic: &Diagnostic,
) -> Result<String, String> {
    let offset = source::text_offset(diagnostic.offset);
    let line_starts = source::line_starts(source_text);

    let Some(line_index) = source::line_index(&line_starts, offset) else {
        return Err(format!(
            "style diagnostic offset is outside {}",
            path.display()
        ));
    };

    let Some(line_start) = line_starts.get(line_index) else {
        return Err(format!(
            "style diagnostic offset is outside {}",
            path.display()
        ));
    };

    let line = line_index + 1;
    let column = offset - line_start + 1;

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

    use ra_ap_syntax::TextSize;

    use super::format_diagnostic;
    use crate::diagnostic::{Diagnostic, Rule};

    #[test]
    fn diagnostics_use_compiler_style_locations_severity_and_help() {
        let source = "fn example() {}\n";

        let diagnostic = Diagnostic::new(Rule::LegacyModRs, TextSize::new(3))
            .with_help("use the modern module layout");

        let rendered = match format_diagnostic(Path::new("src/mod.rs"), source, &diagnostic) {
            Ok(rendered) => rendered,
            Err(error) => panic!("diagnostic should render: {error}"),
        };

        assert_eq!(
            rendered,
            "src/mod.rs:1:4: error[style/legacy-mod-rs]: legacy mod.rs module layout is not allowed\n  help: use the modern module layout"
        );
    }
}
