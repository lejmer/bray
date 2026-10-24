use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::de::DeserializeOwned;
use syn::visit::Visit;

use crate::workspace::collect_rust_files;
use super::super::dependencies::{CargoMetadata, PackageDependencies};
use super::source::{RustTest, SourceCollector, collect_parser_sources, source_module_path};

pub(in crate::readiness) struct RustWorkspace {
    root: PathBuf,
    files: BTreeMap<String, String>,
    tests: Vec<RustTest>,
    parser_functions: Vec<String>,
    parser_test_start: usize,
    enums: BTreeMap<String, Vec<String>>,
    dependencies: CargoMetadata,
}

impl RustWorkspace {
    pub(in crate::readiness) fn load(root: PathBuf) -> Result<Self, String> {
        let root = root.canonicalize().map_err(|error| {
            format!(
                "could not resolve readiness workspace {}: {error}",
                root.display()
            )
        })?;

        let dependencies = CargoMetadata::load(&root)?;
        let mut paths = Vec::new();

        collect_rust_files(&root.join("crates"), &mut paths)?;

        paths.sort();

        let mut files = BTreeMap::new();
        let mut tests = Vec::new();
        let mut parser_functions = Vec::new();
        let mut parser_sources = BTreeMap::new();
        let mut enums: BTreeMap<String, Vec<String>> = BTreeMap::new();

        for path in paths {
            let relative = path
                .strip_prefix(&root)
                .map_err(|_| format!("{} is outside the workspace", path.display()))?
                .to_string_lossy()
                .replace('\\', "/");

            let contents = std::fs::read_to_string(&path)
                .map_err(|error| format!("could not read {}: {error}", path.display()))?;

            let file = syn::parse_file(&contents)
                .map_err(|error| format!("could not parse {relative}: {error}"))?;

            for item in &file.items {
                let syn::Item::Enum(item) = item else {
                    continue;
                };

                enums.entry(item.ident.to_string()).or_default().extend(
                    item.variants
                        .iter()
                        .map(|variant| variant.ident.to_string()),
                );
            }

            if relative.starts_with("crates/bray-parser/") {
                parser_sources.insert(relative.clone(), file);
            } else {
                SourceCollector::new(
                    &relative,
                    source_module_path(&relative),
                    &mut tests,
                    &mut parser_functions,
                )
                .visit_file(&file);
            }

            files.insert(relative, contents);
        }

        let parser_test_start = tests.len();

        if !parser_sources.is_empty() {
            let package = dependencies.package(&root.join("crates/bray-parser/Cargo.toml"))?;

            collect_parser_sources(
                &root,
                package.targets(),
                &parser_sources,
                &mut tests,
                &mut parser_functions,
            )?;
        }

        for variants in enums.values_mut() {
            variants.sort();
        }

        Ok(Self {
            root,
            files,
            tests,
            parser_functions,
            parser_test_start,
            enums,
            dependencies,
        })
    }

    pub(in crate::readiness) fn files(&self) -> &BTreeMap<String, String> {
        &self.files
    }

    pub(in crate::readiness) fn tests(&self) -> &[RustTest] {
        &self.tests
    }

    pub(in crate::readiness) fn parser_tests(&self) -> &[RustTest] {
        &self.tests[self.parser_test_start..]
    }

    pub(in crate::readiness) fn parser_functions(&self) -> &[String] {
        &self.parser_functions
    }

    pub(in crate::readiness) fn executable_test_names(&self, category: &str) -> Result<BTreeSet<&str>, String> {
        self.tests
            .iter()
            .map(|test| {
                if test.path().is_empty() || test.body().is_empty() {
                    Err(format!(
                        "executable {category} test {} has no source body",
                        test.name()
                    ))
                } else {
                    Ok(test.name())
                }
            })
            .collect()
    }

    pub(in crate::readiness) fn enum_variants(&self, name: &str) -> Result<&[String], String> {
        self.enums
            .get(name)
            .map(Vec::as_slice)
            .ok_or_else(|| format!("could not find enum {name} in the Rust workspace"))
    }

