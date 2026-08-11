use std::collections::BTreeSet;
use std::path::Path;

use super::guard::forbidden_profile_term;
use super::{argument, forbidden_internal_term};

const OTHER_USER_FACING_CATALOG_SOURCES: &[(&str, &str)] = &[
    ("build_progress.rs", include_str!("build_progress.rs")),
    ("diagnostics.rs", include_str!("diagnostics.rs")),
    ("interface.rs", include_str!("interface.rs")),
    ("label.rs", include_str!("label.rs")),
    ("language_server.rs", include_str!("language_server.rs")),
    ("related.rs", include_str!("related.rs")),
    ("suggestion.rs", include_str!("suggestion.rs")),
    ("test_report.rs", include_str!("test_report.rs")),
    ("../command_help.rs", include_str!("../../command_help.rs")),
];

const PROFILE_SOURCE: (&str, &str) = ("profile.rs", include_str!("profile.rs"));

#[test]
fn ordinary_user_messages_do_not_expose_compiler_implementation_terms() {
    for &(catalog, source) in argument::USER_FACING_SOURCES
        .iter()
        .chain(OTHER_USER_FACING_CATALOG_SOURCES)
    {
        for literal in rust_string_literals(source) {
            if let Some(forbidden) = forbidden_internal_term(&literal) {
                panic!("{catalog} user message exposes internal term {forbidden:?}: {literal:?}");
            }
        }
    }

    for literal in rust_string_literals(PROFILE_SOURCE.1) {
        if let Some(forbidden) = forbidden_profile_term(&literal) {
            panic!(
                "{} user message exposes internal term {forbidden:?}: {literal:?}",
                PROFILE_SOURCE.0
            );
        }
    }
}

#[test]
fn english_source_inventory_covers_every_renderer_leaf() {
    let english_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/catalog/english");
    let mut actual = BTreeSet::new();
    collect_rust_sources(&english_root, &english_root, &mut actual);
    actual.remove("argument/catalog.rs");
    actual.remove("argument/tests.rs");
    actual.remove("guard.rs");
    actual.remove("tests.rs");

    let mut declared = argument::USER_FACING_SOURCES
        .iter()
        .map(|(path, _)| (*path).to_owned())
        .collect::<BTreeSet<_>>();

    declared.extend(
        OTHER_USER_FACING_CATALOG_SOURCES
            .iter()
            .map(|(path, _)| *path)
            .filter(|path| !path.starts_with("../"))
            .map(str::to_owned),
    );

    declared.insert(PROFILE_SOURCE.0.to_owned());

    assert_eq!(declared, actual);

    let candidate_source = argument::USER_FACING_SOURCES
        .iter()
        .find_map(|(path, source)| {
            (*path == "argument/selection/candidate.rs").then_some(*source)
        })
        .expect("selection candidate renderer must be inventoried");

    assert!(rust_string_literals(candidate_source).contains(&"no candidates".to_owned()));
}

fn collect_rust_sources(directory: &Path, root: &Path, sources: &mut BTreeSet<String>) {
    let mut entries = std::fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", directory.display()))
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(|error| panic!("failed to inspect {}: {error}", directory.display()));

    entries.sort_by_key(std::fs::DirEntry::path);

    for entry in entries {
        let path = entry.path();

        if path.is_dir() {
            collect_rust_sources(&path, root, sources);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            sources.insert(relative_source_path(&path, root));
        }
    }
}

fn relative_source_path(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or_else(|error| {
            panic!(
                "{} is not beneath {}: {error}",
                path.display(),
                root.display()
            )
        })
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn rust_string_literals(source: &str) -> Vec<String> {
    let mut literals = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut escaped = false;

    for character in source.chars() {
        if !in_string {
            if character == '"' {
                in_string = true;
            }

            continue;
        }

        if escaped {
            current.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            literals.push(std::mem::take(&mut current));
            in_string = false;
        } else {
            current.push(character);
        }
    }

    literals
}
