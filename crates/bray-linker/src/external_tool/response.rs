use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy)]
pub(crate) enum ResponseFileEncoding {
    Utf8,
    Utf16LittleEndian,
}

pub(crate) fn encode_response_arguments(
    arguments: &[OsString],
    encoding: ResponseFileEncoding,
) -> Result<Vec<u8>, ResponseFileEncodingError> {
    let arguments = arguments
        .iter()
        .map(|argument| {
            argument
                .to_str()
                .ok_or(ResponseFileEncodingError::NonUnicodeArgument)
        })
        .collect::<Result<Vec<_>, _>>()?;

    match encoding {
        ResponseFileEncoding::Utf8 => encode_utf8(&arguments),
        ResponseFileEncoding::Utf16LittleEndian => encode_utf16_little_endian(&arguments),
    }
}

pub(crate) fn response_file_materialization_path(
    reference: &Path,
    current_directory: Option<&Path>,
) -> PathBuf {
    match current_directory {
        Some(directory) if reference.is_relative() => directory.join(reference),
        _ => reference.to_path_buf(),
    }
}

pub(crate) fn response_file_path(output: &Path, suffix: &str) -> PathBuf {
    let mut path = output.as_os_str().to_os_string();

    path.push(suffix);

    path.into()
}

pub(crate) fn response_file_reference(path: &Path) -> OsString {
    let mut argument = OsString::from("@");

    argument.push(path);

    argument
}

fn encode_utf8(arguments: &[&str]) -> Result<Vec<u8>, ResponseFileEncodingError> {
    let mut contents = Vec::new();

    for argument in arguments {
        let quoted = quote_gnu(argument)?;

        contents.extend_from_slice(quoted.as_bytes());
        contents.push(b'\n');
    }

    Ok(contents)
}

fn quote_gnu(argument: &str) -> Result<String, ResponseFileEncodingError> {
    validate_argument(argument)?;

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

fn encode_utf16_little_endian(arguments: &[&str]) -> Result<Vec<u8>, ResponseFileEncodingError> {
    let mut text = String::new();

    for argument in arguments {
        validate_argument(argument)?;

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

fn validate_argument(argument: &str) -> Result<(), ResponseFileEncodingError> {
    if argument.contains(['\0', '\n', '\r']) {
        return Err(ResponseFileEncodingError::UnsupportedArgument);
    }

    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ResponseFileEncodingError {
    NonUnicodeArgument,
    UnsupportedArgument,
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::{ResponseFileEncoding, encode_response_arguments, quote_microsoft};

    #[test]
    fn gnu_response_files_quote_whitespace_quotes_and_backslashes() {
        let arguments = [
            OsString::from("plain"),
            OsString::from("space name.o"),
            OsString::from("quote\"name"),
            OsString::from(r"path\name"),
        ];

        assert_eq!(
            encode_response_arguments(&arguments, ResponseFileEncoding::Utf8)
                .unwrap_or_else(|error| { panic!("test arguments must encode: {error:?}") }),
            b"\"plain\"\n\"space name.o\"\n\"quote\\\"name\"\n\"path\\\\name\"\n"
        );
    }

    #[test]
    fn microsoft_response_files_use_utf16_little_endian_with_a_bom() {
        let arguments = [OsString::from("plain"), OsString::from(r#"space \"name\""#)];

        let encoded =
            encode_response_arguments(&arguments, ResponseFileEncoding::Utf16LittleEndian)
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
        assert!(
            encode_response_arguments(
                &[OsString::from("first\nsecond")],
                ResponseFileEncoding::Utf8,
            )
            .is_err()
        );

        assert!(
            encode_response_arguments(
                &[OsString::from("first\rsecond")],
                ResponseFileEncoding::Utf16LittleEndian,
            )
            .is_err()
        );
    }
}
