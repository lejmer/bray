use std::io::{self, BufRead, BufReader, Read, Write};

use bray_messages::{LanguageServerMessage, LanguageServerMessageRenderer};
use serde_json::{Value, json};

pub(crate) const REQUEST_CANCELLED: i32 = -32800;
pub(crate) const CONTENT_MODIFIED: i32 = -32801;
pub(crate) const INVALID_PARAMS: i32 = -32602;
pub(crate) const INTERNAL_ERROR: i32 = -32603;
pub(crate) const METHOD_NOT_FOUND: i32 = -32601;

#[derive(Debug)]
pub(crate) enum IncomingMessage {
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    Notification {
        method: String,
        params: Value,
    },
    Response,
}

pub(crate) fn read_messages(
    input: &mut dyn Read,
    mut on_message: impl FnMut(IncomingMessage) -> bool,
) -> io::Result<()> {
    let mut reader = BufReader::new(input);

    while let Some(value) = read_message(&mut reader)? {
        let message = classify(value);

        if !on_message(message) {
            break;
        }
    }

    Ok(())
}

pub(crate) fn write_result(output: &mut dyn Write, id: Value, result: Value) -> io::Result<()> {
    write_value(
        output,
        &json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        }),
    )
}

pub(crate) fn write_error(
    output: &mut dyn Write,
    id: Value,
    code: i32,
    message: LanguageServerMessage,
    renderer: LanguageServerMessageRenderer,
) -> io::Result<()> {
    let message = renderer.render(message);

    write_value(
        output,
        &json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": code,
                "message": message,
            },
        }),
    )
}

pub(crate) fn write_error_with_data(
    output: &mut dyn Write,
    id: Value,
    code: i32,
    message: LanguageServerMessage,
    data: Value,
    renderer: LanguageServerMessageRenderer,
) -> io::Result<()> {
    let message = renderer.render(message);

    write_value(
        output,
        &json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": code,
                "message": message,
                "data": data,
            },
        }),
    )
}

pub(crate) fn write_notification(
    output: &mut dyn Write,
    method: &'static str,
    params: Value,
) -> io::Result<()> {
    write_value(
        output,
        &json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }),
    )
}

fn read_message(reader: &mut impl BufRead) -> io::Result<Option<Value>> {
    let mut content_length = None;
    let mut saw_header = false;

    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line)?;

        if bytes == 0 {
            return if saw_header {
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "language_server_incomplete_header",
                ))
            } else {
                Ok(None)
            };
        }

        saw_header = true;

        if line == "\r\n" || line == "\n" {
            break;
        }

        let Some((name, value)) = line.trim_end().split_once(':') else {
            continue;
        };

        if name.eq_ignore_ascii_case("content-length") {
            content_length = value.trim().parse::<usize>().ok();
        }
    }

    let Some(content_length) = content_length else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "language_server_missing_content_length",
        ));
    };

    let mut content = Vec::new();

    content
        .try_reserve_exact(content_length)
        .map_err(|cause| io::Error::new(io::ErrorKind::OutOfMemory, cause))?;

    content.resize(content_length, 0);

    reader.read_exact(&mut content)?;

    serde_json::from_slice(&content)
        .map(Some)
        .map_err(|cause| io::Error::new(io::ErrorKind::InvalidData, cause))
}

fn write_value(output: &mut dyn Write, value: &Value) -> io::Result<()> {
    let content = serde_json::to_vec(value).map_err(io::Error::other)?;

    write!(output, "Content-Length: {}\r\n\r\n", content.len())?;
    output.write_all(&content)?;

    output.flush()
}

fn classify(value: Value) -> IncomingMessage {
    let Some(object) = value.as_object() else {
        return IncomingMessage::Response;
    };

    let Some(method) = object.get("method").and_then(Value::as_str) else {
        return IncomingMessage::Response;
    };

    let params = object.get("params").cloned().unwrap_or(Value::Null);

    match object.get("id") {
        Some(id) => IncomingMessage::Request {
            id: id.clone(),
            method: method.to_owned(),
            params,
        },
        None => IncomingMessage::Notification {
            method: method.to_owned(),
            params,
        },
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use serde_json::json;

    use bray_messages::{LanguageServerMessage, LanguageServerMessageRenderer};

    use super::{
        INVALID_PARAMS, IncomingMessage, read_messages, write_error, write_error_with_data,
        write_result,
    };

    #[test]
    fn reads_consecutive_framed_messages() {
        let first = br#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
        let second = br#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#;
        let mut bytes = Vec::new();

        bytes.extend_from_slice(format!("Content-Length: {}\r\n\r\n", first.len()).as_bytes());
        bytes.extend_from_slice(first);
        bytes.extend_from_slice(format!("Content-Length: {}\r\n\r\n", second.len()).as_bytes());
        bytes.extend_from_slice(second);

        let mut methods = Vec::new();

        read_messages(&mut Cursor::new(bytes), |message| {
            match message {
                IncomingMessage::Request { method, .. }
                | IncomingMessage::Notification { method, .. } => methods.push(method),
                IncomingMessage::Response => {}
            }

            true
        })
        .unwrap_or_else(|error| panic!("framed messages should parse: {error}"));

        assert_eq!(methods, ["initialize", "initialized"]);
    }

    #[test]
    fn writes_framed_json_rpc_results() {
        let mut output = Vec::new();

        write_result(&mut output, json!(7), json!({"capabilities": {}}))
            .unwrap_or_else(|error| panic!("result should write: {error}"));

        let output = String::from_utf8(output)
            .unwrap_or_else(|error| panic!("protocol output should be UTF-8: {error}"));

        assert!(output.starts_with("Content-Length: "));
        assert!(output.contains(r#""id":7"#));
        assert!(output.contains(r#""capabilities":{}"#));
    }

    #[test]
    fn protocol_errors_render_localized_structured_messages() {
        let mut output = Vec::new();

        write_error(
            &mut output,
            json!(7),
            INVALID_PARAMS,
            LanguageServerMessage::InvalidParams,
            LanguageServerMessageRenderer::english(),
        )
        .unwrap_or_else(|error| panic!("protocol error should write: {error}"));

        let output = String::from_utf8(output)
            .unwrap_or_else(|error| panic!("protocol output should be UTF-8: {error}"));

        assert!(output.contains(r#""message":"Invalid request parameters.""#));
        assert!(!output.contains("language_server_invalid_params"));
    }

    #[test]
    fn protocol_errors_retain_exact_machine_data() {
        let mut output = Vec::new();

        write_error_with_data(
            &mut output,
            json!(7),
            INVALID_PARAMS,
            LanguageServerMessage::InvalidParams,
            json!({
                "reason": "invalid_params",
                "cause": "missing field `textDocument`",
            }),
            LanguageServerMessageRenderer::english(),
        )
        .unwrap_or_else(|error| panic!("protocol error should write: {error}"));

        let output = String::from_utf8(output)
            .unwrap_or_else(|error| panic!("protocol output should be UTF-8: {error}"));

        assert!(output.contains(r#""reason":"invalid_params""#));
        assert!(output.contains(r#""cause":"missing field `textDocument`""#));
    }
}
