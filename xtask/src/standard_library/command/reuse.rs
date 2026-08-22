use std::fs;
use std::path::{Path, PathBuf};

use bray_standard_library::{
    STANDARD_LIBRARY_MANIFEST_FILE_NAME, StandardLibraryArtifactDigest,
    StandardLibraryTargetArtifacts, decode_standard_library_manifest,
};
use bray_target::{NativeTarget, TargetIdentity};

use super::error::BuildError;

pub(crate) fn current_target_bundle(
    source: &Path,
    output: &Path,
    target: NativeTarget,
) -> Result<Option<PathBuf>, String> {
    let target = TargetIdentity::try_new(target.as_str())
        .ok_or_else(|| "standard library target identity is invalid".to_owned())?;

    let input = input_identity(source).map_err(|error| error.to_string())?;

    current(output, &input, std::slice::from_ref(&target))
        .map(|current| current.then(|| output.join(STANDARD_LIBRARY_MANIFEST_FILE_NAME)))
        .map_err(|error| error.to_string())
}

pub(super) fn input_identity(source: &Path) -> Result<String, BuildError> {
    let root = crate::workspace::root().map_err(BuildError::Workspace)?;

    let sources = crate::input_identity::WorkspaceSources::load(&root)
        .map_err(BuildError::InputIdentity)?;

    crate::input_identity::input_digest(
        &root,
        None,
        crate::input_identity::Component::StandardLibrary,
        &[],
        &[source],
        &sources,
    )
    .map_err(BuildError::InputIdentity)
}

pub(super) fn current(
    output: &Path,
    input: &str,
    targets: &[TargetIdentity],
) -> Result<bool, BuildError> {
    if !crate::input_identity::stored_digest_matches(output, input)
        .map_err(BuildError::InputIdentity)?
    {
        return Ok(false);
    }

    let manifest_path = output.join(STANDARD_LIBRARY_MANIFEST_FILE_NAME);

    let bytes = match fs::read(&manifest_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(BuildError::read(&manifest_path, error)),
    };

    let Ok(manifest) = decode_standard_library_manifest(&bytes) else {
        return Ok(false);
    };

    if !targets.iter().all(|target| {
        manifest
            .targets()
            .iter()
            .any(|artifacts| artifacts.target() == target)
    }) {
        return Ok(false);
    }

    for artifact in manifest
        .targets()
        .iter()
        .flat_map(StandardLibraryTargetArtifacts::artifacts)
    {
        let path = artifact.beneath(output);

        let Ok(bytes) = fs::read(path) else {
            return Ok(false);
        };

        if u64::try_from(bytes.len()).ok() != Some(artifact.byte_len())
            || StandardLibraryArtifactDigest::for_bytes(&bytes) != artifact.digest()
        {
            return Ok(false);
        }
    }

    Ok(true)
}