    pub(in crate::readiness) fn contains_source(&self, anchor: &str) -> bool {
        self.files
            .values()
            .any(|contents| contents.contains(anchor))
    }

    pub(in crate::readiness) fn read_fixture<T: DeserializeOwned>(&self, name: &str) -> Result<T, String> {
        let path = self
            .root
            .join("xtask")
            .join("fixtures")
            .join("readiness")
            .join(name);

        let contents = std::fs::read_to_string(&path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;

        serde_json::from_str(&contents)
            .map_err(|error| format!("could not decode {}: {error}", path.display()))
    }

    pub(in crate::readiness) fn read_text(&self, relative: &str) -> Result<String, String> {
        let path = self.root.join(relative);

        std::fs::read_to_string(&path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))
    }

    pub(in crate::readiness) fn package_dependencies(
        &self,
        relative: &str,
    ) -> Result<&PackageDependencies, String> {
        self.dependencies.package(&self.root.join(relative))
    }
}

pub(in crate::readiness) fn is_fixture_anchor(value: &str) -> bool {
    let value = value.trim();

    !value.is_empty()
        && !matches!(
            value.to_ascii_lowercase().as_str(),
            "n/a" | "none" | "pending" | "tbd" | "todo"
        )
}

pub(in crate::readiness) fn require_ordered_names<'name>(
    actual: impl IntoIterator<Item = &'name str>,
    expected: &[&str],
    category: &str,
) -> Result<(), String> {
    let actual = actual.into_iter().collect::<Vec<_>>();

    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "{category} drifted: expected {expected:?}, found {actual:?}"
        ))
    }
}

pub(in crate::readiness) fn require_unique_names<'name>(
    actual: impl IntoIterator<Item = &'name str>,
    expected: &[&str],
    category: &str,
) -> Result<(), String> {
    let actual = actual.into_iter().collect::<Vec<_>>();
    let unique = actual.iter().copied().collect::<BTreeSet<_>>();
    let expected = expected.iter().copied().collect::<BTreeSet<_>>();

    if actual.len() != unique.len() {
        return Err(format!("{category} contains duplicate names"));
    }

    if unique == expected {
        Ok(())
    } else {
        Err(format!(
            "{category} drifted: expected {expected:?}, found {unique:?}"
        ))
    }
}

