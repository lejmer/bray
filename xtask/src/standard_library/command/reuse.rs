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

    current(output, std::slice::from_ref(&target), |targets| {
        input_identity(source, targets)
    })
    .map(|current| current.then(|| output.join(STANDARD_LIBRARY_MANIFEST_FILE_NAME)))
    .map_err(|error| error.to_string())
}

pub(super) fn input_identity(
    source: &Path,
    targets: &[TargetIdentity],
) -> Result<String, BuildError> {
    let root = crate::workspace::root().map_err(BuildError::Workspace)?;

    let sources =
        crate::input_identity::WorkspaceSources::load(&root).map_err(BuildError::InputIdentity)?;

    let mut targets = targets
        .iter()
        .map(|target| {
            NativeTarget::for_identity(target)
                .ok_or_else(|| BuildError::UnsupportedTarget(target.clone()))
        })
        .collect::<Result<Vec<_>, _>>()?;

    targets.sort_unstable();

    crate::input_identity::input_digest(
        &root,
        &targets,
        crate::input_identity::Component::StandardLibrary,
        &[],
        &[source],
        &sources,
    )
    .map_err(BuildError::InputIdentity)
}

pub(super) fn current(
    output: &Path,
    targets: &[TargetIdentity],
    input_identity: impl FnOnce(&[TargetIdentity]) -> Result<String, BuildError>,
) -> Result<bool, BuildError> {
    crate::bundle::DirectoryPublication::recover(output).map_err(BuildError::Publication)?;

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

    // An installed bundle may contain more targets than this consumer requests.
    let published_targets = manifest
        .targets()
        .iter()
        .map(|artifacts| artifacts.target().clone())
        .collect::<Vec<_>>();

    let input = input_identity(&published_targets)?;

    if !crate::input_identity::stored_digest_matches(output, &input)
        .map_err(BuildError::InputIdentity)?
    {
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use bray_runtime_interface::RuntimeAbiVersion;
    use bray_standard_library::{
        STANDARD_LIBRARY_MANIFEST_FILE_NAME, StandardLibraryArtifact, StandardLibraryArtifactKind,
        StandardLibraryBundleManifest, StandardLibraryTargetArtifacts,
        encode_standard_library_manifest, standard_library_target_artifact_directory,
    };
    use bray_target::{NativeTarget, TargetIdentity};

    use super::current;

    fn publish(root: &Path, targets: &[TargetIdentity]) -> StandardLibraryBundleManifest {
        let targets = targets.iter().map(|target| {
            let abi = RuntimeAbiVersion::new(1, 0);
            let prefix = standard_library_target_artifact_directory(target, abi);

            fs::create_dir_all(root.join(&prefix)).expect("target directory");

            let artifacts = [
                (StandardLibraryArtifactKind::PackageInterface, "std.brayi"),
                (
                    StandardLibraryArtifactKind::PackageImplementation,
                    "std.brayimpl",
                ),
            ]
            .into_iter()
            .map(|(kind, name)| {
                let path = format!("{prefix}/{name}");

                fs::write(root.join(&path), b"artifact").expect("artifact");

                StandardLibraryArtifact::try_for_bytes(kind, path, b"artifact")
                    .expect("artifact record")
            });

            StandardLibraryTargetArtifacts::try_new(target.clone(), abi, artifacts)
                .expect("target inventory")
        });

        let manifest = StandardLibraryBundleManifest::try_new(targets).expect("manifest");

        fs::write(
            root.join(STANDARD_LIBRARY_MANIFEST_FILE_NAME),
            encode_standard_library_manifest(&manifest).expect("encoded manifest"),
        )
        .expect("publish manifest");

        crate::input_identity::write_digest(root, "inputs").expect("publish identity");

        manifest
    }

    #[test]
    fn installed_superset_checks_published_target_identity_and_every_artifact() {
        let directory = tempfile::tempdir().expect("standard library cache");

        let targets = [
            NativeTarget::Aarch64LinuxGnu.identity(),
            NativeTarget::X86_64LinuxGnu.identity(),
        ];

        let manifest = publish(directory.path(), &targets);

        assert!(
            current(directory.path(), &targets[..1], |actual| {
                assert_eq!(actual, targets);
                Ok("inputs".to_owned())
            })
            .expect("superset hit")
        );

        for artifact in manifest
            .targets()
            .iter()
            .flat_map(StandardLibraryTargetArtifacts::artifacts)
        {
            let path = artifact.beneath(directory.path());

            fs::write(&path, b"corrupt!").expect("same-length corruption");

            assert!(
                !current(directory.path(), &targets[..1], |_| Ok("inputs".to_owned()))
                    .expect("corruption miss")
            );

            fs::write(path, b"artifact").expect("restore");
        }

        assert!(
            !current(directory.path(), &targets, |_| Ok("changed".to_owned()))
                .expect("identity miss")
        );
    }

    #[test]
    fn incomplete_or_malformed_bundle_is_never_reused() {
        let directory = tempfile::tempdir().expect("cache");
        let target = [NativeTarget::X86_64LinuxGnu.identity()];
        let identity = |_: &[TargetIdentity]| Ok("inputs".to_owned());

        assert!(!current(directory.path(), &target, identity).expect("missing manifest"));

        let manifest = publish(directory.path(), &target);
        let artifact = manifest.targets()[0].artifacts()[0].beneath(directory.path());

        fs::rename(&artifact, directory.path().join("unpublished")).expect("missing artifact");

        assert!(!current(directory.path(), &target, identity).expect("missing artifact miss"));

        for bytes in [&b"{"[..], &[255][..]] {
            fs::write(
                directory.path().join(STANDARD_LIBRARY_MANIFEST_FILE_NAME),
                bytes,
            )
            .expect("malformed manifest");

            assert!(
                !current(directory.path(), &target, identity).expect("malformed manifest miss")
            );
        }
    }
}
