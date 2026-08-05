use std::io::{Read, Write};
use std::sync::Arc;

use bray_source::{SourceId, SourceSpan, SourceVersion, TextRange, TextSize};

use crate::{
    AssertionFailure, CapturedStream, CapturedStreamPolicy, ExplicitTestFailure,
    TestCancellationSource, TestCaptureLimits, TestCapturePolicy, TestCatalogDigest,
    TestCatalogEntryId, TestDuration, TestErrorTypeIdentity, TestHostCommand, TestHostCommandId,
    TestHostControl, TestHostResult, TestInfrastructureFailure, TestInfrastructureFailureKind,
    TestOutcome, TestPanicCause, TestPanicReport, TestSourceAnchor, TestStreamFailure,
    TestStreamFailureKind, TestTimeoutPolicy,
};

use super::support::{
    Decoder, Encoder, TestProtocolError, decode_optional_error_type, encode_optional_error_type,
    read_frame, write_frame,
};

const COMMAND_MAGIC: &[u8; 8] = b"BRAYTSCM";
const CONTROL_MAGIC: &[u8; 8] = b"BRAYTSCT";
const RESULT_MAGIC: &[u8; 8] = b"BRAYTSRS";

/// Writes one bounded native host startup command.
pub fn write_host_command(
    writer: &mut impl Write,
    command: &TestHostCommand,
) -> Result<(), TestProtocolError> {
    let mut encoder = Encoder::new(COMMAND_MAGIC);

    encoder.bytes(&command.id().bytes())?;
    encoder.bytes(&command.catalog_digest().bytes())?;
    encoder.u32(command.entry().value());
    encode_timeout(&mut encoder, command.timeout());
    encode_capture_policy(&mut encoder, command.capture());

    encode_optional_error_type(&mut encoder, command.error_type())?;

    write_frame(writer, &encoder.finish()?)
}

/// Reads one bounded native host startup command.
pub fn read_host_command(reader: &mut impl Read) -> Result<TestHostCommand, TestProtocolError> {
    let payload = read_frame(reader)?;
    let mut decoder = Decoder::new(&payload, COMMAND_MAGIC)?;
    let id = TestHostCommandId::from_bytes(decode_digest(&mut decoder)?);
    let catalog_digest = TestCatalogDigest::from_bytes(decode_digest(&mut decoder)?);
    let entry = TestCatalogEntryId::new(decoder.u32()?);
    let timeout = decode_timeout(&mut decoder)?;
    let capture = decode_capture_policy(&mut decoder)?;

    let error_type = decode_optional_error_type(&mut decoder)?;

    decoder.finish()?;

    Ok(TestHostCommand::new(
        id,
        catalog_digest,
        entry,
        timeout,
        capture,
        error_type,
    ))
}

/// Writes one control message to an active native test host.
pub fn write_host_control(
    writer: &mut impl Write,
    control: TestHostControl,
) -> Result<(), TestProtocolError> {
    let mut encoder = Encoder::new(CONTROL_MAGIC);

    encoder.u8(match control {
        TestHostControl::Cancel => 0,
    });

    write_frame(writer, &encoder.finish()?)
}

/// Reads one control message for an active native test host.
pub fn read_host_control(reader: &mut impl Read) -> Result<TestHostControl, TestProtocolError> {
    let payload = read_frame(reader)?;
    let mut decoder = Decoder::new(&payload, CONTROL_MAGIC)?;

    let control = match decoder.u8()? {
        0 => TestHostControl::Cancel,
        _ => return Err(TestProtocolError::Malformed),
    };

    decoder.finish()?;

    Ok(control)
}

/// Writes one terminal native host result after cleanup.
pub fn write_host_result(
    writer: &mut impl Write,
    result: &TestHostResult,
) -> Result<(), TestProtocolError> {
    let mut encoder = Encoder::new(RESULT_MAGIC);

    encoder.bytes(&result.command_id().bytes())?;
    encoder.bytes(&result.catalog_digest().bytes())?;
    encode_outcome(&mut encoder, result.outcome())?;
    encode_stream(&mut encoder, result.standard_output())?;
    encode_stream(&mut encoder, result.standard_error())?;

    write_frame(writer, &encoder.finish()?)
}

