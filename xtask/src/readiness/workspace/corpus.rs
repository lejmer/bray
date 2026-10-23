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

