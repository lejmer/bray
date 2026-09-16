use std::sync::Arc;

use bray_base::Cancellation;
use bray_diagnostics::DiagnosticBag;

use super::response::{SystemLinkerInvocationBuildError, invocation};
use super::{SystemLinkerConfiguration, SystemLinkerFamily};
use crate::capability::system_driver_capabilities;
use crate::command::system_arguments_for;
use crate::optimization::OptimizationReportRequest;
use crate::outcome::failed_outcome;
use crate::staging::{complete_linked_outputs, validate_file_inputs};
use crate::{
    ExternalToolHost, LinkFailure, LinkOutcome, LinkPlan, LinkerDriver, LinkerDriverCapabilities,
    LinkerDriverCapabilitiesBuildError, LinkerDriverIdentity, LinkerDriverKind,
};

/// Driver for one explicitly configured platform system linker.
pub struct SystemLinkerDriver {
    capabilities: LinkerDriverCapabilities,
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

        // Capabilities retain their exact target after configuration moves into the driver.
        let target = configuration.target().clone();

        let accepts_thin_lto = configuration.thin_lto_cache().is_some()
            && matches!(
                configuration.family(),
                SystemLinkerFamily::GnuCompiler
                    | SystemLinkerFamily::MicrosoftCompiler
                    | SystemLinkerFamily::AppleCompiler
            );

        let capabilities =
            system_driver_capabilities(identity, configuration.family(), target, accepts_thin_lto)
                .map_err(SystemLinkerDriverBuildError::Capabilities)?;

        Ok(Self {
            capabilities,
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

        if let Err(failure) = validate_file_inputs(plan) {
            return failed_outcome(plan, failure);
        }

        let current_directory = self.configuration.invocation_directory(plan);

        let mut arguments = match system_arguments_for(plan, self.family(), current_directory) {
            Ok(arguments) => arguments,
            Err(_) => {
                return failed_outcome(plan, LinkFailure::DriverIncompatible);
            }
        };

        if let Some(output) = self.configuration.map_output() {
            arguments.extend(output.arguments(self.family(), current_directory));
        }

        if matches!(
            plan.policy().optimization(),
            crate::LinkTimeOptimizationPolicy::ThinLto { .. }
        ) {
            let Some(cache) = self.configuration.thin_lto_cache() else {
                return failed_outcome(plan, LinkFailure::DriverIncompatible);
            };

            arguments.extend(thin_lto_cache_arguments(
                self.family(),
                cache,
                current_directory,
            ));
        }

        let optimization = if matches!(
            plan.policy().optimization(),
            crate::LinkTimeOptimizationPolicy::ThinLto { .. }
        ) {
            let Some(output) = plan.primary_output() else {
                return failed_outcome(plan, LinkFailure::DriverIncompatible);
            };

            let request = OptimizationReportRequest::new(
                output.destination().path(),
                current_directory,
                self.capabilities.identity().toolchain_revision(),
                plan.target().object_format().as_str(),
            );

            if let Err(problem) = request.prepare() {
                return failed_outcome(plan, LinkFailure::OptimizationReport(problem));
            }

            Some(request)
        } else {
            None
        };

        let additional_environment: Vec<_> = optimization
            .as_ref()
            .map(OptimizationReportRequest::environment)
            .into_iter()
            .collect();

        let invocation = match invocation(
            &self.configuration,
            plan,
            arguments,
            &additional_environment,
        ) {
            Ok(invocation) => invocation,
            Err(error) => {
                if let Some(request) = &optimization {
                    request.abandon();
                }

                return outcome_from_invocation_error(plan, error);
            }
        };

        let output = match self.host.run(&invocation, cancellation) {
            Ok(output) => output,
            Err(error) => {
                if let Some(request) = &optimization {
                    request.abandon();
                }

                return LinkOutcome::from_external_tool_failure(plan, error);
            }
        };

        if cancellation.is_cancelled() {
            if let Some(request) = &optimization {
                request.abandon();
            }

            return LinkOutcome::cancelled(DiagnosticBag::new());
        }

        if !output.success() {
            if output.exit_code() == Some(86)
                && let Some(request) = &optimization
            {
                let problem = request
                    .read()
                    .err()
                    .unwrap_or(crate::LinkOptimizationReportProblem::Missing);

                request.abandon();

                return failed_outcome(plan, LinkFailure::OptimizationReport(problem));
            }

            if let Some(request) = &optimization {
                request.abandon();
            }

            return failed_outcome(plan, LinkFailure::ToolExit(output));
        }

        let report = match &optimization {
            Some(request) => match request.read() {
                Ok(report) => Some(report),
                Err(problem) => {
                    request.abandon();

                    return failed_outcome(plan, LinkFailure::OptimizationReport(problem));
                }
            },
            None => None,
        };

        let outcome = complete_linked_outputs(plan);

        match report {
            Some(report) => outcome.with_optimization_report(report),
            None => outcome,
        }
    }
}

fn thin_lto_cache_arguments(
    family: SystemLinkerFamily,
    root: &std::path::Path,
    current_directory: Option<&std::path::Path>,
) -> Vec<std::ffi::OsString> {
    let root = crate::command::linker_visible_path(root, current_directory);

    match family {
        SystemLinkerFamily::GnuCompiler => vec![
            format!("-Wl,--thinlto-cache-dir={}", root.display()).into(),
            "-Wl,--thinlto-cache-policy=cache_size_bytes=1073741824".into(),
        ],
        SystemLinkerFamily::MicrosoftCompiler => vec![
            "-Xlinker".into(),
            format!("/lldltocache:{}", root.display()).into(),
            "-Xlinker".into(),
            "/lldltocachepolicy:cache_size_bytes=1073741824".into(),
        ],
        SystemLinkerFamily::AppleCompiler => vec![
            "-Wl,-cache_path_lto".into(),
            format!("-Wl,{}", root.display()).into(),
        ],
        SystemLinkerFamily::Gnu
        | SystemLinkerFamily::Microsoft
        | SystemLinkerFamily::Apple
        | SystemLinkerFamily::WslGnuCompiler => Vec::new(),
    }
}

/// A contract violation that prevents system-linker driver construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SystemLinkerDriverBuildError {
    /// The supplied identity does not select a configured system linker.
    DriverKindMismatch,
    /// The driver's immutable capability record is invalid.
    Capabilities(LinkerDriverCapabilitiesBuildError),
}

