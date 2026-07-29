use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use quote::ToTokens;
use serde::de::DeserializeOwned;
use syn::visit::Visit;

pub(super) struct RustTest {
    name: String,
    path: String,
    body: String,
}

impl RustTest {
    pub(super) fn name(&self) -> &str {
        &self.name
    }

    pub(super) fn path(&self) -> &str {
        &self.path
    }

    pub(super) fn body(&self) -> &str {
        &self.body
    }
}

pub(super) struct RustWorkspace {
    root: PathBuf,
    files: BTreeMap<String, String>,
    tests: Vec<RustTest>,
    enums: BTreeMap<String, Vec<String>>,
}

impl RustWorkspace {
    pub(super) fn load(root: PathBuf) -> Result<Self, String> {
        let mut paths = Vec::new();

        collect_rust_files(&root.join("crates"), &mut paths)?;

        paths.sort();

        let mut files = BTreeMap::new();
        let mut tests = Vec::new();
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

                enums
                    .entry(item.ident.to_string())
                    .or_default()
                    .extend(
                        item.variants
                            .iter()
                            .map(|variant| variant.ident.to_string()),
                    );
            }

            TestCollector {
                path: &relative,
                tests: &mut tests,
            }
            .visit_file(&file);

            files.insert(relative, contents);
        }

        for variants in enums.values_mut() {
            variants.sort();
        }

        Ok(Self {
            root,
            files,
            tests,
            enums,
        })
    }

    pub(super) fn files(&self) -> &BTreeMap<String, String> {
        &self.files
    }

    pub(super) fn tests(&self) -> &[RustTest] {
        &self.tests
    }

    pub(super) fn executable_test_names(
        &self,
        category: &str,
    ) -> Result<BTreeSet<&str>, String> {
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

    pub(super) fn enum_variants(&self, name: &str) -> Result<&[String], String> {
        self.enums
            .get(name)
            .map(Vec::as_slice)
            .ok_or_else(|| format!("could not find enum {name} in the Rust workspace"))
    }

    pub(super) fn contains_source(&self, anchor: &str) -> bool {
        self.files.values().any(|contents| contents.contains(anchor))
    }

    pub(super) fn read_fixture<T: DeserializeOwned>(&self, name: &str) -> Result<T, String> {
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

    pub(super) fn read_text(&self, relative: &str) -> Result<String, String> {
        let path = self.root.join(relative);

        std::fs::read_to_string(&path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))
    }
}

pub(super) fn is_fixture_anchor(value: &str) -> bool {
    let value = value.trim();

    !value.is_empty()
        && !matches!(
            value.to_ascii_lowercase().as_str(),
            "n/a" | "none" | "pending" | "tbd" | "todo"
        )
}

pub(super) fn require_ordered_names<'name>(
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

pub(super) fn require_unique_names<'name>(
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

fn collect_rust_files(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(directory)
        .map_err(|error| format!("could not read {}: {error}", directory.display()))?;

    for entry in entries {
        let entry = entry.map_err(|error| format!("could not read directory entry: {error}"))?;
        let path = entry.path();

        if path.is_dir() {
            collect_rust_files(&path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }

    Ok(())
}

struct TestCollector<'input, 'output> {
    path: &'input str,
    tests: &'output mut Vec<RustTest>,
}

impl<'syntax> Visit<'syntax> for TestCollector<'_, '_> {
    fn visit_item_fn(&mut self, function: &'syntax syn::ItemFn) {
        if function
            .attrs
            .iter()
            .any(|attribute| attribute.path().is_ident("test"))
        {
            self.tests.push(RustTest {
                name: function.sig.ident.to_string(),
                path: self.path.to_owned(),
                body: function.block.to_token_stream().to_string(),
            });
        }
    }
}
