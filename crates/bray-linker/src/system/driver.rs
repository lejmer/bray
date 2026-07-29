use std::sync::Arc;

use bray_base::Cancellation;
use bray_diagnostics::DiagnosticBag;

use super::response::{SystemLinkerInvocationBuildError, invocation};
use super::{SystemLinkerConfiguration, SystemLinkerFamily};
use crate::command::arguments_for;
use crate::staging::{complete_linked_outputs, validate_file_inputs};
use crate::{
    ExternalToolHost, LinkFailure, LinkOutcome, LinkPlan, LinkedProductKind, LinkerDriver,
    LinkerDriverIdentity, LinkerDriverKind,
};

/// Driver for one explicitly configured platform system linker.
pub struct SystemLinkerDriver {
    identity: LinkerDriverIdentity,
    configuration: SystemLinkerConfiguration,
    host: Arc<dyn ExternalToolHost>,
}

impl SystemLinkerDriver {
    /// Creates a driver when its identity selects the system-linker category.
    pub fn try_new(
        identity: LinkerDriverIdentity,
        configuration: SystemLinkerConfiguration,
        host: Arc<dyn ExternalToolHost>,
    ) -> Result<Self, SystemLinkerDriverBuildError> {
        if identity.kind() != LinkerDriverKind::System {
            return Err(SystemLinkerDriverBuildError::DriverKindMismatch);
        }

        Ok(Self {
            identity,
            configuration,
            host,
        })
    }

    /// Returns the exact configured linker command family.
    pub const fn family(&self) -> SystemLinkerFamily {
        self.configuration.family()
    }
}

impl LinkerDriver for SystemLinkerDriver {
    fn identity(&self) -> &LinkerDriverIdentity {
        &self.identity
    }

    fn supports(&self, target: &crate::LinkTarget, product: LinkedProductKind) -> bool {
        self.family().supports(target, product)
    }

    fn link(&self, plan: &LinkPlan, cancellation: &dyn Cancellation) -> LinkOutcome {
        if cancellation.is_cancelled() {
            return LinkOutcome::cancelled(DiagnosticBag::new());
        }

        if plan.driver() != &self.identity || !self.supports(plan.target(), plan.product_kind()) {
            return failed(LinkFailure::DriverIncompatible);
        }

        if let Err(failure) = validate_file_inputs(plan) {
            return failed(failure);
        }

        let arguments = match arguments_for(plan, self.family().flavor()) {
            Ok(arguments) => arguments,
            Err(_) => return failed(LinkFailure::DriverIncompatible),
        };

        let invocation = match invocation(&self.configuration, plan, arguments) {
            Ok(invocation) => invocation,
            Err(error) => return outcome_from_invocation_error(error),
        };

        let output = match self.host.run(&invocation, cancellation) {
            Ok(output) => output,
            Err(error) => return LinkOutcome::from_external_tool_failure(error),
        };

        if cancellation.is_cancelled() {
            return LinkOutcome::cancelled(DiagnosticBag::new());
        }

        if !output.success() {
            return failed(LinkFailure::Invocation);
        }

        complete_linked_outputs(plan)
    }
}

/// A contract violation that prevents system-linker driver construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SystemLinkerDriverBuildError {
    /// The supplied identity does not select a configured system linker.
    DriverKindMismatch,
}

fn outcome_from_invocation_error(error: SystemLinkerInvocationBuildError) -> LinkOutcome {
    match error {
        SystemLinkerInvocationBuildError::ResponseFile(_)
        | SystemLinkerInvocationBuildError::MissingPrimaryOutput => {
            failed(LinkFailure::ResponseFile)
        }
        SystemLinkerInvocationBuildError::Invocation(_) => failed(LinkFailure::Invocation),
        SystemLinkerInvocationBuildError::NonUnicodeArgument
        | SystemLinkerInvocationBuildError::UnsupportedArgument => {
            failed(LinkFailure::DriverIncompatible)
        }
    }
}

