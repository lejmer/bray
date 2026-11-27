use std::path::{Path, PathBuf};

use tempfile::TempDir;

pub(crate) struct ProjectFixture {
    root: TempDir,
    pub(crate) source: PathBuf,
    pub(crate) source_text: &'static str,
}

impl ProjectFixture {
    pub(crate) fn new() -> Self {
        let root = bray_testing::temporary_directory();
        let source = root.path().join("app").join("src").join("main.bray");

        let source_text = concat!(
            "module app;\n",
            "\n",
            "func add(pos left: i32, pos right: i32) -> i32\n",
            "{\n",
            "    return left + right;\n",
            "}\n",
            "\n",
            "func main()\n",
            "{\n",
            "    let value: i32 = add(1, 2);\n",
            "}\n",
        );

        std::fs::create_dir_all(source.parent().unwrap_or(root.path()))
            .unwrap_or_else(|error| panic!("test source directory should form: {error}"));

        std::fs::write(
            root.path().join("bray-workspace.json"),
            r#"{
                "format": 1,
                "package": {"version": "0.1.0"},
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
        )
        .unwrap_or_else(|error| panic!("test workspace manifest should write: {error}"));

        std::fs::write(
            root.path().join("app").join("bray-package.json"),
            r#"{
                "format": 1,
                "identity": "example.application",
                "version": {"workspace": true},
                "features": [],
                "source_roots": [
                    {
                        "name": "main",
                        "path": "src"
                    }
                ],
                "products": [
                    {
                        "name": "application",
                        "kind": "executable",
                        "source_roots": ["main"],
                        "targets": ["native"],
                        "dependencies": [],
                        "outputs": ["executable"]
                    }
                ]
            }"#,
        )
        .unwrap_or_else(|error| panic!("test package manifest should write: {error}"));

        std::fs::write(&source, source_text)
            .unwrap_or_else(|error| panic!("test source should write: {error}"));

        Self {
            root,
            source,
            source_text,
        }
    }

    pub(crate) fn path(&self) -> &Path {
        self.root.path()
    }
}
