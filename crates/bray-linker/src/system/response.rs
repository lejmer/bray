use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::SystemLinkerConfiguration;
use super::family::ResponseFileEncoding;
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

    let reference_path = response_file_path(primary_output.destination().path());
    let materialization_path = materialization_path(&reference_path, current_directory);
    let contents = encode_arguments(arguments, encoding)?;

    let response_file = ExternalToolResponseFile::try_new(materialization_path, contents)
        .map_err(SystemLinkerInvocationBuildError::ResponseFile)?;

    Ok((response_file, reference_path))
}

fn response_file_path(output: &Path) -> PathBuf {
    let mut path = output.as_os_str().to_os_string();

    path.push(".bray-link.rsp");

    path.into()
}

fn materialization_path(reference: &Path, current_directory: Option<&Path>) -> PathBuf {
    match current_directory {
        Some(directory) if reference.is_relative() => directory.join(reference),
        _ => reference.to_path_buf(),
    }
}

fn response_file_reference(path: &Path) -> OsString {
    let mut argument = OsString::from("@");

    argument.push(path);

    argument
}

fn encode_arguments(
    arguments: &[OsString],
    encoding: ResponseFileEncoding,
) -> Result<Vec<u8>, SystemLinkerInvocationBuildError> {
    let arguments = arguments
        .iter()
        .map(|argument| {
            argument
                .to_str()
                .ok_or(SystemLinkerInvocationBuildError::NonUnicodeArgument)
        })
        .collect::<Result<Vec<_>, _>>()?;

    match encoding {
        ResponseFileEncoding::Utf8 => encode_utf8(&arguments),
        ResponseFileEncoding::Utf16LittleEndian => encode_utf16_little_endian(&arguments),
    }
}

fn encode_utf8(arguments: &[&str]) -> Result<Vec<u8>, SystemLinkerInvocationBuildError> {
    let mut contents = Vec::new();

    for argument in arguments {
        let quoted = quote_gnu(argument)?;

        contents.extend_from_slice(quoted.as_bytes());
        contents.push(b'\n');
    }

    Ok(contents)
}

fn quote_gnu(argument: &str) -> Result<String, SystemLinkerInvocationBuildError> {
    if argument.contains(['\0', '\n', '\r']) {
        return Err(SystemLinkerInvocationBuildError::UnsupportedArgument);
    }

    let mut quoted = String::with_capacity(argument.len() + 2);

    quoted.push('"');

    for character in argument.chars() {
        if matches!(character, '\\' | '"') {
            quoted.push('\\');
        }

        quoted.push(character);
    }

    quoted.push('"');

    Ok(quoted)
}

fn encode_utf16_little_endian(
    arguments: &[&str],
) -> Result<Vec<u8>, SystemLinkerInvocationBuildError> {
    let mut text = String::new();

    for argument in arguments {
        if argument.contains(['\0', '\n', '\r']) {
            return Err(SystemLinkerInvocationBuildError::UnsupportedArgument);
        }

        text.push_str(&quote_microsoft(argument));
        text.push_str("\r\n");
    }

    let mut contents = vec![0xff, 0xfe];

    for unit in text.encode_utf16() {
        contents.extend_from_slice(&unit.to_le_bytes());
    }

    Ok(contents)
}

fn quote_microsoft(argument: &str) -> String {
    let mut quoted = String::with_capacity(argument.len() + 2);
    let mut backslashes = 0;

    quoted.push('"');

    for character in argument.chars() {
        match character {
            '\\' => backslashes += 1,
            '"' => {
                quoted.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                quoted.push('"');
                backslashes = 0;
            }
            _ => {
                quoted.extend(std::iter::repeat_n('\\', backslashes));
                quoted.push(character);
                backslashes = 0;
            }
        }
    }

    quoted.extend(std::iter::repeat_n('\\', backslashes * 2));
    quoted.push('"');

    quoted
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

    use super::{encode_utf8, encode_utf16_little_endian, invocation, quote_microsoft};
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
    fn gnu_response_files_quote_whitespace_quotes_and_backslashes() {
        assert_eq!(
            encode_utf8(&["plain", "space name.o", "quote\"name", r"path\name"])
                .unwrap_or_else(|error| panic!("test arguments must encode: {error:?}")),
            b"\"plain\"\n\"space name.o\"\n\"quote\\\"name\"\n\"path\\\\name\"\n"
        );
    }

    #[test]
    fn microsoft_response_files_use_utf16_little_endian_with_a_bom() {
        let encoded = encode_utf16_little_endian(&["plain", r#"space \"name\""#])
            .unwrap_or_else(|error| panic!("test arguments must encode: {error:?}"));

        assert_eq!(&encoded[..2], &[0xff, 0xfe]);

        let decoded = String::from_utf16(
            &encoded[2..]
                .chunks_exact(2)
                .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
                .collect::<Vec<_>>(),
        )
        .unwrap_or_else(|error| panic!("test response must decode: {error:?}"));

        assert_eq!(
            decoded,
            format!(
                "{}\r\n{}\r\n",
                quote_microsoft("plain"),
                quote_microsoft(r#"space \"name\""#)
            )
        );
    }

    #[test]
    fn response_encoders_reject_line_breaks() {
        assert!(encode_utf8(&["first\nsecond"]).is_err());
        assert!(encode_utf16_little_endian(&["first\rsecond"]).is_err());
    }

    #[test]
    fn response_file_references_are_single_native_arguments() {
        let path = OsStr::new("stage/output with spaces.bray-link.rsp");
        let reference = super::response_file_reference(std::path::Path::new(path));

        assert_eq!(
            reference,
            OsString::from("@stage/output with spaces.bray-link.rsp")
        );
    }
}
