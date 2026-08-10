use std::path::{Path, PathBuf};

pub(crate) struct ProjectFixture {
    root: PathBuf,
    pub(crate) source: PathBuf,
    pub(crate) source_text: &'static str,
}

impl ProjectFixture {
    pub(crate) fn new() -> Self {
        let root = bray_testing::unique_temporary_directory();
        let source = root.join("app").join("src").join("main.bray");

        let source_text = concat!(
            "module app;\n",
            "\n",
            "func add(left: i32, right: i32) -> i32\n",
            "{\n",
            "    return left + right;\n",
            "}\n",
            "\n",
            "func main()\n",
            "{\n",
            "    let value: i32 = add(1, 2);\n",
            "}\n",
        );

        std::fs::create_dir_all(source.parent().unwrap_or(&root))
            .unwrap_or_else(|error| panic!("test source directory should form: {error}"));

        std::fs::write(
            root.join("bray-workspace.json"),
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
            root.join("app").join("bray-package.json"),
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
        &self.root
    }
}

impl Drop for ProjectFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
