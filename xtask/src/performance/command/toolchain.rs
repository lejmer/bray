use std::path::{Path, PathBuf};

use super::options::Options;
use super::progress;

pub(super) struct PreparedToolchain {
    runtime: PathBuf,
    standard_library: PathBuf,
}

impl PreparedToolchain {
    pub(super) fn runtime(&self) -> &Path {
        &self.runtime
    }

    pub(super) fn standard_library(&self) -> &Path {
        &self.standard_library
    }
}

pub(super) fn prepare(root: &Path, options: &Options) -> Result<PreparedToolchain, String> {
    if let Some((standard_library, runtime)) = &options.toolchain {
        progress::phase("Using pinned performance toolchain inputs");

        return Ok(PreparedToolchain {
            runtime: root.join(runtime),
            standard_library: root.join(standard_library),
        });
    }

    let target = options.target;
    let cargo_target = crate::workspace::cargo_target(root);

    let cache = cargo_target
        .join(".bray")
        .join("performance")
        .join(target.as_str());

    let installed = cargo_target.join("release").join("lib").join("bray");

    progress::phase("Checking installed performance runtime");

    let runtime = crate::runtime_artifact::build_for_readiness(
        target,
        &installed.join("runtime").join(target.as_str()),
    )?;

    let source = root.join("standard-library");
    let installed_standard_library = installed.join("standard-library");

    let standard_library = match crate::standard_library::current_target_bundle(
        &source,
        &installed_standard_library,
        target,
    )? {
        Some(_) => {
            progress::phase("Reusing installed performance standard library");

            installed_standard_library
        }
        None => {
            progress::phase("Checking performance standard library cache");

            let manifest = crate::standard_library::build_target_bundle(
                &source,
                &cache.join("standard-library"),
                target,
            )?;

            manifest
                .parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| "performance standard library has no bundle root".to_owned())?
        }
    };

    Ok(PreparedToolchain {
        runtime,
        standard_library,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn pinned_inputs_do_not_rebuild_or_overwrite_the_selected_packages() {
        let root = tempfile::tempdir().expect("checkout root");

        for standard_library in [
            std::path::PathBuf::from("shared/standard-library"),
            root.path().join("retained/standard-library"),
        ] {
            let options = super::Options::parse(
                [
                    "--output".to_owned(),
                    "report".to_owned(),
                    "--standard-library".to_owned(),
                    standard_library.display().to_string(),
                    "--runtime".to_owned(),
                    "shared/runtime/bray-runtime.brayrt".to_owned(),
                ]
                .into_iter(),
            )
            .expect("paired pinned inputs");

            let prepared = super::prepare(root.path(), &options).expect("select prebuilt inputs");

            assert_eq!(
                prepared.standard_library(),
                root.path().join(standard_library)
            );

            assert_eq!(
                prepared.runtime(),
                root.path().join("shared/runtime/bray-runtime.brayrt")
            );

            assert_eq!(
                std::fs::read_dir(root.path())
                    .expect("checkout contents")
                    .count(),
                0
            );
        }
    }
}