/// Reads one terminal native host result after cleanup.
pub fn read_host_result(reader: &mut impl Read) -> Result<TestHostResult, TestProtocolError> {
    let payload = read_frame(reader)?;
    let mut decoder = Decoder::new(&payload, RESULT_MAGIC)?;
    let command_id = TestHostCommandId::from_bytes(decode_digest(&mut decoder)?);
    let catalog_digest = TestCatalogDigest::from_bytes(decode_digest(&mut decoder)?);
    let outcome = decode_outcome(&mut decoder)?;
    let standard_output = decode_stream(&mut decoder)?;
    let standard_error = decode_stream(&mut decoder)?;

    decoder.finish()?;

    Ok(TestHostResult::after_cleanup(
        command_id,
        catalog_digest,
        outcome,
        standard_output,
        standard_error,
    ))
}

fn decode_digest(decoder: &mut Decoder<'_>) -> Result<[u8; 32], TestProtocolError> {
    decoder
        .bytes()?
        .try_into()
        .map_err(|_| TestProtocolError::Malformed)
}

fn encode_timeout(encoder: &mut Encoder, timeout: TestTimeoutPolicy) {
    match timeout {
        TestTimeoutPolicy::Unlimited => encoder.u8(0),
        TestTimeoutPolicy::Limit(limit) => {
            encoder.u8(1);
            encoder.u64(limit.nanoseconds());
        }
    }
}

fn decode_timeout(decoder: &mut Decoder<'_>) -> Result<TestTimeoutPolicy, TestProtocolError> {
    match decoder.u8()? {
        0 => Ok(TestTimeoutPolicy::Unlimited),
        1 => Ok(TestTimeoutPolicy::Limit(TestDuration::from_nanoseconds(
            decoder.u64()?,
        ))),
        _ => Err(TestProtocolError::Malformed),
    }
}

fn encode_capture_policy(encoder: &mut Encoder, capture: TestCapturePolicy) {
    match capture {
        TestCapturePolicy::Captured(limits) => {
            encoder.u8(0);
            encoder.u64(limits.per_stream_byte_limit());
            encoder.u64(limits.invocation_byte_limit());
        }
        TestCapturePolicy::Inherited => encoder.u8(1),
        TestCapturePolicy::Discarded => encoder.u8(2),
    }
}

fn decode_capture_policy(
    decoder: &mut Decoder<'_>,
) -> Result<TestCapturePolicy, TestProtocolError> {
    match decoder.u8()? {
        0 => Ok(TestCapturePolicy::Captured(TestCaptureLimits::new(
            decoder.u64()?,
            decoder.u64()?,
        ))),
        1 => Ok(TestCapturePolicy::Inherited),
        2 => Ok(TestCapturePolicy::Discarded),
        _ => Err(TestProtocolError::Malformed),
    }
}

fn encode_outcome(encoder: &mut Encoder, outcome: &TestOutcome) -> Result<(), TestProtocolError> {
    match outcome {
        TestOutcome::Passed => encoder.u8(0),
        TestOutcome::ReturnedError {
            error_type,
            formatted_value,
        } => {
            encoder.u8(1);
            encoder.string(error_type.as_str())?;
            encode_optional_string(encoder, formatted_value.as_deref())?;
        }
        TestOutcome::ExplicitFailure(failure) => {
            encoder.u8(2);
            encode_source(encoder, failure.source());
            encoder.string(failure.message())?;
        }
        TestOutcome::AssertionFailure(failure) => {
            encoder.u8(3);
            encode_source(encoder, failure.source());
            encode_optional_string(encoder, failure.message())?;
        }
        TestOutcome::Panicked(report) => {
            encoder.u8(4);

            encoder.u8(match report.cause() {
                TestPanicCause::Message => 0,
                TestPanicCause::Assertion => 1,
                TestPanicCause::ExplicitFailure => 2,
            });

            match report.source() {
                Some(source) => {
                    encoder.u8(1);
                    encode_source(encoder, source);
                }
                None => encoder.u8(0),
            }

            encoder.string(report.message())?;
        }
        TestOutcome::TimedOut(limit) => {
            encoder.u8(5);
            encoder.u64(limit.nanoseconds());
        }
        TestOutcome::Cancelled(source) => {
            encoder.u8(6);

            encoder.u8(match source {
                TestCancellationSource::Command => 0,
                TestCancellationSource::Invocation => 1,
            });
        }
        TestOutcome::InfrastructureFailed(failure) => {
            encoder.u8(7);
            encode_infrastructure_failure(encoder, *failure);
        }
    }

    Ok(())
}

