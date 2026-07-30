pub(crate) use bray_testing::{TemporaryFile, unique_temporary_directory};

use std::path::{Path, PathBuf};

pub(crate) struct ProjectWorkspace {
    path: PathBuf,
}

impl ProjectWorkspace {
    pub(crate) fn basic() -> Self {
        Self::basic_at(unique_temporary_directory())
    }

    pub(crate) fn basic_at(path: PathBuf) -> Self {
        let workspace = Self::new(path);

        workspace.write(
            "bray-workspace.json",
            r#"{
                "format": 1,
                "output_root": "build",
                "targets": [
                    {
                        "name": "native",
                        "identity": "x86_64-unknown-linux-gnu"
                    }
                ],
                "packages": [
                    {
                        "path": "app",
                        "role": "root",
                        "features": []
                    }
                ]
            }"#,
        );

        workspace.write(
            "app/bray-package.json",
            r#"{
                "format": 1,
                "identity": "example.application",
                "features": [],
                "source_roots": [
                    {
                        "name": "main",
                        "path": "src"
                    }
                ],
                "dependencies": [],
                "products": [
                    {
                        "name": "application",
                        "kind": "executable",
                        "source_roots": ["main"],
                        "targets": ["native"],
                        "outputs": ["executable"]
                    }
                ]
            }"#,
        );

        workspace.write(
            "app/src/main.bray",
            "module app;\n\nfunc main()\n{\n}\n",
        );

        workspace
    }

    pub(crate) fn with_missing_vendor() -> Self {
        let workspace = Self::basic();

        workspace.write(
            "app/bray-package.json",
            r#"{
                "format": 1,
                "identity": "example.application",
                "features": [],
                "source_roots": [
                    {
                        "name": "main",
                        "path": "src"
                    }
                ],
                "dependencies": [
                    {
                        "package": "example.math",
                        "product": "math"
                    }
                ],
                "products": [
                    {
                        "name": "application",
                        "kind": "executable",
                        "source_roots": ["main"],
                        "targets": ["native"],
                        "outputs": ["executable"]
                    }
                ]
            }"#,
        );

        workspace
    }

    pub(crate) fn with_vendor() -> Self {
        let workspace = Self::basic();

        workspace.write(
            "bray-workspace.json",
            r#"{
                "format": 1,
                "output_root": "build",
                "targets": [
                    {
                        "name": "native",
                        "identity": "x86_64-unknown-linux-gnu"
                    }
                ],
                "packages": [
                    {
                        "path": "app",
                        "role": "root",
                        "features": []
                    },
                    {
                        "path": "vendor/math",
                        "role": "vendored",
                        "features": []
                    }
                ]
            }"#,
        );

        workspace.write(
            "app/bray-package.json",
            r#"{
                "format": 1,
                "identity": "example.application",
                "features": [],
                "source_roots": [
                    {
                        "name": "main",
                        "path": "src"
                    }
                ],
                "dependencies": [
                    {
                        "package": "example.math",
                        "product": "math"
                    }
                ],
                "products": [
                    {
                        "name": "application",
                        "kind": "executable",
                        "source_roots": ["main"],
                        "targets": ["native"],
                        "outputs": ["executable"]
                    }
                ]
            }"#,
        );

        workspace.write(
            "vendor/math/bray-package.json",
            r#"{
                "format": 1,
                "identity": "example.math",
                "features": [],
                "source_roots": [
                    {
                        "name": "library",
                        "path": "src"
                    }
                ],
                "dependencies": [],
                "products": [
                    {
                        "name": "math",
                        "kind": "library",
                        "source_roots": ["library"],
                        "targets": ["native"],
                        "outputs": ["package_interface"]
                    }
                ]
            }"#,
        );

        workspace.write(
            "vendor/math/src/math.bray",
            "module math;\n",
        );

        workspace
    }

    fn new(path: PathBuf) -> Self {
        std::fs::create_dir_all(&path)
            .unwrap_or_else(|error| panic!("test workspace should be created: {error:?}"));

        Self { path }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn write(&self, relative_path: &str, contents: &str) {
        let path = relative_path
            .split('/')
            .fold(self.path.clone(), |path, part| path.join(part));

        let parent = path
            .parent()
            .unwrap_or_else(|| panic!("test file should have a parent"));

        std::fs::create_dir_all(parent)
            .unwrap_or_else(|error| panic!("test file parent should be created: {error:?}"));

        std::fs::write(path, contents)
            .unwrap_or_else(|error| panic!("test file should be written: {error:?}"));
    }
}

impl Drop for ProjectWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

pub(crate) fn package_identity() -> bray_symbols::PackageIdentity {
    match bray_symbols::PackageIdentity::try_new("test.package") {
        Some(identity) => identity,
        None => panic!("test package identity must be valid"),
    }
}