fn outcome_from_invocation_error(
    plan: &LinkPlan,
    error: SystemLinkerInvocationBuildError,
) -> LinkOutcome {
    match error {
        SystemLinkerInvocationBuildError::ResponseFile(_)
        | SystemLinkerInvocationBuildError::MissingPrimaryOutput => {
            failed_outcome(plan, LinkFailure::DriverIncompatible)
        }
        SystemLinkerInvocationBuildError::Invocation(_) => {
            failed_outcome(plan, LinkFailure::Invocation)
        }
        SystemLinkerInvocationBuildError::NonUnicodeArgument
        | SystemLinkerInvocationBuildError::UnsupportedArgument => {
            failed_outcome(plan, LinkFailure::DriverIncompatible)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::{OsStr, OsString};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bray_diagnostics::DiagnosticKind;
    use bray_target::{ObjectFormat, TargetArchitecture};
    use bray_testing::{TemporaryFile, assert_goal_state_diagnostic_kind};

    use super::{SystemLinkerDriver, SystemLinkerDriverBuildError, thin_lto_cache_arguments};
    use crate::test_support::{
        RecordingExternalToolHost, TestOutput, planned_output, product, target,
    };
    use crate::{
        ExternalToolFailure, ExternalToolHost, ExternalToolOutput, LinkFailure, LinkInput,
        LinkInputId, LinkInputKind, LinkInputMode, LinkInputProvenance, LinkInputSource, LinkPlan,
        LinkPlanBuilder, LinkPolicy, LinkStatus, LinkTarget, LinkedArtifactKind,
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
    fn linker_map_arguments_follow_each_driver_family() {
        let path = Path::new("reports/application.map");

        let cases = [
            (
                SystemLinkerFamily::Gnu,
                vec![OsString::from("-Map"), path.as_os_str().to_owned()],
            ),
            (
                SystemLinkerFamily::Microsoft,
                vec![OsString::from("/map:reports/application.map")],
            ),
            (
                SystemLinkerFamily::Apple,
                vec![OsString::from("-map"), path.as_os_str().to_owned()],
            ),
            (
                SystemLinkerFamily::GnuCompiler,
                vec![
                    OsString::from("-Xlinker"),
                    OsString::from("-Map"),
                    OsString::from("-Xlinker"),
                    path.as_os_str().to_owned(),
                ],
            ),
            (
                SystemLinkerFamily::WslGnuCompiler,
                vec![
                    OsString::from("-Xlinker"),
                    OsString::from("-Map"),
                    OsString::from("-Xlinker"),
                    path.as_os_str().to_owned(),
                ],
            ),
            (
                SystemLinkerFamily::MicrosoftCompiler,
                vec![
                    OsString::from("-Xlinker"),
                    OsString::from("/map:reports/application.map"),
                ],
            ),
            (
                SystemLinkerFamily::AppleCompiler,
                vec![
                    OsString::from("-Xlinker"),
                    OsString::from("-map"),
                    OsString::from("-Xlinker"),
                    path.as_os_str().to_owned(),
                ],
            ),
        ];

        for (family, expected) in cases {
            let output = crate::SystemLinkerMapOutput::try_new(path)
                .unwrap_or_else(|| panic!("test map path must be valid"));

            let arguments = output.arguments(family, None);

            assert_eq!(arguments, expected);
        }
    }

    #[test]
    fn thin_lto_cache_arguments_follow_each_compiler_driver_family() {
        let root = Path::new("cache/thin-lto");

        assert_eq!(
            thin_lto_cache_arguments(SystemLinkerFamily::GnuCompiler, root, None),
            [
                OsString::from("-Wl,--thinlto-cache-dir=cache/thin-lto"),
                OsString::from("-Wl,--thinlto-cache-policy=cache_size_bytes=1073741824"),
            ]
        );

        assert_eq!(
            thin_lto_cache_arguments(SystemLinkerFamily::MicrosoftCompiler, root, None),
            [
                OsString::from("-Xlinker"),
                OsString::from("/lldltocache:cache/thin-lto"),
                OsString::from("-Xlinker"),
                OsString::from("/lldltocachepolicy:cache_size_bytes=1073741824"),
            ]
        );

        assert_eq!(
            thin_lto_cache_arguments(SystemLinkerFamily::AppleCompiler, root, None),
            [
                OsString::from("-Wl,-cache_path_lto"),
                OsString::from("-Wl,cache/thin-lto"),
            ]
        );

        for family in [
            SystemLinkerFamily::Gnu,
            SystemLinkerFamily::Microsoft,
            SystemLinkerFamily::Apple,
            SystemLinkerFamily::WslGnuCompiler,
        ] {
            assert!(thin_lto_cache_arguments(family, root, None).is_empty());
        }
    }

    #[test]
    fn thin_lto_links_publish_and_validate_exact_optimization_outcomes() {
        let input = TemporaryFile::write("main.bc", b"bitcode");
        let output = TestOutput::new("application.stage");

        let report = br#"{
            "format": 1,
            "toolchain": "toolchain-1",
            "driver": "coff",
            "imported_functions": 7,
            "imported_data": 3,
            "eliminated_functions": 4,
            "eliminated_data": 2,
            "eliminated_bytes": 128,
            "cache_hits": 5,
            "cache_misses": 2,
            "cache_writes": 2,
            "reused_partitions": 5,
            "peak_resident_bytes": 8192,
            "active_workers": 2
        }"#;

        let host = Arc::new(RecordingExternalToolHost::writing_with_optimization_report(
            output.path(),
            report,
        ));

        let identity = driver_identity(LinkerDriverKind::System);
        let target = target(TargetArchitecture::X86_64, ObjectFormat::Coff);

        let plan = executable_plan_with_optimization(
            &identity,
            target,
            input.path(),
            output.path(),
            crate::LinkStartupMode::PlatformCompilerDriver,
            crate::LinkTimeOptimizationPolicy::ThinLto {
                jobs: std::num::NonZeroUsize::MIN,
            },
        );

        let configuration = configuration(SystemLinkerFamily::MicrosoftCompiler, [])
            .try_with_thin_lto_cache("cache")
            .unwrap_or_else(|error| panic!("test ThinLTO cache must be valid: {error:?}"));

        let driver = SystemLinkerDriver::try_new(
            identity,
            configuration,
            Arc::clone(&host) as Arc<dyn ExternalToolHost>,
        )
        .unwrap_or_else(|error| panic!("test ThinLTO driver must be valid: {error:?}"));

        let outcome = driver.link(&plan, &|| false);

        let optimization = outcome
            .optimization()
            .unwrap_or_else(|| panic!("successful ThinLTO link must publish its report"));

        assert_eq!(optimization.imported_functions(), 7);
        assert_eq!(optimization.eliminated_bytes(), 128);
        assert_eq!(optimization.cache_hits(), 5);
        assert_eq!(optimization.active_workers(), 2);

        let invocation = host.only_invocation();

        let report_path = invocation
            .environment()
            .iter()
            .find_map(|(name, value)| {
                (name == crate::optimization::REPORT_ENVIRONMENT).then_some(value)
            })
            .unwrap_or_else(|| panic!("ThinLTO invocation must name its report destination"));

        assert!(!Path::new(report_path).exists());
    }

    #[test]
    fn malformed_thin_lto_reports_fail_with_the_exact_contract_problem() {
        let input = TemporaryFile::write("main.bc", b"bitcode");
        let output = TestOutput::new("application.stage");

        let host = Arc::new(RecordingExternalToolHost::writing_with_optimization_report(
            output.path(),
            b"{}",
        ));

        let identity = driver_identity(LinkerDriverKind::System);
        let target = target(TargetArchitecture::X86_64, ObjectFormat::Coff);

        let plan = executable_plan_with_optimization(
            &identity,
            target,
            input.path(),
            output.path(),
            crate::LinkStartupMode::PlatformCompilerDriver,
            crate::LinkTimeOptimizationPolicy::ThinLto {
                jobs: std::num::NonZeroUsize::MIN,
            },
        );

        let configuration = configuration(SystemLinkerFamily::MicrosoftCompiler, [])
            .try_with_thin_lto_cache("cache")
            .unwrap_or_else(|error| panic!("test ThinLTO cache must be valid: {error:?}"));

        let driver =
            SystemLinkerDriver::try_new(identity, configuration, host as Arc<dyn ExternalToolHost>)
                .unwrap_or_else(|error| panic!("test ThinLTO driver must be valid: {error:?}"));

        let outcome = driver.link(&plan, &|| false);

        assert!(matches!(
            outcome.status(),
            LinkStatus::Failed(LinkFailure::OptimizationReport(
                crate::LinkOptimizationReportProblem::Malformed
            ))
        ));

        assert_goal_state_diagnostic_kind(
            outcome.diagnostics(),
            DiagnosticKind::LinkerOptimizationReportInvalid,
        );
    }

    #[test]
    fn missing_thin_lto_reports_fail_with_the_exact_contract_problem() {
        let input = TemporaryFile::write("main.bc", b"bitcode");
        let output = TestOutput::new("application.stage");
        let host = Arc::new(RecordingExternalToolHost::writing(output.path()));
        let identity = driver_identity(LinkerDriverKind::System);
        let target = target(TargetArchitecture::X86_64, ObjectFormat::Coff);

        let plan = executable_plan_with_optimization(
            &identity,
            target,
            input.path(),
            output.path(),
            crate::LinkStartupMode::PlatformCompilerDriver,
            crate::LinkTimeOptimizationPolicy::ThinLto {
                jobs: std::num::NonZeroUsize::MIN,
            },
        );

        let configuration = configuration(SystemLinkerFamily::MicrosoftCompiler, [])
            .try_with_thin_lto_cache("cache")
            .unwrap_or_else(|error| panic!("test ThinLTO cache must be valid: {error:?}"));

        let driver =
            SystemLinkerDriver::try_new(identity, configuration, host as Arc<dyn ExternalToolHost>)
                .unwrap_or_else(|error| panic!("test ThinLTO driver must be valid: {error:?}"));

        assert!(matches!(
            driver.link(&plan, &|| false).status(),
            LinkStatus::Failed(LinkFailure::OptimizationReport(
                crate::LinkOptimizationReportProblem::Missing
            ))
        ));
    }

    #[test]
    fn cancellation_removes_incomplete_thin_lto_reports() {
        let input = TemporaryFile::write("main.bc", b"bitcode");
        let output = TestOutput::new("application.stage");

        let host = Arc::new(
            RecordingExternalToolHost::writing_with_incomplete_optimization_report(
                output.path(),
                b"partial",
            ),
        );

        let identity = driver_identity(LinkerDriverKind::System);
        let target = target(TargetArchitecture::X86_64, ObjectFormat::Coff);

        let plan = executable_plan_with_optimization(
            &identity,
            target,
            input.path(),
            output.path(),
            crate::LinkStartupMode::PlatformCompilerDriver,
            crate::LinkTimeOptimizationPolicy::ThinLto {
                jobs: std::num::NonZeroUsize::MIN,
            },
        );

        let configuration = configuration(SystemLinkerFamily::MicrosoftCompiler, [])
            .try_with_thin_lto_cache("cache")
            .unwrap_or_else(|error| panic!("test ThinLTO cache must be valid: {error:?}"));

        let driver = SystemLinkerDriver::try_new(
            identity,
            configuration,
            Arc::clone(&host) as Arc<dyn ExternalToolHost>,
        )
        .unwrap_or_else(|error| panic!("test ThinLTO driver must be valid: {error:?}"));

        let checks = AtomicUsize::new(0);
        let cancellation = || checks.fetch_add(1, Ordering::SeqCst) > 0;

        assert_eq!(
            driver.link(&plan, &cancellation).status(),
            &LinkStatus::Cancelled
        );

        let invocation = host.only_invocation();

        let report_path = invocation
            .environment()
            .iter()
            .find_map(|(name, value)| {
                (name == crate::optimization::REPORT_ENVIRONMENT).then_some(value)
            })
            .unwrap_or_else(|| panic!("ThinLTO invocation must name its report destination"));

        let partial = crate::optimization::partial_report_path(Path::new(report_path));

        assert!(!partial.exists());
    }

    #[test]
    fn thin_lto_report_identity_covers_every_supported_lld_flavor() {
        for (family, architecture, format, expected_driver) in [
            (
                SystemLinkerFamily::GnuCompiler,
                TargetArchitecture::X86_64,
                ObjectFormat::Elf,
                "elf",
            ),
            (
                SystemLinkerFamily::MicrosoftCompiler,
                TargetArchitecture::X86_64,
                ObjectFormat::Coff,
                "coff",
            ),
            (
                SystemLinkerFamily::AppleCompiler,
                TargetArchitecture::Aarch64,
                ObjectFormat::MachO,
                "macho",
            ),
        ] {
            let input = TemporaryFile::write("main.bc", b"bitcode");
            let output = TestOutput::new("application.stage");

            let report = format!(
                r#"{{
                    "format": 1,
                    "toolchain": "toolchain-1",
                    "driver": "{expected_driver}",
                    "imported_functions": 0,
                    "imported_data": 0,
                    "eliminated_functions": 0,
                    "eliminated_data": 0,
                    "eliminated_bytes": 0,
                    "cache_hits": 0,
                    "cache_misses": 1,
                    "cache_writes": 1,
                    "reused_partitions": 0,
                    "peak_resident_bytes": 4096,
                    "active_workers": 1
                }}"#,
            );

            let host = Arc::new(RecordingExternalToolHost::writing_with_optimization_report(
                output.path(),
                report.as_bytes(),
            ));

            let identity = driver_identity(LinkerDriverKind::System);
            let target = target(architecture, format);

            let plan = executable_plan_with_optimization(
                &identity,
                target,
                input.path(),
                output.path(),
                crate::LinkStartupMode::PlatformCompilerDriver,
                crate::LinkTimeOptimizationPolicy::ThinLto {
                    jobs: std::num::NonZeroUsize::MIN,
                },
            );

            let configuration = configuration(family, [])
                .try_with_thin_lto_cache("cache")
                .unwrap_or_else(|error| panic!("test ThinLTO cache must be valid: {error:?}"));

            let driver = SystemLinkerDriver::try_new(
                identity,
                configuration,
                host as Arc<dyn ExternalToolHost>,
            )
            .unwrap_or_else(|error| panic!("test ThinLTO driver must be valid: {error:?}"));

            let outcome = driver.link(&plan, &|| false);

            let optimization = outcome
                .optimization()
                .unwrap_or_else(|| panic!("{expected_driver} must publish ThinLTO telemetry"));

            assert_eq!(optimization.driver(), expected_driver);
        }
    }

    #[test]
    fn gnu_driver_submits_deterministic_response_file_and_explicit_environment() {
        let input = TemporaryFile::write("main.o", b"object");
        let output = TestOutput::new("application.stage");
        let host = Arc::new(RecordingExternalToolHost::writing(output.path()));
        let identity = driver_identity(LinkerDriverKind::System);
        let target = target(TargetArchitecture::X86_64, ObjectFormat::Elf);

        let plan = executable_plan(
            &identity,
            target,
            input.path(),
            output.path(),
            crate::LinkStartupMode::ExplicitInputs,
        );

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

        assert_eq!(invocation.current_directory(), Some(Path::new("toolchain")));

        assert_eq!(invocation.response_files().len(), 1);

        let escaped_output = output.path().display().to_string().replace('\\', "\\\\");
        let escaped_input = input.path().display().to_string().replace('\\', "\\\\");

        assert_eq!(
            invocation.response_files()[0].contents(),
            format!(
                "\"--build-id=none\"\n\"--pie\"\n\"-o\"\n\"{}\"\n\"--entry=_bray_host_start\"\n\"{}\"\n\"{}\"\n",
                escaped_output,
                escaped_input,
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

        let plan = executable_plan(
            &identity,
            target,
            input.path(),
            output.path(),
            crate::LinkStartupMode::ExplicitInputs,
        );

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
    fn microsoft_compiler_driver_uses_utf8_response_files() {
        let input = TemporaryFile::write("main.o", b"object");
        let output = TestOutput::new("application.stage");
        let host = Arc::new(RecordingExternalToolHost::writing(output.path()));
        let identity = driver_identity(LinkerDriverKind::System);
        let target = target(TargetArchitecture::X86_64, ObjectFormat::Coff);

        let plan = executable_plan(
            &identity,
            target,
            input.path(),
            output.path(),
            crate::LinkStartupMode::PlatformCompilerDriver,
        );

        let driver = system_driver(
            identity,
            SystemLinkerFamily::MicrosoftCompiler,
            Arc::clone(&host) as Arc<dyn ExternalToolHost>,
        );

        assert!(matches!(
            driver.link(&plan, &|| false).status(),
            LinkStatus::Complete(_)
        ));

        let invocation = host.only_invocation();

        assert_eq!(invocation.arguments().len(), 1);
        assert_eq!(invocation.response_files().len(), 1);

        assert!(
            !invocation.response_files()[0]
                .contents()
                .starts_with(&[0xff, 0xfe])
        );
    }

    #[test]
    fn apple_driver_uses_direct_arguments_without_an_undocumented_response_format() {
        let input = TemporaryFile::write("main.o", b"object");
        let output = TestOutput::new("application.stage");
        let host = Arc::new(RecordingExternalToolHost::writing(output.path()));
        let identity = driver_identity(LinkerDriverKind::System);
        let target = target(TargetArchitecture::Aarch64, ObjectFormat::MachO);

        let plan = executable_plan(
            &identity,
            target,
            input.path(),
            output.path(),
            crate::LinkStartupMode::ExplicitInputs,
        );

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

        let plan = executable_plan(
            &identity,
            target,
            input.path(),
            output.path(),
            crate::LinkStartupMode::ExplicitInputs,
        );

        let driver = system_driver(
            identity,
            SystemLinkerFamily::Gnu,
            Arc::clone(&host) as Arc<dyn ExternalToolHost>,
        );

        assert_eq!(
            driver.link(&plan, &|| false).status(),
            &LinkStatus::Failed(LinkFailure::UnsupportedRequirement(
                crate::UnsupportedLinkRequirement::Target {
                    identity: plan.target().identity().clone(),
                    triple: Arc::from(plan.target().triple()),
                    architecture: TargetArchitecture::X86_64,
                    object_format: ObjectFormat::Coff,
                }
            ))
        );

        assert!(host.invocations().is_empty());
    }

    #[test]
    fn host_failures_and_cancellation_map_to_link_outcomes() {
        let input = TemporaryFile::write("main.o", b"object");
        let output = TestOutput::new("application.stage");
        let identity = driver_identity(LinkerDriverKind::System);
        let target = target(TargetArchitecture::X86_64, ObjectFormat::Elf);

        let plan = executable_plan(
            &identity,
            target,
            input.path(),
            output.path(),
            crate::LinkStartupMode::ExplicitInputs,
        );

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

        let outcome = tool_failure.link(&plan, &|| false);

        assert!(matches!(
            outcome.status(),
            LinkStatus::Failed(LinkFailure::ToolExit(output))
                if output.exit_code() == Some(1)
                    && output.standard_output() == b"tool stdout"
                    && output.standard_error() == b"tool stderr"
        ));

        assert_goal_state_diagnostic_kind(
            outcome.diagnostics(),
            DiagnosticKind::LinkerExternalToolExitedUnsuccessfully,
        );
    }

    #[test]
    fn cancellation_before_invocation_never_reaches_the_host() {
        let input = TemporaryFile::write("main.o", b"object");
        let output = TestOutput::new("application.stage");
        let host = Arc::new(RecordingExternalToolHost::default());
        let identity = driver_identity(LinkerDriverKind::System);
        let target = target(TargetArchitecture::X86_64, ObjectFormat::Elf);

        let plan = executable_plan(
            &identity,
            target,
            input.path(),
            output.path(),
            crate::LinkStartupMode::ExplicitInputs,
        );

        let driver = system_driver(
            identity,
            SystemLinkerFamily::Gnu,
            Arc::clone(&host) as Arc<dyn ExternalToolHost>,
        );

        assert_eq!(
            driver.link(&plan, &|| true).status(),
            &LinkStatus::Cancelled
        );

        assert!(host.invocations().is_empty());
    }

    fn executable_plan(
        driver: &LinkerDriverIdentity,
        target: LinkTarget,
        input_path: &Path,
        output_path: &Path,
        startup_mode: crate::LinkStartupMode,
    ) -> LinkPlan {
        executable_plan_with_optimization(
            driver,
            target,
            input_path,
            output_path,
            startup_mode,
            crate::LinkTimeOptimizationPolicy::None,
        )
    }

    fn executable_plan_with_optimization(
        driver: &LinkerDriverIdentity,
        target: LinkTarget,
        input_path: &Path,
        output_path: &Path,
        startup_mode: crate::LinkStartupMode,
        optimization: crate::LinkTimeOptimizationPolicy,
    ) -> LinkPlan {
        let host =
            bray_testing::test_executable_host_contract_for(product(), target.identity().clone());

        let mut builder = LinkPlanBuilder::new(
            product(),
            LinkedProductKind::Executable,
            target,
            driver.clone(),
            startup_mode,
            LinkPolicy::new(
                crate::DeadStripPolicy::Preserve,
                crate::SectionGarbageCollectionPolicy::Preserve,
                crate::DebugLinkPolicy::None,
                None,
            )
            .with_optimization(optimization),
        );

        if startup_mode == crate::LinkStartupMode::ExplicitInputs {
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
        }

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

        builder.set_entry_point(host.native_entry().clone());

        builder
            .finish()
            .unwrap_or_else(|error| panic!("test link plan must be valid: {error:?}"))
    }

    fn configuration(
        family: SystemLinkerFamily,
        environment: impl IntoIterator<Item = (OsString, OsString)>,
    ) -> SystemLinkerConfiguration {
        let object_format = match family {
            SystemLinkerFamily::Gnu
            | SystemLinkerFamily::GnuCompiler
            | SystemLinkerFamily::WslGnuCompiler => ObjectFormat::Elf,
            SystemLinkerFamily::Microsoft | SystemLinkerFamily::MicrosoftCompiler => {
                ObjectFormat::Coff
            }
            SystemLinkerFamily::Apple | SystemLinkerFamily::AppleCompiler => ObjectFormat::MachO,
        };

        let architecture = match family {
            SystemLinkerFamily::Apple | SystemLinkerFamily::AppleCompiler => {
                TargetArchitecture::Aarch64
            }
            _ => TargetArchitecture::X86_64,
        };

        let target = crate::LinkerTargetIdentity::try_new(
            crate::test_support::target(architecture, object_format)
                .identity()
                .clone(),
            "test-target-triple",
            architecture,
            object_format,
        )
        .unwrap_or_else(|| panic!("test linker target must be valid"));

        SystemLinkerConfiguration::try_new(
            family,
            target,
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