fn decode_outcome(decoder: &mut Decoder<'_>) -> Result<TestOutcome, TestProtocolError> {
    match decoder.u8()? {
        0 => Ok(TestOutcome::Passed),
        1 => {
            let error_type = TestErrorTypeIdentity::try_new(decoder.string()?)
                .ok_or(TestProtocolError::Malformed)?;

            let formatted_value = decode_optional_string(decoder)?.map(Arc::from);

            Ok(TestOutcome::ReturnedError {
                error_type,
                formatted_value,
            })
        }
        2 => Ok(TestOutcome::ExplicitFailure(ExplicitTestFailure::new(
            decode_source(decoder)?,
            decoder.string()?,
        ))),
        3 => {
            let source = decode_source(decoder)?;

            match decode_optional_string(decoder)? {
                Some(message) => Ok(TestOutcome::AssertionFailure(
                    AssertionFailure::with_message(source, message),
                )),
                None => Ok(TestOutcome::AssertionFailure(
                    AssertionFailure::without_message(source),
                )),
            }
        }
        4 => {
            let cause = match decoder.u8()? {
                0 => TestPanicCause::Message,
                1 => TestPanicCause::Assertion,
                2 => TestPanicCause::ExplicitFailure,
                _ => return Err(TestProtocolError::Malformed),
            };

            let source = match decoder.u8()? {
                0 => None,
                1 => Some(decode_source(decoder)?),
                _ => return Err(TestProtocolError::Malformed),
            };

            Ok(TestOutcome::Panicked(TestPanicReport::new(
                cause,
                source,
                decoder.string()?,
            )))
        }
        5 => Ok(TestOutcome::TimedOut(TestDuration::from_nanoseconds(
            decoder.u64()?,
        ))),
        6 => Ok(TestOutcome::Cancelled(match decoder.u8()? {
            0 => TestCancellationSource::Command,
            1 => TestCancellationSource::Invocation,
            _ => return Err(TestProtocolError::Malformed),
        })),
        7 => Ok(TestOutcome::InfrastructureFailed(
            decode_infrastructure_failure(decoder)?,
        )),
        _ => Err(TestProtocolError::Malformed),
    }
}

fn encode_stream(encoder: &mut Encoder, stream: &CapturedStream) -> Result<(), TestProtocolError> {
    encoder.u8(match stream.policy() {
        CapturedStreamPolicy::Captured => 0,
        CapturedStreamPolicy::Inherited => 1,
        CapturedStreamPolicy::Discarded => 2,
    });

    encoder.bytes(stream.bytes())?;
    encoder.u64(stream.discarded_byte_count());

    match stream.failure() {
        Some(failure) => {
            encoder.u8(1);

            encoder.u8(match failure.kind() {
                TestStreamFailureKind::ResourceExhausted => 0,
                TestStreamFailureKind::SinkUnavailable => 1,
                TestStreamFailureKind::Platform => 2,
            });

            match failure.platform_code() {
                Some(code) => {
                    encoder.u8(1);
                    encoder.u64(u64::from_le_bytes(code.to_le_bytes()));
                }
                None => encoder.u8(0),
            }
        }
        None => encoder.u8(0),
    }

    Ok(())
}

