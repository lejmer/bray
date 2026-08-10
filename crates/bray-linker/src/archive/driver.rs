use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;

use bray_base::Cancellation;
use bray_diagnostics::DiagnosticBag;

use super::command::{ArchiveInvocationBuildError, invocation};
use super::format::ArchiveFormat;
use crate::capability::archive_driver_capabilities;
use crate::external_tool::is_explicit_program_path;
use crate::outcome::failed_outcome;
use crate::staging::{complete_linked_outputs, validate_file_inputs};
use crate::{
    ExternalToolHost, ExternalToolInvocation, ExternalToolInvocationBuildError, LinkFailure,
    LinkOutcome, LinkPlan, LinkerDriver, LinkerDriverCapabilities,
    LinkerDriverCapabilitiesBuildError, LinkerDriverIdentity, LinkerDriverKind,
};

/// Linker-domain driver for deterministic LLVM static-library archives.
pub struct LlvmArchiveDriver {
    capabilities: LinkerDriverCapabilities,
    invocation_template: ExternalToolInvocation,
    host: Arc<dyn ExternalToolHost>,
}

impl LlvmArchiveDriver {
    /// Creates a driver from one explicit LLVM archiver program and host contract.
    pub fn try_new(
        identity: LinkerDriverIdentity,
        program: impl Into<PathBuf>,
        environment: impl IntoIterator<Item = (OsString, OsString)>,
        current_directory: Option<PathBuf>,
        host: Arc<dyn ExternalToolHost>,
    ) -> Result<Self, LlvmArchiveDriverBuildError> {
        if identity.kind() != LinkerDriverKind::Archiver {
            return Err(LlvmArchiveDriverBuildError::DriverKindMismatch);
        }

        let program = program.into();

        if !is_explicit_program_path(&program) {
            return Err(LlvmArchiveDriverBuildError::ProgramPathNotExplicit);
        }

        let invocation_template =
            ExternalToolInvocation::try_new(program, [], environment, current_directory, [])
                .map_err(LlvmArchiveDriverBuildError::Invocation)?;

        let capabilities = archive_driver_capabilities(identity)
            .map_err(LlvmArchiveDriverBuildError::Capabilities)?;

        Ok(Self {
            capabilities,
            invocation_template,
            host,
        })
    }
}

impl LinkerDriver for LlvmArchiveDriver {
    fn capabilities(&self) -> &LinkerDriverCapabilities {
        &self.capabilities
    }

    fn link(&self, plan: &LinkPlan, cancellation: &dyn Cancellation) -> LinkOutcome {
        if cancellation.is_cancelled() {
            return LinkOutcome::cancelled(DiagnosticBag::new());
        }

        if plan.driver() != self.capabilities.identity() {
            return failed_outcome(LinkFailure::DriverIncompatible);
        }

        if let Err(requirement) = self.capabilities.validate(plan) {
            return failed_outcome(LinkFailure::UnsupportedRequirement(requirement));
        }

        if let Err(failure) = validate_file_inputs(plan) {
            return failed_outcome(failure);
        }

        let Some(format) = ArchiveFormat::for_target(plan.target()) else {
            return failed_outcome(LinkFailure::DriverIncompatible);
        };

        let invocation = match invocation(&self.invocation_template, plan, format) {
            Ok(invocation) => invocation,
            Err(error) => {
                return outcome_from_invocation_error(error);
            }
        };

        if let Err(failure) = clear_staging_output(plan) {
            return failed_outcome(failure);
        }

        let output = match self.host.run(&invocation, cancellation) {
            Ok(output) => output,
            Err(error) => {
                return LinkOutcome::from_external_tool_failure(error);
            }
        };

        if cancellation.is_cancelled() {
            return LinkOutcome::cancelled(DiagnosticBag::new());
        }

        if !output.success() {
            return failed_outcome(LinkFailure::Invocation);
        }

        complete_linked_outputs(plan)
    }
}

/// A contract violation that prevents LLVM archive-driver construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LlvmArchiveDriverBuildError {
    /// The supplied identity does not select the archiver category.
    DriverKindMismatch,
    /// The driver's immutable capability record is invalid.
    Capabilities(LinkerDriverCapabilitiesBuildError),
    /// The archiver program would require implicit host tool discovery.
    ProgramPathNotExplicit,
    /// The external-tool invocation configuration is invalid.
    Invocation(ExternalToolInvocationBuildError),
}

