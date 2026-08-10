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
        })
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
    use super::{SystemLinkerConfiguration, SystemLinkerConfigurationBuildError};
    use crate::{LinkerTargetIdentity, SystemLinkerFamily};
    use bray_target::{ObjectFormat, TargetArchitecture, TargetIdentity};

    #[test]
    fn configurations_reject_program_names_that_require_path_discovery() {
        assert_eq!(
            SystemLinkerConfiguration::try_new(
                SystemLinkerFamily::Gnu,
                target(),
                "ld",
                [],
                None,
            ),
            Err(SystemLinkerConfigurationBuildError::ProgramPathNotExplicit)
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
