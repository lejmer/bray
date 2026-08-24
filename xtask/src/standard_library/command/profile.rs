use std::fs;
use std::path::Path;

use bray_compilation::CompilationProfileReport;
use bray_target::TargetIdentity;

use super::error::BuildError;

pub(super) fn write_compiler_profiles(
    output: &Path,
    profiles: &[(TargetIdentity, CompilationProfileReport)],
) -> Result<(), BuildError> {
    fs::create_dir_all(output).map_err(|error| {
        BuildError::CompilerProfile(format!("could not create {}: {error}", output.display()))
    })?;

    for (target, profile) in profiles {
        let path = output.join(format!(
            "std-library-{}-standard-library-build.json",
            target.as_str()
        ));

        crate::json::write_compact(&path, profile).map_err(BuildError::CompilerProfile)?;
    }

    Ok(())
}
