use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::external_tool::is_explicit_program_path;
use crate::{ExternalToolInvocation, ExternalToolInvocationBuildError, LinkerTargetIdentity};

use super::SystemLinkerFamily;

/// Explicit compiler-host configuration for one platform system linker.
#[derive(Debug, Eq, PartialEq)]
pub struct SystemLinkerConfiguration {
    family: SystemLinkerFamily,
    target: LinkerTargetIdentity,
    invocation_template: ExternalToolInvocation,
    map_output: Option<SystemLinkerMapOutput>,
}

impl SystemLinkerConfiguration {
    /// Creates a configuration after validating its program, environment, and working directory.
    pub fn try_new(
        family: SystemLinkerFamily,
        target: LinkerTargetIdentity,
        program: impl Into<PathBuf>,
        environment: impl IntoIterator<Item = (OsString, OsString)>,
        current_directory: Option<PathBuf>,
    ) -> Result<Self, SystemLinkerConfigurationBuildError> {
        let program = program.into();

        if !is_explicit_program_path(&program) {
            return Err(SystemLinkerConfigurationBuildError::ProgramPathNotExplicit);
        }

        let invocation_template =
            ExternalToolInvocation::try_new(program, [], environment, current_directory, [])
                .map_err(SystemLinkerConfigurationBuildError::Invocation)?;

        Ok(Self {
            family,
            target,
            invocation_template,
            map_output: None,
        })
    }

    /// Returns this configuration with one explicit linker-map destination.
    pub fn with_map_output(mut self, output: SystemLinkerMapOutput) -> Self {
        self.map_output = Some(output);

        self
    }

    /// Returns the configured platform linker family.
    pub const fn family(&self) -> SystemLinkerFamily {
        self.family
    }

    /// Returns the exact target configured for this system linker.
    pub const fn target(&self) -> &LinkerTargetIdentity {
        &self.target
    }

    /// Returns the exact configured linker executable path.
    pub fn program(&self) -> &Path {
        self.invocation_template.program()
    }

    /// Returns the complete explicit child environment.
    pub fn environment(&self) -> &[(OsString, OsString)] {
        self.invocation_template.environment()
    }

    /// Returns the configured child working directory.
    pub fn current_directory(&self) -> Option<&Path> {
        self.invocation_template.current_directory()
    }

    /// Returns the optional linker-map destination selected by the host workflow.
    pub const fn map_output(&self) -> Option<&SystemLinkerMapOutput> {
        self.map_output.as_ref()
    }
}

/// Validated destination for one explicitly requested linker map.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemLinkerMapOutput(PathBuf);

impl SystemLinkerMapOutput {
    /// Creates a linker-map destination when the exact path is nonempty.
    pub fn try_new(path: impl Into<PathBuf>) -> Option<Self> {
        let path = path.into();

        (!path.as_os_str().is_empty()).then_some(Self(path))
    }

    /// Returns the exact destination selected by the host workflow.
    pub fn path(&self) -> &Path {
        &self.0
    }

    /// Returns the complete map arguments for the selected system-linker family.
    pub fn arguments(&self, family: SystemLinkerFamily) -> Vec<OsString> {
        match family {
            SystemLinkerFamily::Gnu => vec!["-Map".into(), self.0.as_os_str().to_owned()],
            SystemLinkerFamily::Microsoft => {
                vec![format!("/map:{}", self.0.display()).into()]
            }
            SystemLinkerFamily::Apple => vec!["-map".into(), self.0.as_os_str().to_owned()],
            SystemLinkerFamily::GnuCompiler | SystemLinkerFamily::WslGnuCompiler => {
                let output = if family == SystemLinkerFamily::WslGnuCompiler {
                    crate::command::wsl_path(&self.0.to_string_lossy()).into()
                } else {
                    self.0.as_os_str().to_owned()
                };

                vec!["-Xlinker".into(), "-Map".into(), "-Xlinker".into(), output]
            }
            SystemLinkerFamily::MicrosoftCompiler => vec![
                "-Xlinker".into(),
                format!("/map:{}", self.0.display()).into(),
            ],
            SystemLinkerFamily::AppleCompiler => vec![
                "-Xlinker".into(),
                "-map".into(),
                "-Xlinker".into(),
                self.0.as_os_str().to_owned(),
            ],
        }
    }
}

/// A contract violation that prevents configured system-linker construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SystemLinkerConfigurationBuildError {
    /// The program does not name a path and would require implicit tool discovery.
    ProgramPathNotExplicit,
    /// The external-tool invocation configuration is invalid.
    Invocation(ExternalToolInvocationBuildError),
}

#[cfg(test)]
mod tests {
    use super::{
        SystemLinkerConfiguration, SystemLinkerConfigurationBuildError, SystemLinkerMapOutput,
    };
    use crate::{LinkerTargetIdentity, SystemLinkerFamily};
    use bray_target::{ObjectFormat, TargetArchitecture, TargetIdentity};

    #[test]
    fn configurations_reject_program_names_that_require_path_discovery() {
        assert_eq!(
            SystemLinkerConfiguration::try_new(SystemLinkerFamily::Gnu, target(), "ld", [], None,),
            Err(SystemLinkerConfigurationBuildError::ProgramPathNotExplicit)
        );
    }

    #[test]
    fn linker_map_outputs_require_an_explicit_nonempty_destination() {
        assert_eq!(SystemLinkerMapOutput::try_new(""), None);

        let output = SystemLinkerMapOutput::try_new("reports/application.map")
            .unwrap_or_else(|| panic!("nonempty map path must be valid"));

        let configuration = SystemLinkerConfiguration::try_new(
            SystemLinkerFamily::Gnu,
            target(),
            "tools/ld",
            [],
            None,
        )
        .unwrap_or_else(|error| panic!("test configuration must be valid: {error:?}"))
        .with_map_output(output);

        assert_eq!(
            configuration.map_output().map(SystemLinkerMapOutput::path),
            Some(std::path::Path::new("reports/application.map"))
        );
    }

    fn target() -> LinkerTargetIdentity {
        LinkerTargetIdentity::try_new(
            TargetIdentity::try_new("test-target")
                .unwrap_or_else(|| panic!("test target identity must be valid")),
            "x86_64-unknown-linux-gnu",
            TargetArchitecture::X86_64,
            ObjectFormat::Elf,
        )
        .unwrap_or_else(|| panic!("test linker target must be valid"))
    }
}
