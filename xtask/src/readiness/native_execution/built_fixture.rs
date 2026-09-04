use std::path::{Path, PathBuf};

use bray_target::NativeTarget;

use super::core::{executable_path, native_output, object_files};

pub(super) struct BuiltFixture {
    executable: bray_emitter::PublishedArtifact,
    output: tempfile::TempDir,
    objects: Vec<PathBuf>,
}

impl BuiltFixture {
    pub(super) fn build_command_line(
        prefix: &str,
        target: NativeTarget,
        build: impl FnOnce(&Path) -> Result<(), String>,
    ) -> Result<Self, String> {
        Self::build(prefix, target, "command.line", build, |output| {
            Ok(bray_emitter::ManagedFilesystemDestination::at_root(output))
        })
    }

    pub(super) fn build_standard_library(
        prefix: &str,
        target: NativeTarget,
        build: impl FnOnce(&Path) -> Result<(), String>,
    ) -> Result<Self, String> {
        Self::build(
            prefix,
            target,
            "std",
            build,
            crate::native_product::managed_destination,
        )
    }

    fn build(
        prefix: &str,
        target: NativeTarget,
        package: &str,
        build: impl FnOnce(&Path) -> Result<(), String>,
        destination: impl FnOnce(&Path) -> Result<bray_emitter::ManagedFilesystemDestination, String>,
    ) -> Result<Self, String> {
        let output = native_output(prefix)?;

        build(output.path())?;

        let executable =
            executable_path(destination(output.path())?, package).map_err(|error| {
                format!(
                    "could not resolve {prefix} fixture for package {package} in {}: {error}",
                    output.path().display()
                )
            })?;

        let objects = object_files(output.path(), target)?;

        Ok(Self {
            output,
            executable,
            objects,
        })
    }

    pub(super) fn output(&self) -> &Path {
        self.output.path()
    }

    pub(super) fn executable(&self) -> &Path {
        self.executable.path()
    }

    pub(super) fn objects(&self) -> &[PathBuf] {
        &self.objects
    }
}

#[cfg(test)]
mod tests {
    use super::BuiltFixture;

    #[test]
    fn built_fixture_owns_its_workspace_through_use_and_removes_it_on_drop() {
        let output = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("test fixture output must exist: {error:?}"));

        let directory = output.path().to_owned();
        let executable = published_fixture(&directory);
        let object = directory.join("application.obj");

        std::fs::write(&object, b"object")
            .unwrap_or_else(|error| panic!("test object must write: {error:?}"));

        let fixture = BuiltFixture {
            output,
            executable,
            objects: vec![object],
        };

        assert!(fixture.output().is_dir());
        assert!(fixture.executable().is_file());
        assert!(fixture.objects()[0].is_file());

        drop(fixture);

        assert!(!directory.exists());
    }

    fn published_fixture(root: &std::path::Path) -> bray_emitter::PublishedArtifact {
        use bray_emitter::{
            ArtifactContribution, ArtifactKind, ArtifactPublisher, ArtifactRequirement,
            EmissionPlanner, EmissionRequest, ReplacementPolicy, RequestedArtifact,
            RequestedArtifactDestination,
        };

        use bray_symbols::{PackageIdentity, ProductIdentity, ProductKind};

        let product =
            ProductIdentity::try_new(PackageIdentity::try_new("fixture").unwrap(), "application")
                .unwrap();

        let target = bray_target::NativeTarget::current().unwrap();

        let outputs = bray_target::TargetOutputDescription::for_native(
            target,
            [bray_target::TargetOutputKind::DependencyMetadata],
        );

        let kind = ArtifactKind::DependencyMetadata;

        let request = EmissionRequest::try_new(
            product.clone(),
            ProductKind::Library,
            None,
            target.identity(),
            RequestedArtifactDestination::FilesystemDirectory(root.to_owned().into()),
            [RequestedArtifact::new(kind, ArtifactRequirement::Required)],
            ReplacementPolicy::ReplaceExisting,
        )
        .unwrap();

        let plan = EmissionPlanner::new(outputs, None, None)
            .plan(request)
            .unwrap();

        let planned = &plan.artifacts()[0];
        let content = bray_codegen::ArtifactContent::try_memory(b"fixture".as_slice()).unwrap();

        let contribution = ArtifactContribution::new(
            planned.id().clone(),
            planned.producer().clone(),
            content,
            None,
        );

        let outcome = ArtifactPublisher::new(&|| false).publish(&plan, [contribution]);

        assert!(
            matches!(outcome.status(), bray_emitter::EmissionStatus::Complete),
            "{outcome:?}"
        );

        bray_emitter::resolve_published_artifact(root, &product, kind, 0).unwrap()
    }
}