fn failed(failure: LinkFailure) -> LinkOutcome {
    LinkOutcome::failed(failure, DiagnosticBag::new())
}

#[cfg(test)]
mod tests {
    use std::ffi::{OsStr, OsString};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use bray_target::{
        CodeModel, ObjectFormat, RelocationModel, TargetArchitecture, TargetIdentity,
    };
    use bray_testing::TemporaryFile;

    use super::{SystemLinkerDriver, SystemLinkerDriverBuildError};
    use crate::test_support::{
        RecordingExternalToolHost, TestOutput, planned_output, product,
    };
    use crate::{
        ExternalToolFailure, ExternalToolHost, ExternalToolOutput, LinkFailure, LinkInput,
        LinkInputId, LinkInputKind, LinkInputMode, LinkInputProvenance, LinkInputSource, LinkModel,
        LinkPlan, LinkPlanBuilder, LinkPolicy, LinkStatus, LinkTarget, LinkedArtifactKind,
        LinkedArtifactRequirement, LinkedProductKind, LinkerDriver, LinkerDriverIdentity,
        LinkerDriverKind, SystemLinkerConfiguration, SystemLinkerFamily,
    };

    #[test]
    fn driver_construction_requires_a_system_identity() {
        let configuration = configuration(SystemLinkerFamily::Gnu, []);

        assert!(matches!(
            SystemLinkerDriver::try_new(
                driver_identity(LinkerDriverKind::ExternalLld),
                configuration,
                Arc::new(RecordingExternalToolHost::default()),
            ),
            Err(SystemLinkerDriverBuildError::DriverKindMismatch)
        ));
    }

    #[test]
    fn gnu_driver_submits_deterministic_response_file_and_explicit_environment() {
        let input = TemporaryFile::write("main.o", b"object");
        let output = TestOutput::new("application.stage");
        let host = Arc::new(RecordingExternalToolHost::writing(output.path()));
        let identity = driver_identity(LinkerDriverKind::System);
        let target = target(TargetArchitecture::X86_64, ObjectFormat::Elf);
        let plan = executable_plan(&identity, target, input.path(), output.path());

        let driver = SystemLinkerDriver::try_new(
            identity,
            configuration(
                SystemLinkerFamily::Gnu,
                [
                    (OsString::from("Z_VAR"), OsString::from("last")),
                    (OsString::from("A_VAR"), OsString::from("first")),
                ],
            ),
            Arc::clone(&host) as Arc<dyn ExternalToolHost>,
        )
        .unwrap_or_else(|error| panic!("test system linker must be valid: {error:?}"));

        let outcome = driver.link(&plan, &|| false);

        assert!(matches!(outcome.status(), LinkStatus::Complete(_)));

        let repeated = driver.link(&plan, &|| false);

        assert!(matches!(repeated.status(), LinkStatus::Complete(_)));

        let invocations = host.invocations();

        assert_eq!(invocations.len(), 2);
        assert_eq!(invocations[0], invocations[1]);

        let invocation = &invocations[0];

        assert_eq!(invocation.program(), Path::new("toolchain/system-linker"));

        assert_eq!(
            invocation.arguments(),
            [OsString::from(format!(
                "@{}.bray-link.rsp",
                output.path().display()
            ))]
        );

        assert_eq!(
            invocation.environment(),
            [
                (OsString::from("A_VAR"), OsString::from("first")),
                (OsString::from("Z_VAR"), OsString::from("last")),
            ]
        );

        assert_eq!(
            invocation.current_directory(),
            Some(Path::new("toolchain"))
        );

        assert_eq!(invocation.response_files().len(), 1);

        let escaped_output = output.path().display().to_string().replace('\\', "\\\\");
        let escaped_input = input.path().display().to_string().replace('\\', "\\\\");

        assert_eq!(
            invocation.response_files()[0].contents(),
            format!(
                "\"--build-id=none\"\n\"--pie\"\n\"-o\"\n\"{}\"\n\"--entry=_bray_host_start\"\n\"{}\"\n",
                escaped_output,
                escaped_input,
            )
            .as_bytes()
        );
    }