fn decode_stream(decoder: &mut Decoder<'_>) -> Result<CapturedStream, TestProtocolError> {
    let policy = decoder.u8()?;
    let bytes = decoder.bytes()?.to_vec();
    let discarded = decoder.u64()?;

    let failure = match decoder.u8()? {
        0 => None,
        1 => {
            let kind = match decoder.u8()? {
                0 => TestStreamFailureKind::ResourceExhausted,
                1 => TestStreamFailureKind::SinkUnavailable,
                2 => TestStreamFailureKind::Platform,
                _ => return Err(TestProtocolError::Malformed),
            };

            let platform_code = match decoder.u8()? {
                0 => None,
                1 => Some(i64::from_le_bytes(decoder.u64()?.to_le_bytes())),
                _ => return Err(TestProtocolError::Malformed),
            };

            Some(TestStreamFailure::new(kind, platform_code))
        }
        _ => return Err(TestProtocolError::Malformed),
    };

    match policy {
        0 => Ok(CapturedStream::captured(bytes, discarded, failure)),
        1 if bytes.is_empty() && discarded == 0 => Ok(CapturedStream::inherited(failure)),
        2 if bytes.is_empty() && discarded == 0 && failure.is_none() => {
            Ok(CapturedStream::discarded())
        }
        _ => Err(TestProtocolError::Malformed),
    }
}

fn encode_source(encoder: &mut Encoder, source: TestSourceAnchor) {
    let span = source.span();

    encoder.u32(span.source_id().raw());
    encoder.u32(span.start().bytes());
    encoder.u32(span.end().bytes());
    encoder.u64(source.version().raw());
}

fn decode_source(decoder: &mut Decoder<'_>) -> Result<TestSourceAnchor, TestProtocolError> {
    let source = SourceId::stored(decoder.u32()?).ok_or(TestProtocolError::Malformed)?;
    let start = TextSize::new(decoder.u32()?);
    let end = TextSize::new(decoder.u32()?);

    if start > end {
        return Err(TestProtocolError::Malformed);
    }

    Ok(TestSourceAnchor::new(
        SourceSpan::new(source, TextRange::new(start, end)),
        SourceVersion::new(decoder.u64()?),
    ))
}

fn encode_optional_string(
    encoder: &mut Encoder,
    value: Option<&str>,
) -> Result<(), TestProtocolError> {
    match value {
        Some(value) => {
            encoder.u8(1);
            encoder.string(value)?;
        }
        None => encoder.u8(0),
    }

    Ok(())
}

fn decode_optional_string<'bytes>(
    decoder: &mut Decoder<'bytes>,
) -> Result<Option<&'bytes str>, TestProtocolError> {
    match decoder.u8()? {
        0 => Ok(None),
        1 => decoder.string().map(Some),
        _ => Err(TestProtocolError::Malformed),
    }
}

fn encode_infrastructure_failure(encoder: &mut Encoder, failure: TestInfrastructureFailure) {
    encoder.u8(match failure.kind() {
        TestInfrastructureFailureKind::Protocol => 0,
        TestInfrastructureFailureKind::Host => 1,
        TestInfrastructureFailureKind::Capture => 2,
        TestInfrastructureFailureKind::ResourceExhausted => 3,
        TestInfrastructureFailureKind::MissingOutcome => 4,
        TestInfrastructureFailureKind::Cleanup => 5,
        TestInfrastructureFailureKind::ForcedTermination => 6,
    });

    match failure.detail_code() {
        Some(code) => {
            encoder.u8(1);
            encoder.u64(code);
        }
        None => encoder.u8(0),
    }
}

