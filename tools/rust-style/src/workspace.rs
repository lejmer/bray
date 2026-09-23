//! Public workspace-wide style operations.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use ra_ap_syntax::{AstNode, Edition, SourceFile};
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};

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
    let results = paths
        .par_iter()
        .map(|path| {
            let source = read_source(path)?;

            let (fixed, source_fix_count) = blank_line::fix_source(&source)
                .map_err(|error| format!("{}: {error}", path.display()))?;

            if source_fix_count > 0 {
                replace_source(path, fixed.as_bytes())
                    .map_err(|error| source::io_error("write", path, error))?;
            }

            Ok(source_fix_count)
        })
        .collect::<Vec<Result<usize, String>>>();

    results.into_iter().try_fold(0, |total, result| result.map(|count| total + count))
}

fn replace_source(path: &Path, contents: &[u8]) -> io::Result<()> {
    let permissions = std::fs::metadata(path)?.permissions();

    let directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    let mut temporary = tempfile::NamedTempFile::new_in(directory)?;

    temporary.write_all(contents)?;
    temporary.as_file().set_permissions(permissions)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;

    Ok(())
}

fn validate_workspace(root: &Path, paths: &[PathBuf]) -> Result<(), String> {
    let failure_policy = failure::Policy::from_paths(paths)?;

    let results = paths
        .par_iter()
        .map(|path| {
            let source = read_source(path)?;
            let diagnostics = source_diagnostics(path, &source, &failure_policy);

            let relative_path = path.strip_prefix(root).unwrap_or(path);

            diagnostics
                .iter()
                .map(|diagnostic| {
                    format_diagnostic(relative_path, &source, diagnostic)
                        .map(|message| (message, diagnostic.rule.severity() == Severity::Error))
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Vec<Result<Vec<(String, bool)>, String>>>();

    let mut error_count = 0;

    for result in results {
        for (message, is_error) in result? {
            eprintln!("{message}");
            error_count += usize::from(is_error);
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
    let mut diagnostics = blank_line::check_syntax(source, file.syntax());

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
    fn source_replacement_preserves_permissions_and_cleans_failed_staging() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("example.rs");

        std::fs::write(&path, "original").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
        }

        let permissions = std::fs::metadata(&path).unwrap().permissions();

        super::replace_source(&path, b"replacement").unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"replacement");
        assert_eq!(std::fs::metadata(&path).unwrap().permissions(), permissions);

        let blocked = directory.path().join("directory.rs");

        std::fs::create_dir(&blocked).unwrap();
        std::fs::write(blocked.join("retained"), "original").unwrap();

        assert!(super::replace_source(&blocked, b"replacement").is_err());
        assert_eq!(std::fs::read(blocked.join("retained")).unwrap(), b"original");
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 2);
    }

    #[cfg(windows)]
    #[test]
    fn locked_source_survives_failed_replacement() {
        use std::fs::OpenOptions;
        use std::os::windows::fs::OpenOptionsExt;

        const FILE_SHARE_READ: u32 = 1;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("example.rs");

        std::fs::write(&path, "original").unwrap();

        let permissions = std::fs::metadata(&path).unwrap().permissions();

        let lock = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(&path)
            .unwrap();

        assert!(super::replace_source(&path, b"replacement").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
        assert_eq!(std::fs::metadata(&path).unwrap().permissions(), permissions);
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);

        drop(lock);
    }

    #[test]
    fn workspace_fixes_replace_source_and_are_idempotent() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("example.rs");
        let source = "fn example() {\n    let value = 1;\n    return value;\n}\n";

        std::fs::write(&path, source).unwrap();

        let (expected, fixes) = crate::blank_line::fix_source(source).unwrap();

        assert!(fixes > 0);
        assert_eq!(super::fix_sources(&[path.clone()]).unwrap(), fixes);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
        assert_eq!(super::fix_sources(&[path]).unwrap(), 0);
    }

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