fn clear_staging_output(plan: &LinkPlan) -> Result<(), LinkFailure> {
    let Some(output) = plan.primary_output() else {
        return Err(LinkFailure::DriverIncompatible);
    };

    match std::fs::remove_file(output.destination().path()) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(LinkFailure::Invocation),
    }
}

fn outcome_from_invocation_error(error: ArchiveInvocationBuildError) -> LinkOutcome {
    match error {
        ArchiveInvocationBuildError::InvalidPlan
        | ArchiveInvocationBuildError::ResponseEncoding(_) => {
            failed_outcome(LinkFailure::DriverIncompatible)
        }
        ArchiveInvocationBuildError::Invocation(_) => failed_outcome(LinkFailure::Invocation),
        ArchiveInvocationBuildError::ResponseFile(_) => failed_outcome(LinkFailure::ResponseFile),
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use bray_base::Cancellation;
    use bray_target::{ObjectFormat, TargetArchitecture};
    use bray_testing::TemporaryFile;

    use super::{LlvmArchiveDriver, LlvmArchiveDriverBuildError};
    use crate::test_support::{
        RecordingExternalToolHost, TestOutput, archive_target as target, planned_output, product,
    };
    use crate::{
        BinarySymbolName, DeadStripPolicy, DebugLinkPolicy, ExternalToolFailure, ExternalToolHost,
        ExternalToolInvocation, ExternalToolOutput, LinkFailure, LinkInput, LinkInputId,
        LinkInputKind, LinkInputMode, LinkInputProvenance, LinkInputSource, LinkPlan,
        LinkPlanBuilder, LinkPolicy, LinkSearchPath, LinkSearchPathKind, LinkStatus, LinkSubsystem,
        LinkTarget, LinkedArtifactKind, LinkedArtifactRequirement, LinkedProductKind, LinkerDriver,
        LinkerDriverIdentity, LinkerDriverKind, SectionGarbageCollectionPolicy,
    };

    #[test]
    fn construction_requires_an_archiver_identity_and_explicit_program() {
        assert!(matches!(
            LlvmArchiveDriver::try_new(
                driver_identity(LinkerDriverKind::ExternalLld),
                "toolchain/llvm-ar",
                [],
                None,
                Arc::new(RecordingExternalToolHost::default()),
            ),
            Err(LlvmArchiveDriverBuildError::DriverKindMismatch)
        ));

        assert!(matches!(
            LlvmArchiveDriver::try_new(
                driver_identity(LinkerDriverKind::Archiver),
                "llvm-ar",
                [],
                None,
                Arc::new(RecordingExternalToolHost::default()),
            ),
            Err(LlvmArchiveDriverBuildError::ProgramPathNotExplicit)
        ));
    }

    #[test]
    fn driver_preserves_input_order_and_requests_deterministic_indexed_archives() {
        let first = TemporaryFile::write("first object.o", b"first");
        let second = TemporaryFile::write("second.o", b"second");
        let output = TestOutput::new("library.stage");

        let host = Arc::new(RecordingExternalToolHost::writing(output.path()));

        let identity = driver_identity(LinkerDriverKind::Archiver);

        let plan = archive_plan(
            &identity,
            target(TargetArchitecture::X86_64, ObjectFormat::Elf),
            [
                (second.path(), LinkInputKind::RelocatableObject),
                (first.path(), LinkInputKind::RelocatableObject),
            ],
            output.path(),
        );

        let driver = driver(identity, Arc::clone(&host) as Arc<dyn ExternalToolHost>);

        assert!(matches!(
            driver.link(&plan, &|| false).status(),
            LinkStatus::Complete(_)
        ));

        assert!(matches!(
            driver.link(&plan, &|| false).status(),
            LinkStatus::Complete(_)
        ));

        let invocations = host.invocations();

        assert_eq!(invocations.len(), 2);
        assert_eq!(invocations[0], invocations[1]);

        let invocation = &invocations[0];

        assert_eq!(invocation.program(), Path::new("toolchain/llvm-ar"));

        assert_eq!(
            invocation.arguments(),
            [
                OsString::from("--rsp-quoting=posix"),
                OsString::from("--format=gnu"),
                OsString::from("qcsD"),
                output.path().as_os_str().to_os_string(),
                OsString::from(format!("@{}.bray-archive.rsp", output.path().display())),
            ]
        );

        assert_eq!(
            invocation.environment(),
            [
                (OsString::from("A_VAR"), OsString::from("first")),
                (OsString::from("Z_VAR"), OsString::from("last")),
            ]
        );

        assert_eq!(invocation.response_files().len(), 1);

        let escaped_second = escaped_response_path(second.path());
        let escaped_first = escaped_response_path(first.path());

        assert_eq!(
            invocation.response_files()[0].contents(),
            format!("\"{escaped_second}\"\n\"{escaped_first}\"\n").as_bytes()
        );
    }

    #[test]
    fn driver_selects_explicit_formats_for_supported_targets() {
        let cases = [
            (
                TargetArchitecture::X86_64,
                ObjectFormat::Coff,
                "--format=coff",
            ),
            (
                TargetArchitecture::Aarch64,
                ObjectFormat::MachO,
                "--format=darwin",
            ),
            (
                TargetArchitecture::Wasm32,
                ObjectFormat::WebAssembly,
                "--format=gnu",
            ),
            (
                TargetArchitecture::PowerPc64,
                ObjectFormat::Xcoff,
                "--format=bigarchive",
            ),
        ];

        for (architecture, object_format, expected) in cases {
            let input = TemporaryFile::write("input.o", b"object");
            let output = TestOutput::new("library.stage");

            let host = Arc::new(RecordingExternalToolHost::writing(output.path()));

            let identity = driver_identity(LinkerDriverKind::Archiver);

            let plan = archive_plan(
                &identity,
                target(architecture, object_format),
                [(input.path(), LinkInputKind::RelocatableObject)],
                output.path(),
            );

            let driver = driver(identity, Arc::clone(&host) as Arc<dyn ExternalToolHost>);

            assert!(matches!(
                driver.link(&plan, &|| false).status(),
                LinkStatus::Complete(_)
            ));

            assert_eq!(
                host.only_invocation().arguments()[1],
                OsString::from(expected)
            );
        }
    }

    #[test]
    fn driver_rejects_unsupported_targets_and_non_object_inputs() {
        let input = TemporaryFile::write("input.bc", b"bitcode");
        let output = TestOutput::new("library.stage");
        let host = Arc::new(RecordingExternalToolHost::default());
        let identity = driver_identity(LinkerDriverKind::Archiver);

        let unsupported_target = target(TargetArchitecture::Arm, ObjectFormat::MachO);

        let unsupported_plan = archive_plan(
            &identity,
            unsupported_target,
            [(input.path(), LinkInputKind::RelocatableObject)],
            output.path(),
        );

        let driver = driver(
            identity.clone(),
            Arc::clone(&host) as Arc<dyn ExternalToolHost>,
        );

        assert_eq!(
            driver.link(&unsupported_plan, &|| false).status(),
            &LinkStatus::Failed(LinkFailure::UnsupportedRequirement(
                crate::UnsupportedLinkRequirement::Target {
                    identity: unsupported_plan.target().identity().clone(),
                    triple: Arc::from(unsupported_plan.target().triple()),
                    architecture: TargetArchitecture::Arm,
                    object_format: ObjectFormat::MachO,
                }
            ))
        );

        let bitcode_plan = archive_plan(
            &identity,
            target(TargetArchitecture::X86_64, ObjectFormat::Elf),
            [(input.path(), LinkInputKind::Bitcode)],
            output.path(),
        );

        assert_eq!(
            driver.link(&bitcode_plan, &|| false).status(),
            &LinkStatus::Failed(LinkFailure::UnsupportedRequirement(
                crate::UnsupportedLinkRequirement::Input(LinkInputKind::Bitcode)
            ))
        );

        assert!(host.invocations().is_empty());
    }

    #[test]
    fn driver_rejects_link_only_archive_semantics() {
        let input = TemporaryFile::write("input.o", b"object");
        let output = TestOutput::new("library.stage");
        let host = Arc::new(RecordingExternalToolHost::default());
        let identity = driver_identity(LinkerDriverKind::Archiver);

        let cases: [(fn(&mut LinkPlanBuilder), crate::UnsupportedLinkRequirement); 6] = [
            (
                |builder| builder.push_exported_symbol(binary_symbol_name("exported")),
                crate::UnsupportedLinkRequirement::Symbol(
                    crate::LinkSymbolRequirement::ExportedSymbols,
                ),
            ),
            (
                |builder| builder.push_retained_symbol(binary_symbol_name("retained")),
                crate::UnsupportedLinkRequirement::Symbol(
                    crate::LinkSymbolRequirement::RetainedSymbols,
                ),
            ),
            (
                |builder| {
                    builder.push_search_path(
                        LinkSearchPath::try_new(LinkSearchPathKind::Library, "native-libraries")
                            .unwrap_or_else(|error| {
                                panic!("test search path must be valid: {error:?}")
                            }),
                    );
                },
                crate::UnsupportedLinkRequirement::SearchPath(LinkSearchPathKind::Library),
            ),
            (
                |builder| {
                    builder.set_policy(archive_policy(
                        DeadStripPolicy::RemoveUnreachable,
                        SectionGarbageCollectionPolicy::Preserve,
                        None,
                    ));
                },
                crate::UnsupportedLinkRequirement::DeadStrip(DeadStripPolicy::RemoveUnreachable),
            ),
            (
                |builder| {
                    builder.set_policy(archive_policy(
                        DeadStripPolicy::Preserve,
                        SectionGarbageCollectionPolicy::RemoveUnreferenced,
                        None,
                    ));
                },
                crate::UnsupportedLinkRequirement::SectionGarbageCollection(
                    SectionGarbageCollectionPolicy::RemoveUnreferenced,
                ),
            ),
            (
                |builder| {
                    builder.set_policy(archive_policy(
                        DeadStripPolicy::Preserve,
                        SectionGarbageCollectionPolicy::Preserve,
                        Some(LinkSubsystem::Console),
                    ));
                },
                crate::UnsupportedLinkRequirement::Subsystem(LinkSubsystem::Console),
            ),
        ];

        let driver = driver(
            identity.clone(),
            Arc::clone(&host) as Arc<dyn ExternalToolHost>,
        );

        for (configure, unsupported) in cases {
            let plan = archive_plan_with(
                &identity,
                target(TargetArchitecture::X86_64, ObjectFormat::Elf),
                [(input.path(), LinkInputKind::RelocatableObject)],
                output.path(),
                configure,
            );

            assert_eq!(
                driver.link(&plan, &|| false).status(),
                &LinkStatus::Failed(LinkFailure::UnsupportedRequirement(unsupported))
            );
        }

        assert!(host.invocations().is_empty());
    }

    #[test]
    fn missing_inputs_fail_before_invocation() {
        let output = TestOutput::new("library.stage");
        let host = Arc::new(RecordingExternalToolHost::default());
        let identity = driver_identity(LinkerDriverKind::Archiver);

        let plan = archive_plan(
            &identity,
            target(TargetArchitecture::X86_64, ObjectFormat::Elf),
            [(
                Path::new("missing-object.o"),
                LinkInputKind::RelocatableObject,
            )],
            output.path(),
        );

        let driver = driver(identity, Arc::clone(&host) as Arc<dyn ExternalToolHost>);

        assert_eq!(
            driver.link(&plan, &|| false).status(),
            &LinkStatus::Failed(LinkFailure::MissingInput(LinkInputId::new(0)))
        );

        assert!(host.invocations().is_empty());
    }

    #[test]
    fn driver_removes_stale_archive_content_before_invocation() {
        let input = TemporaryFile::write("input.o", b"object");
        let output = TestOutput::new("library.stage");

        std::fs::write(output.path(), b"stale archive")
            .unwrap_or_else(|error| panic!("stale test output must be written: {error:?}"));

        let identity = driver_identity(LinkerDriverKind::Archiver);

        let plan = archive_plan(
            &identity,
            target(TargetArchitecture::X86_64, ObjectFormat::Elf),
            [(input.path(), LinkInputKind::RelocatableObject)],
            output.path(),
        );

        let driver = driver(
            identity,
            Arc::new(CleanOutputHost {
                output: output.path().to_path_buf(),
            }),
        );

        assert!(matches!(
            driver.link(&plan, &|| false).status(),
            LinkStatus::Complete(_)
        ));
    }

    #[test]
    fn cancellation_and_host_failures_use_shared_link_outcomes() {
        let input = TemporaryFile::write("input.o", b"object");
        let output = TestOutput::new("library.stage");
        let identity = driver_identity(LinkerDriverKind::Archiver);

        let plan = archive_plan(
            &identity,
            target(TargetArchitecture::X86_64, ObjectFormat::Elf),
            [(input.path(), LinkInputKind::RelocatableObject)],
            output.path(),
        );

        let cancelled_host = Arc::new(RecordingExternalToolHost::default());

        let cancelled = driver(
            identity.clone(),
            Arc::clone(&cancelled_host) as Arc<dyn ExternalToolHost>,
        );

        assert_eq!(
            cancelled.link(&plan, &|| true).status(),
            &LinkStatus::Cancelled
        );

        assert!(cancelled_host.invocations().is_empty());

        let exhausted = driver(
            identity.clone(),
            Arc::new(RecordingExternalToolHost::failing(
                ExternalToolFailure::ProcessBudgetUnavailable,
            )),
        );

        assert_eq!(
            exhausted.link(&plan, &|| false).status(),
            &LinkStatus::Failed(LinkFailure::ResourceExhausted)
        );

        let tool_failure = driver(
            identity,
            Arc::new(RecordingExternalToolHost::reporting(
                ExternalToolOutput::new(
                    false,
                    Some(1),
                    b"tool stdout".as_slice(),
                    b"tool stderr".as_slice(),
                ),
            )),
        );

        assert_eq!(
            tool_failure.link(&plan, &|| false).status(),
            &LinkStatus::Failed(LinkFailure::Invocation)
        );
    }

    fn driver(
        identity: LinkerDriverIdentity,
        host: Arc<dyn ExternalToolHost>,
    ) -> LlvmArchiveDriver {
        LlvmArchiveDriver::try_new(
            identity,
            "toolchain/llvm-ar",
            [
                (OsString::from("Z_VAR"), OsString::from("last")),
                (OsString::from("A_VAR"), OsString::from("first")),
            ],
            None,
            host,
        )
        .unwrap_or_else(|error| panic!("test archive driver must be valid: {error:?}"))
    }

    fn archive_plan<'a>(
        driver: &LinkerDriverIdentity,
        target: LinkTarget,
        inputs: impl IntoIterator<Item = (&'a Path, LinkInputKind)>,
        output: &Path,
    ) -> LinkPlan {
        archive_plan_with(driver, target, inputs, output, |_| {})
    }

    fn archive_plan_with<'a>(
        driver: &LinkerDriverIdentity,
        target: LinkTarget,
        inputs: impl IntoIterator<Item = (&'a Path, LinkInputKind)>,
        output: &Path,
        configure: impl FnOnce(&mut LinkPlanBuilder),
    ) -> LinkPlan {
        let mut builder = LinkPlanBuilder::new(
            product(),
            LinkedProductKind::StaticLibrary,
            target,
            driver.clone(),
            crate::LinkStartupMode::NotApplicable,
            archive_policy(
                DeadStripPolicy::Preserve,
                SectionGarbageCollectionPolicy::Preserve,
                None,
            ),
        );

        for (ordinal, (path, kind)) in inputs.into_iter().enumerate() {
            builder.push_input(
                LinkInput::try_new(
                    LinkInputId::new(u32::try_from(ordinal).unwrap_or(u32::MAX)),
                    kind,
                    LinkInputSource::file(path),
                    LinkInputProvenance::Product,
                    LinkInputMode::Ordinary,
                )
                .unwrap_or_else(|error| panic!("test link input must be valid: {error:?}")),
            );
        }

        builder.push_output(planned_output(
            0,
            LinkedArtifactKind::StaticLibrary,
            LinkedArtifactRequirement::Required,
            &output.to_string_lossy(),
        ));

        configure(&mut builder);

        builder
            .finish()
            .unwrap_or_else(|error| panic!("test archive plan must be valid: {error:?}"))
    }

    fn archive_policy(
        dead_strip: DeadStripPolicy,
        section_garbage_collection: SectionGarbageCollectionPolicy,
        subsystem: Option<LinkSubsystem>,
    ) -> LinkPolicy {
        LinkPolicy::new(
            dead_strip,
            section_garbage_collection,
            DebugLinkPolicy::None,
            subsystem,
        )
    }

    fn binary_symbol_name(name: &str) -> BinarySymbolName {
        BinarySymbolName::try_new(name)
            .unwrap_or_else(|| panic!("test binary symbol name must be valid"))
    }

    fn driver_identity(kind: LinkerDriverKind) -> LinkerDriverIdentity {
        LinkerDriverIdentity::try_new(kind, "llvm-archive", "1", "22.1.8")
            .unwrap_or_else(|| panic!("test archive identity must be valid"))
    }

    fn escaped_response_path(path: &Path) -> String {
        path.display().to_string().replace('\\', "\\\\")
    }

    struct CleanOutputHost {
        output: PathBuf,
    }

    impl ExternalToolHost for CleanOutputHost {
        fn run(
            &self,
            _invocation: &ExternalToolInvocation,
            _cancellation: &dyn Cancellation,
        ) -> Result<ExternalToolOutput, ExternalToolFailure> {
            assert!(!self.output.exists());

            std::fs::write(&self.output, b"archive")
                .unwrap_or_else(|error| panic!("test archive must be written: {error:?}"));

            Ok(ExternalToolOutput::new(true, Some(0), [], []))
        }
    }
}