pub(in crate::readiness) fn require_executable_source_contracts<'row>(
    rows: impl IntoIterator<Item = (&'row str, &'row str, &'row str)>,
    workspace: &RustWorkspace,
    category: &str,
) -> Result<(), String> {
    let tests = workspace.executable_test_names(category)?;
    let mut names = BTreeSet::new();

    for (name, production, test) in rows {
        if !names.insert(name) {
            return Err(format!("{category} coverage fixture repeats {name}"));
        }

        if !is_fixture_anchor(production) || !workspace.contains_source(production) {
            return Err(format!(
                "missing {category} production anchor for {name}: {production}"
            ));
        }

        if !is_fixture_anchor(test) || !tests.contains(test) {
            return Err(format!(
                "missing executable {category} test for {name}: {test}"
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
pub(super) mod tests {
    use super::RustWorkspace;

    pub(in crate::readiness) fn dependency_workspace(
        packages: &[(&str, &str)],
    ) -> (tempfile::TempDir, RustWorkspace) {
        let directory = tempfile::tempdir().expect("readiness dependency workspace");

        std::fs::write(
            directory.path().join("Cargo.toml"),
            r#"
            [workspace]
            members = ["crates/*"]
            resolver = "3"

            [workspace.dependencies]
            bray-ir = "1"
            mir = { package = "bray-ir", version = "1" }
        "#,
        )
        .expect("workspace manifest");

        for (name, dependencies) in packages {
            let package = directory.path().join("crates").join(name);

            std::fs::create_dir_all(package.join("src")).expect("package source directory");
            std::fs::write(package.join("src/lib.rs"), "").expect("package source");

            std::fs::write(
                package.join("Cargo.toml"),
                format!(
                    r#"
                [package]
                name = "{name}"
                version = "0.0.0"
                edition = "2024"

                {dependencies}
            "#
                ),
            )
            .expect("package manifest");
        }

        let workspace = RustWorkspace::load(directory.path().to_owned())
            .expect("readiness workspace must load without dependency resolution");

        (directory, workspace)
    }

    #[test]
    fn parser_anchors_follow_cargo_targets_module_inclusion_and_logical_names() {
        for (declaration, manifest, production, test, valid) in [
            ("", "", "parse_root", "orphan::source_case", false),
            ("mod orphan;", "", "orphan::parse_source", "orphan::source_case", true),
            ("", "", "orphan::parse_source", "root_case", false),
            ("#[path = \"orphan.rs\"] mod renamed;", "", "parse_root", "orphan::source_case", false),
            ("#[path = \"orphan.rs\"] mod renamed;", "", "renamed::parse_source", "renamed::source_case", true),
            ("mod nested;", "", "parse_root", "nested::child::source_case", true),
            ("mod separate;", "", "parse_root", "separate::child::source_case", true),
            ("mod inline { mod child; }", "", "parse_root", "inline::child::source_case", true),
            ("#[cfg(not(test))] mod orphan;", "", "parse_root", "orphan::source_case", false),
            ("#[cfg(any())] mod orphan;", "", "orphan::parse_source", "root_case", false),
            ("mod orphan;", "[lib]\ntest = false", "parse_root", "orphan::source_case", false),
            ("", "[[test]]\nname = \"literal_cases\"\npath = \"tests/different_filename.rs\"", "parse_root", "literal_cases::source_case", true),
            ("", "[[test]]\nname = \"literal_cases\"\npath = \"tests/different_filename.rs\"", "parse_root", "different_filename::source_case", false),
            ("", "", "parse_root", "different_filename::source_case", false),
        ] {
            let directory = tempfile::tempdir().expect("parser module workspace");
            let root = directory.path();

            let sources = [
                ("Cargo.toml", "[workspace]\nmembers = [\"crates/bray-parser\"]\nresolver = \"3\"".to_owned()),
                ("crates/bray-parser/Cargo.toml", format!("[package]\nname = \"bray-parser\"\nversion = \"0.0.0\"\nedition = \"2024\"\nautotests = false\n{manifest}")),
                ("crates/bray-parser/src/lib.rs", format!("pub fn parse_root() {{}}\n#[test] fn root_case() {{}}\n{declaration}")),
                ("crates/bray-parser/src/orphan.rs", "pub fn parse_source() {}\n#[test] fn source_case() {}".to_owned()),
                ("crates/bray-parser/src/nested/mod.rs", "mod child;".to_owned()),
                ("crates/bray-parser/src/nested/child.rs", "#[test] fn source_case() {}".to_owned()),
                ("crates/bray-parser/src/separate.rs", "mod child;".to_owned()),
                ("crates/bray-parser/src/separate/child.rs", "#[test] fn source_case() {}".to_owned()),
                ("crates/bray-parser/src/inline/child.rs", "#[test] fn source_case() {}".to_owned()),
                ("crates/bray-parser/tests/different_filename.rs", "#[test] fn source_case() {}".to_owned()),
                ("docs/language/syntax-grammar.ebnf", "source = identifier ;".to_owned()),
                ("crates/bray-parser/tests/fixtures/parser-coverage.md", format!("| `source` | `{production}` | `{test}` |")),
            ];

            for (path, source) in sources {
                let path = root.join(path);

                std::fs::create_dir_all(path.parent().expect("fixture parent")).expect("fixture directory");
                std::fs::write(path, source).expect("module fixture source");
            }

            let workspace = RustWorkspace::load(root.to_owned()).expect("module fixture corpus");
            let result = crate::readiness::parser::audit(&workspace);

            assert_eq!(result.is_ok(), valid, "{declaration}: {manifest}: {production}, {test}: {result:?}");

            if let Err(error) = result {
                assert!(error.contains("source"), "{error}");
                assert!(error.contains(production) || error.contains(test), "{error}");
            }
        }
    }
}
