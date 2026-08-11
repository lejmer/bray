use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bray_base::Cancellation;
use bray_diagnostics::DiagnosticBag;

use super::{EmbeddedLldHost, LldFlavor};
use crate::capability::lld_driver_capabilities;
use crate::command::{LldPlanError, arguments_for};
use crate::outcome::failed_outcome;
use crate::staging::{complete_linked_outputs, validate_file_inputs};
use crate::{
    ExternalToolFailure, ExternalToolHost, ExternalToolInvocation,
    ExternalToolInvocationBuildError, ExternalToolOutput, LinkFailure, LinkOutcome, LinkPlan,
    LinkerDriver, LinkerDriverCapabilities, LinkerDriverCapabilitiesBuildError,
    LinkerDriverIdentity, LinkerDriverKind,
};

/// LLD driver using either a packaging-provided embedded library or an explicit executable.
pub struct LldDriver {
    capabilities: LinkerDriverCapabilities,
    host: LldHost,
}

impl LldDriver {
    /// Creates a driver backed by an LLD library packaged with the compiler.
    pub fn try_embedded(
        identity: LinkerDriverIdentity,
        host: Arc<dyn EmbeddedLldHost>,
    ) -> Result<Self, LldDriverBuildError> {
        if identity.kind() != LinkerDriverKind::EmbeddedLld {
            return Err(LldDriverBuildError::DriverKindMismatch);
        }

        let capabilities =
            lld_driver_capabilities(identity).map_err(LldDriverBuildError::Capabilities)?;

        Ok(Self {
            capabilities,
            host: LldHost::Embedded(host),
        })
    }

    /// Creates a driver backed by an explicitly configured universal LLD executable.
    pub fn try_external(
        identity: LinkerDriverIdentity,
        program: impl Into<PathBuf>,
        host: Arc<dyn ExternalToolHost>,
    ) -> Result<Self, LldDriverBuildError> {
        if identity.kind() != LinkerDriverKind::ExternalLld {
            return Err(LldDriverBuildError::DriverKindMismatch);
        }

        let program = program.into();

        if program.as_os_str().is_empty() {
            return Err(LldDriverBuildError::EmptyExternalProgram);
        }

        let capabilities =
            lld_driver_capabilities(identity).map_err(LldDriverBuildError::Capabilities)?;

        Ok(Self {
            capabilities,
            host: LldHost::External { program, host },
        })
    }

    fn run(
        &self,
        plan: &LinkPlan,
        flavor: LldFlavor,
        cancellation: &dyn Cancellation,
    ) -> Result<ExternalToolOutput, LldRunError> {
        match &self.host {
            LldHost::Embedded(host) => {
                let arguments = arguments_for(plan, flavor).map_err(|_| LldRunError::Plan)?;

                host.run(flavor, &arguments, cancellation)
                    .map_err(LldRunError::ExternalTool)
            }
            LldHost::External { program, host } => {
                let arguments = external_arguments(plan, flavor).map_err(|_| LldRunError::Plan)?;

                let invocation =
                    external_invocation(program, arguments).map_err(|_| LldRunError::Invocation)?;

                host.run(&invocation, cancellation)
                    .map_err(LldRunError::ExternalTool)
            }
        }
    }
}

impl LinkerDriver for LldDriver {
    fn capabilities(&self) -> &LinkerDriverCapabilities {
        &self.capabilities
    }

    fn link(&self, plan: &LinkPlan, cancellation: &dyn Cancellation) -> LinkOutcome {
        if cancellation.is_cancelled() {
            return LinkOutcome::cancelled(DiagnosticBag::new());
        }

        if plan.driver() != self.capabilities.identity() {
            return failed_outcome(plan, LinkFailure::DriverIncompatible);
        }

        if let Err(requirement) = self.capabilities.validate(plan) {
            return failed_outcome(plan, LinkFailure::UnsupportedRequirement(requirement));
        }

        let Some(flavor) = LldFlavor::for_target(plan.target(), plan.product_kind()) else {
            return failed_outcome(plan, LinkFailure::DriverIncompatible);
        };

        if let Err(failure) = validate_file_inputs(plan) {
            return failed_outcome(plan, failure);
        }

        let output = match self.run(plan, flavor, cancellation) {
            Ok(output) => output,
            Err(error) => return outcome_from_run_error(plan, error),
        };

        if cancellation.is_cancelled() {
            return LinkOutcome::cancelled(DiagnosticBag::new());
        }

        if !output.success() {
            return failed_outcome(plan, LinkFailure::ToolExit(output));
        }

        complete_linked_outputs(plan)
    }
}