fn decode_infrastructure_failure(
    decoder: &mut Decoder<'_>,
) -> Result<TestInfrastructureFailure, TestProtocolError> {
    let kind = match decoder.u8()? {
        0 => TestInfrastructureFailureKind::Protocol,
        1 => TestInfrastructureFailureKind::Host,
        2 => TestInfrastructureFailureKind::Capture,
        3 => TestInfrastructureFailureKind::ResourceExhausted,
        4 => TestInfrastructureFailureKind::MissingOutcome,
        5 => TestInfrastructureFailureKind::Cleanup,
        6 => TestInfrastructureFailureKind::ForcedTermination,
        _ => return Err(TestProtocolError::Malformed),
    };

    let detail = match decoder.u8()? {
        0 => None,
        1 => Some(decoder.u64()?),
        _ => return Err(TestProtocolError::Malformed),
    };

    Ok(TestInfrastructureFailure::new(kind, detail))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::{
        CapturedStream, ExplicitTestFailure, TestCaptureLimits, TestCapturePolicy,
        TestCatalogDigest, TestCatalogEntryId, TestDuration, TestErrorTypeIdentity,
        TestHostCommand, TestHostCommandId, TestHostControl, TestHostResult, TestOutcome,
        TestSourceAnchor, TestTimeoutPolicy,
    };
    use bray_source::{SourceId, SourceSpan, SourceVersion, TextRange, TextSize};

    #[test]
    fn host_commands_and_results_round_trip_through_bounded_frames() {
        let command = TestHostCommand::new(
            TestHostCommandId::from_bytes([3; 32]),
            TestCatalogDigest::from_bytes([4; 32]),
            TestCatalogEntryId::new(7),
            TestTimeoutPolicy::Limit(TestDuration::from_nanoseconds(50)),
            TestCapturePolicy::Captured(TestCaptureLimits::new(1024, 1536)),
            TestErrorTypeIdentity::try_new("example.tests.Error"),
        );

        let mut command_bytes = Vec::new();

        super::write_host_command(&mut command_bytes, &command)
            .unwrap_or_else(|error| panic!("test command must encode: {error:?}"));

        let decoded_command = super::read_host_command(&mut Cursor::new(command_bytes))
            .unwrap_or_else(|error| panic!("test command must decode: {error:?}"));

        assert_eq!(decoded_command, command);

        let source = TestSourceAnchor::new(
            SourceSpan::new(
                SourceId::new(3),
                TextRange::new(TextSize::new(4), TextSize::new(9)),
            ),
            SourceVersion::new(2),
        );

        let result = TestHostResult::after_cleanup(
            command.id(),
            command.catalog_digest(),
            TestOutcome::ExplicitFailure(ExplicitTestFailure::new(source, "failed")),
            CapturedStream::captured(b"out".iter().copied(), 2, None),
            CapturedStream::discarded(),
        );

        let mut result_bytes = Vec::new();

        super::write_host_result(&mut result_bytes, &result)
            .unwrap_or_else(|error| panic!("test result must encode: {error:?}"));

        let decoded_result = super::read_host_result(&mut Cursor::new(result_bytes))
            .unwrap_or_else(|error| panic!("test result must decode: {error:?}"));

        assert_eq!(decoded_result, result);
    }

    #[test]
    fn host_control_messages_round_trip() {
        let mut bytes = Vec::new();

        super::write_host_control(&mut bytes, TestHostControl::Cancel)
            .unwrap_or_else(|error| panic!("test control must encode: {error:?}"));

        let decoded = super::read_host_control(&mut Cursor::new(bytes))
            .unwrap_or_else(|error| panic!("test control must decode: {error:?}"));

        assert_eq!(decoded, TestHostControl::Cancel);
    }

    #[test]
    fn captured_streams_accept_the_default_one_mebibyte_limit() {
        let command_id = TestHostCommandId::from_bytes([8; 32]);
        let catalog_digest = TestCatalogDigest::from_bytes([9; 32]);
        let bytes = vec![b'x'; 1024 * 1024];

        let result = TestHostResult::after_cleanup(
            command_id,
            catalog_digest,
            TestOutcome::Passed,
            CapturedStream::captured(bytes, 0, None),
            CapturedStream::discarded(),
        );

        let mut encoded = Vec::new();

        super::write_host_result(&mut encoded, &result)
            .unwrap_or_else(|error| panic!("bounded capture must encode: {error:?}"));

        let decoded = super::read_host_result(&mut Cursor::new(encoded))
            .unwrap_or_else(|error| panic!("bounded capture must decode: {error:?}"));

        assert_eq!(decoded, result);
    }
}