    #[test]
    fn microsoft_driver_uses_utf16_response_files() {
        let input = TemporaryFile::write("main.o", b"object");
        let output = TestOutput::new("application.stage");
        let host = Arc::new(RecordingExternalToolHost::writing(output.path()));
        let identity = driver_identity(LinkerDriverKind::System);
        let target = target(TargetArchitecture::X86_64, ObjectFormat::Coff);
        let plan = executable_plan(&identity, target, input.path(), output.path());

        let driver = system_driver(
            identity,
            SystemLinkerFamily::Microsoft,
            Arc::clone(&host) as Arc<dyn ExternalToolHost>,
        );

        assert!(matches!(
            driver.link(&plan, &|| false).status(),
            LinkStatus::Complete(_)
        ));

        let response = host.only_invocation().response_files()[0]
            .contents()
            .to_vec();

        assert_eq!(&response[..2], &[0xff, 0xfe]);
    }

    #[test]
    fn apple_driver_uses_direct_arguments_without_an_undocumented_response_format() {
        let input = TemporaryFile::write("main.o", b"object");
        let output = TestOutput::new("application.stage");
        let host = Arc::new(RecordingExternalToolHost::writing(output.path()));
        let identity = driver_identity(LinkerDriverKind::System);
        let target = target(TargetArchitecture::Aarch64, ObjectFormat::MachO);
        let plan = executable_plan(&identity, target, input.path(), output.path());

        let driver = system_driver(
            identity,
            SystemLinkerFamily::Apple,
            Arc::clone(&host) as Arc<dyn ExternalToolHost>,
        );

        assert!(matches!(
            driver.link(&plan, &|| false).status(),
            LinkStatus::Complete(_)
        ));

        let invocation = host.only_invocation();

        assert!(invocation.response_files().is_empty());
        assert_eq!(invocation.arguments()[0], OsStr::new("-no_uuid"));
        assert_eq!(invocation.arguments()[1], OsStr::new("-arch"));
        assert_eq!(invocation.arguments()[2], OsStr::new("arm64"));
    }

    #[test]
    fn driver_rejects_incompatible_family_target_pairs_before_invocation() {
        let input = TemporaryFile::write("main.o", b"object");
        let output = TestOutput::new("application.stage");
        let host = Arc::new(RecordingExternalToolHost::default());
        let identity = driver_identity(LinkerDriverKind::System);
        let target = target(TargetArchitecture::X86_64, ObjectFormat::Coff);
        let plan = executable_plan(&identity, target, input.path(), output.path());

        let driver = system_driver(
            identity,
            SystemLinkerFamily::Gnu,
            Arc::clone(&host) as Arc<dyn ExternalToolHost>,
        );

        assert_eq!(
            driver.link(&plan, &|| false).status(),
            &LinkStatus::Failed(LinkFailure::DriverIncompatible)
        );

        assert!(host.invocations().is_empty());
    }

