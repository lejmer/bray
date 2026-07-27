use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use quote::ToTokens;
use syn::visit::Visit;

pub(crate) struct RustTest {
    name: String,
    path: String,
    body: String,
}

impl RustTest {
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn path(&self) -> &str {
        &self.path
    }

    pub(crate) fn body(&self) -> &str {
        &self.body
    }
}

pub(crate) fn workspace_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));

    let Some(root) = manifest_dir.parent().and_then(Path::parent) else {
        panic!("could not resolve workspace root from {manifest_dir:?}");
    };

    root.to_path_buf()
}

pub(crate) fn rust_workspace(root: &Path) -> BTreeMap<String, String> {
    let mut files = Vec::new();

    collect_rust_files(&root.join("crates"), &mut files);

    files.sort();

    files
        .into_iter()
        .map(|path| {
            let relative = path
                .strip_prefix(root)
                .unwrap_or_else(|_| panic!("{} is outside the workspace", path.display()))
                .to_string_lossy()
                .replace('\\', "/");

            let contents = match std::fs::read_to_string(&path) {
                Ok(contents) => contents,
                Err(error) => panic!("could not read {}: {error}", path.display()),
            };

            (relative, contents)
        })
        .collect()
}

pub(crate) fn rust_tests(rust: &BTreeMap<String, String>) -> Vec<RustTest> {
    let mut tests = Vec::new();

    for (path, contents) in rust {
        let file = syn::parse_file(contents)
            .unwrap_or_else(|error| panic!("could not parse {path}: {error}"));

        TestCollector {
            path,
            tests: &mut tests,
        }
        .visit_file(&file);
    }

    tests
}

fn collect_rust_files(directory: &Path, files: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) => panic!("could not read {}: {error}", directory.display()),
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => panic!("could not read directory entry: {error}"),
        };

        let path = entry.path();

        if path.is_dir() {
            collect_rust_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
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
