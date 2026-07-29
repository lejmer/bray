use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::SystemLinkerConfiguration;
use crate::external_tool::{
    ResponseFileEncoding, ResponseFileEncodingError,
    encode_response_arguments, response_file_materialization_path,
    response_file_path, response_file_reference,
};
use crate::{
    ExternalToolInvocation, ExternalToolInvocationBuildError, ExternalToolResponseFile,
    ExternalToolResponseFileBuildError, LinkPlan,
};

pub(super) fn invocation(
    configuration: &SystemLinkerConfiguration,
    plan: &LinkPlan,
    arguments: Vec<OsString>,
) -> Result<ExternalToolInvocation, SystemLinkerInvocationBuildError> {
    let (arguments, response_files) = match configuration.family().response_file_encoding() {
        Some(encoding) => {
            let (response_file, reference_path) = response_file(
                plan,
                &arguments,
                encoding,
                configuration.current_directory(),
            )?;

            let reference = response_file_reference(&reference_path);

            (vec![reference], vec![response_file])
        }
        None => (arguments, Vec::new()),
    };

    // Each process request owns its configuration so the immutable driver can serve concurrent links.
    let environment = configuration.environment().iter().cloned();

    let current_directory = configuration
        .current_directory()
        .map(std::path::Path::to_path_buf);

    ExternalToolInvocation::try_new(
        configuration.program(),
        arguments,
        environment,
        current_directory,
        response_files,
    )
    .map_err(SystemLinkerInvocationBuildError::Invocation)
}

fn response_file(
    plan: &LinkPlan,
    arguments: &[OsString],
    encoding: ResponseFileEncoding,
    current_directory: Option<&Path>,
) -> Result<(ExternalToolResponseFile, PathBuf), SystemLinkerInvocationBuildError> {
    let Some(primary_output) = plan.primary_output() else {
        return Err(SystemLinkerInvocationBuildError::MissingPrimaryOutput);
    };

    let reference_path = response_file_path(
        primary_output.destination().path(),
        ".bray-link.rsp",
    );

    let materialization_path =
        response_file_materialization_path(&reference_path, current_directory);

    let contents = encode_response_arguments(arguments, encoding)
        .map_err(|error| match error {
            ResponseFileEncodingError::NonUnicodeArgument => {
                SystemLinkerInvocationBuildError::NonUnicodeArgument
            }
            ResponseFileEncodingError::UnsupportedArgument => {
                SystemLinkerInvocationBuildError::UnsupportedArgument
            }
        })?;

    let response_file = ExternalToolResponseFile::try_new(materialization_path, contents)
        .map_err(SystemLinkerInvocationBuildError::ResponseFile)?;

    Ok((response_file, reference_path))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SystemLinkerInvocationBuildError {
    Invocation(ExternalToolInvocationBuildError),
    ResponseFile(ExternalToolResponseFileBuildError),
    MissingPrimaryOutput,
    NonUnicodeArgument,
    UnsupportedArgument,
}

#[cfg(test)]
mod tests {
    use std::ffi::{OsStr, OsString};
    use std::path::{Path, PathBuf};

    use super::invocation;
    use crate::test_support::link_plan_with_driver;
    use crate::{
        LinkerDriverIdentity, LinkerDriverKind, SystemLinkerConfiguration, SystemLinkerFamily,
    };

    #[test]
    fn relative_response_files_materialize_under_the_child_working_directory() {
        let driver = LinkerDriverIdentity::try_new(
            LinkerDriverKind::System,
            "configured-system-linker",
            "1",
            "toolchain-1",
        )
        .unwrap_or_else(|| panic!("test linker identity must be valid"));

        let plan = link_plan_with_driver(driver);

        let configuration = SystemLinkerConfiguration::try_new(
            SystemLinkerFamily::Gnu,
            "toolchain/system-linker",
            [],
            Some(PathBuf::from("toolchain")),
        )
        .unwrap_or_else(|error| panic!("test configuration must be valid: {error:?}"));

        let invocation = invocation(&configuration, &plan, vec![OsString::from("main.o")])
            .unwrap_or_else(|error| panic!("test invocation must be valid: {error:?}"));

        assert_eq!(
            invocation.arguments(),
            [OsString::from("@application.stage.bray-link.rsp")]
        );

        assert_eq!(
            invocation.response_files()[0].path(),
            Path::new("toolchain/application.stage.bray-link.rsp")
        );
    }

    #[test]
    fn response_file_references_are_single_native_arguments() {
        let path = OsStr::new("stage/output with spaces.bray-link.rsp");

        let reference = crate::external_tool::response_file_reference(
            std::path::Path::new(path),
        );

        assert_eq!(
            reference,
            OsString::from("@stage/output with spaces.bray-link.rsp")
        );
    }
}