    #[test]
    fn host_failures_and_cancellation_map_to_link_outcomes() {
        let input = TemporaryFile::write("main.o", b"object");
        let output = TestOutput::new("application.stage");
        let identity = driver_identity(LinkerDriverKind::System);
        let target = target(TargetArchitecture::X86_64, ObjectFormat::Elf);
        let plan = executable_plan(&identity, target, input.path(), output.path());

        let failed = system_driver(
            identity.clone(),
            SystemLinkerFamily::Gnu,
            Arc::new(RecordingExternalToolHost::failing(
                ExternalToolFailure::ProcessBudgetUnavailable,
            )),
        );

        assert_eq!(
            failed.link(&plan, &|| false).status(),
            &LinkStatus::Failed(LinkFailure::ResourceExhausted)
        );

        let cancelled = system_driver(
            identity,
            SystemLinkerFamily::Gnu,
            Arc::new(RecordingExternalToolHost::failing(
                ExternalToolFailure::Cancelled,
            )),
        );

        assert_eq!(
            cancelled.link(&plan, &|| false).status(),
            &LinkStatus::Cancelled
        );

        let tool_failure = system_driver(
            driver_identity(LinkerDriverKind::System),
            SystemLinkerFamily::Gnu,
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

    #[test]
    fn cancellation_before_invocation_never_reaches_the_host() {
        let input = TemporaryFile::write("main.o", b"object");
        let output = TestOutput::new("application.stage");
        let host = Arc::new(RecordingExternalToolHost::default());
        let identity = driver_identity(LinkerDriverKind::System);
        let target = target(TargetArchitecture::X86_64, ObjectFormat::Elf);
        let plan = executable_plan(&identity, target, input.path(), output.path());

        let driver = system_driver(
            identity,
            SystemLinkerFamily::Gnu,
            Arc::clone(&host) as Arc<dyn ExternalToolHost>,
        );

        assert_eq!(driver.link(&plan, &|| true).status(), &LinkStatus::Cancelled);
        assert!(host.invocations().is_empty());
    }

    fn executable_plan(
        driver: &LinkerDriverIdentity,
        target: LinkTarget,
        input_path: &Path,
        output_path: &Path,
    ) -> LinkPlan {
        let host =
            bray_testing::test_executable_host_contract_for(product(), target.identity().clone());

        let mut builder = LinkPlanBuilder::new(
            product(),
            LinkedProductKind::Executable,
            target,
            driver.clone(),
            LinkPolicy::new(
                crate::DeadStripPolicy::Preserve,
                crate::SectionGarbageCollectionPolicy::Preserve,
                crate::DebugLinkPolicy::None,
                None,
            ),
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

        builder.push_output(planned_output(
            0,
            LinkedArtifactKind::Executable,
            LinkedArtifactRequirement::Required,
            &output_path.to_string_lossy(),
        ));

        builder.set_executable_host(host);

        builder
            .finish()
            .unwrap_or_else(|error| panic!("test link plan must be valid: {error:?}"))
    }

    fn target(architecture: TargetArchitecture, object_format: ObjectFormat) -> LinkTarget {
        let Some(identity) = TargetIdentity::try_new("test-target") else {
            panic!("test target identity must be valid");
        };

        LinkTarget::try_new(
            identity,
            "test-target-triple",
            architecture,
            object_format,
            RelocationModel::PositionIndependent,
            CodeModel::Small,
            LinkModel::Dynamic,
        )
        .unwrap_or_else(|error| panic!("test link target must be valid: {error:?}"))
    }

    fn configuration(
        family: SystemLinkerFamily,
        environment: impl IntoIterator<Item = (OsString, OsString)>,
    ) -> SystemLinkerConfiguration {
        SystemLinkerConfiguration::try_new(
            family,
            "toolchain/system-linker",
            environment,
            Some(PathBuf::from("toolchain")),
        )
        .unwrap_or_else(|error| panic!("test configuration must be valid: {error:?}"))
    }

    fn system_driver(
        identity: LinkerDriverIdentity,
        family: SystemLinkerFamily,
        host: Arc<dyn ExternalToolHost>,
    ) -> SystemLinkerDriver {
        SystemLinkerDriver::try_new(identity, configuration(family, []), host)
            .unwrap_or_else(|error| panic!("test driver must be valid: {error:?}"))
    }

    fn driver_identity(kind: LinkerDriverKind) -> LinkerDriverIdentity {
        LinkerDriverIdentity::try_new(kind, "configured-system-linker", "1", "toolchain-1")
            .unwrap_or_else(|| panic!("test linker identity must be valid"))
    }

}