/// A contract violation that prevents creation of an LLD driver.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LldDriverBuildError {
    /// The supplied identity does not select the requested embedded or external driver category.
    DriverKindMismatch,
    /// The configured external LLD executable path is empty.
    EmptyExternalProgram,
    /// The driver's immutable capability record is invalid.
    Capabilities(LinkerDriverCapabilitiesBuildError),
}

enum LldHost {
    Embedded(Arc<dyn EmbeddedLldHost>),
    External {
        program: PathBuf,
        host: Arc<dyn ExternalToolHost>,
    },
}

enum LldRunError {
    ExternalTool(ExternalToolFailure),
    Invocation,
    Plan,
}

fn external_invocation(
    program: &Path,
    arguments: Vec<OsString>,
) -> Result<ExternalToolInvocation, ExternalToolInvocationBuildError> {
    ExternalToolInvocation::try_new(program, arguments, [], None, [])
}

fn external_arguments(plan: &LinkPlan, flavor: LldFlavor) -> Result<Vec<OsString>, LldPlanError> {
    let mut arguments = vec![
        OsString::from("-flavor"),
        OsString::from(flavor.external_selector()),
    ];

    arguments.extend(arguments_for(plan, flavor)?);

    Ok(arguments)
}

fn outcome_from_run_error(plan: &LinkPlan, error: LldRunError) -> LinkOutcome {
    match error {
        LldRunError::ExternalTool(error) => LinkOutcome::from_external_tool_failure(plan, error),
        LldRunError::Invocation => failed_outcome(plan, LinkFailure::Invocation),
        LldRunError::Plan => failed_outcome(plan, LinkFailure::DriverIncompatible),
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use bray_base::Cancellation;
    use bray_target::{
        CodeModel, ObjectFormat, RelocationModel, TargetArchitecture, TargetIdentity,
    };
    use bray_testing::TemporaryFile;

    use super::{LldDriver, LldDriverBuildError};
    use crate::test_support::{
        RecordingExternalToolHost, TestOutput, executable_host_contract, planned_output, product,
    };
    use crate::{
        EmbeddedLldHost, ExternalToolFailure, ExternalToolHost, ExternalToolOutput, LinkFailure,
        LinkInput, LinkInputId, LinkInputKind, LinkInputMode, LinkInputProvenance, LinkInputSource,
        LinkModel, LinkPlan, LinkPlanBuilder, LinkPolicy, LinkStatus, LinkTarget,
        LinkedArtifactKind, LinkedArtifactRequirement, LinkedProductKind, LinkerDriver,
        LinkerDriverIdentity, LinkerDriverKind, LldFlavor,
    };

    #[test]
    fn driver_construction_matches_embedded_and_external_identity_kinds() {
        let embedded_host = Arc::new(RecordingEmbeddedHost::default());
        let external_host = Arc::new(RecordingExternalToolHost::default());

        assert!(matches!(
            LldDriver::try_embedded(
                driver_identity(LinkerDriverKind::ExternalLld),
                embedded_host
            ),
            Err(LldDriverBuildError::DriverKindMismatch)
        ));

        assert!(matches!(
            LldDriver::try_external(
                driver_identity(LinkerDriverKind::EmbeddedLld),
                "lld",
                external_host,
            ),
            Err(LldDriverBuildError::DriverKindMismatch)
        ));

        assert!(matches!(
            LldDriver::try_external(
                driver_identity(LinkerDriverKind::ExternalLld),
                "",
                Arc::new(RecordingExternalToolHost::default()),
            ),
            Err(LldDriverBuildError::EmptyExternalProgram)
        ));
    }

    #[test]
    fn external_driver_submits_exact_process_request_without_host_lld() {
        let input = TemporaryFile::write("main.o", b"object");
        let output = TestOutput::new("application.stage");
        let host = Arc::new(RecordingExternalToolHost::writing(output.path()));
        let identity = driver_identity(LinkerDriverKind::ExternalLld);
        let plan = executable_plan(&identity, input.path(), output.path());

        let driver = LldDriver::try_external(
            identity,
            "toolchain/lld",
            Arc::clone(&host) as Arc<dyn ExternalToolHost>,
        )
        .unwrap_or_else(|error| panic!("test LLD driver must be valid: {error:?}"));

        let outcome = driver.link(&plan, &|| false);

        assert!(matches!(outcome.status(), LinkStatus::Complete(_)));

        let invocations = host.invocations();

        assert_eq!(invocations.len(), 1);
        assert_eq!(invocations[0].program(), Path::new("toolchain/lld"));
        assert_eq!(invocations[0].arguments()[0], OsString::from("-flavor"));
        assert_eq!(invocations[0].arguments()[1], OsString::from("gnu"));
        assert!(invocations[0].environment().is_empty());
        assert!(invocations[0].response_files().is_empty());
    }

    #[test]
    fn embedded_driver_uses_packaging_provided_host() {
        let input = TemporaryFile::write("main.o", b"object");
        let output = TestOutput::new("application.stage");
        let host = Arc::new(RecordingEmbeddedHost::writing(output.path()));
        let identity = driver_identity(LinkerDriverKind::EmbeddedLld);
        let plan = executable_plan(&identity, input.path(), output.path());

        let driver =
            LldDriver::try_embedded(identity, Arc::clone(&host) as Arc<dyn EmbeddedLldHost>)
                .unwrap_or_else(|error| panic!("test LLD driver must be valid: {error:?}"));

        let outcome = driver.link(&plan, &|| false);

        assert!(matches!(outcome.status(), LinkStatus::Complete(_)));

        let calls = host
            .calls
            .lock()
            .unwrap_or_else(|error| panic!("test embedded-host lock must be available: {error:?}"));

        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, LldFlavor::Elf);
        assert_eq!(calls[0].1[0], OsString::from("--build-id=none"));
    }

    #[test]
    fn driver_rejects_incompatible_targets_and_missing_inputs() {
        let identity = driver_identity(LinkerDriverKind::EmbeddedLld);
        let host = Arc::new(RecordingEmbeddedHost::default());

        let driver = LldDriver::try_embedded(identity.clone(), host as Arc<dyn EmbeddedLldHost>)
            .unwrap_or_else(|error| panic!("test LLD driver must be valid: {error:?}"));

        assert!(!driver.capabilities().supports_target_product(
            &target(
                "powerpc64-ibm-aix",
                TargetArchitecture::PowerPc64,
                ObjectFormat::Xcoff,
            ),
            LinkedProductKind::SharedLibrary,
        ));

        let output = TestOutput::new("application.stage");
        let plan = executable_plan(&identity, Path::new("missing-input.o"), output.path());

        assert_eq!(
            driver.link(&plan, &|| false).status(),
            &LinkStatus::Failed(LinkFailure::MissingInput(LinkInputId::new(1)))
        );
    }

    #[test]
    fn successful_tool_without_staged_output_reports_the_exact_destination() {
        let input = TemporaryFile::write("main.o", b"object");
        let output = TestOutput::new("application.stage");
        let identity = driver_identity(LinkerDriverKind::EmbeddedLld);
        let plan = executable_plan(&identity, input.path(), output.path());

        let driver = LldDriver::try_embedded(identity, Arc::new(RecordingEmbeddedHost::default()))
            .unwrap_or_else(|error| panic!("test LLD driver must be valid: {error:?}"));

        assert_eq!(
            driver.link(&plan, &|| false).status(),
            &LinkStatus::Failed(LinkFailure::MissingOutput(
                crate::StagingDestinationId::new(0)
            ))
        );
    }

    fn executable_plan(
        driver: &LinkerDriverIdentity,
        input_path: &Path,
        output_path: &Path,
    ) -> LinkPlan {
        let mut builder = LinkPlanBuilder::new(
            product(),
            LinkedProductKind::Executable,
            target(
                "x86_64-unknown-linux-gnu",
                TargetArchitecture::X86_64,
                ObjectFormat::Elf,
            ),
            driver.clone(),
            crate::LinkStartupMode::ExplicitInputs,
            LinkPolicy::new(
                crate::DeadStripPolicy::Preserve,
                crate::SectionGarbageCollectionPolicy::Preserve,
                crate::DebugLinkPolicy::None,
                None,
            ),
        );

        builder.push_input(
            LinkInput::try_new(
                LinkInputId::new(1),
                LinkInputKind::StartupObject,
                LinkInputSource::file(input_path),
                LinkInputProvenance::TargetProfile,
                LinkInputMode::Ordinary,
            )
            .unwrap_or_else(|error| panic!("test startup input must be valid: {error:?}")),
        );

        builder.push_input(
            LinkInput::try_new(
                LinkInputId::new(0),
                LinkInputKind::RelocatableObject,
                LinkInputSource::file(input_path),
                LinkInputProvenance::Product,
                LinkInputMode::Ordinary,
            )
            .unwrap_or_else(|error| panic!("test link input must be valid: {error:?}")),
        );

        let output_path = output_path.to_string_lossy();

        builder.push_output(planned_output(
            0,
            LinkedArtifactKind::Executable,
            LinkedArtifactRequirement::Required,
            &output_path,
        ));

        builder.set_executable_host(executable_host_contract());

        builder
            .finish()
            .unwrap_or_else(|error| panic!("test link plan must be valid: {error:?}"))
    }

    fn target(
        triple: &str,
        architecture: TargetArchitecture,
        object_format: ObjectFormat,
    ) -> LinkTarget {
        let identity_text = if object_format == ObjectFormat::Elf {
            "linux-x86_64"
        } else {
            triple
        };

        let identity = TargetIdentity::try_new(identity_text)
            .unwrap_or_else(|| panic!("test target identity must be valid"));

        LinkTarget::try_new(
            identity,
            triple,
            architecture,
            object_format,
            RelocationModel::PositionIndependent,
            CodeModel::Small,
            LinkModel::Dynamic,
        )
        .unwrap_or_else(|error| panic!("test link target must be valid: {error:?}"))
    }

    fn driver_identity(kind: LinkerDriverKind) -> LinkerDriverIdentity {
        LinkerDriverIdentity::try_new(kind, "lld", "1", "20")
            .unwrap_or_else(|| panic!("test linker identity must be valid"))
    }

    #[derive(Default)]
    struct RecordingEmbeddedHost {
        calls: Mutex<Vec<(LldFlavor, Vec<OsString>)>>,
        output: Option<PathBuf>,
    }

    impl RecordingEmbeddedHost {
        fn writing(output: &Path) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                output: Some(output.to_path_buf()),
            }
        }
    }

    impl EmbeddedLldHost for RecordingEmbeddedHost {
        fn run(
            &self,
            flavor: LldFlavor,
            arguments: &[OsString],
            _cancellation: &dyn Cancellation,
        ) -> Result<ExternalToolOutput, ExternalToolFailure> {
            self.calls
                .lock()
                .unwrap_or_else(|error| {
                    panic!("test embedded-host lock must be available: {error:?}")
                })
                .push((flavor, arguments.to_vec()));

            write_test_output(self.output.as_deref());

            Ok(successful_output())
        }
    }

    fn write_test_output(output: Option<&Path>) {
        let Some(output) = output else {
            return;
        };

        std::fs::write(output, b"linked")
            .unwrap_or_else(|error| panic!("test linked output must be written: {error:?}"));
    }

    fn successful_output() -> ExternalToolOutput {
        ExternalToolOutput::new(true, Some(0), [], [])
    }
}
