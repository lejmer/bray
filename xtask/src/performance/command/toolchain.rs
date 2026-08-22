use std::path::{Path, PathBuf};

use bray_target::NativeTarget;

use super::progress;

pub(super) struct PreparedToolchain {
    runtime: PathBuf,
    observation_runtime: PathBuf,
    standard_library: PathBuf,
}

impl PreparedToolchain {
    pub(super) fn runtime(&self) -> &Path {
        &self.runtime
    }

    pub(super) fn observation_runtime(&self) -> &Path {
        &self.observation_runtime
    }

    pub(super) fn standard_library(&self) -> &Path {
        &self.standard_library
    }
}

pub(super) fn prepare(root: &Path, target: NativeTarget) -> Result<PreparedToolchain, String> {
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

    progress::phase("Checking performance observation runtime");

    let observation_runtime = crate::runtime_artifact::build_for_performance_observation(
        target,
        &cache.join("observation-runtime"),
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
        observation_runtime,
        standard_library,
    })
}
