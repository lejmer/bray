use bray_diagnostics::{
    DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticIoErrorKind,
};
use bray_source::{SourceInputKind, SourceSpan};

use crate::locale::DiagnosticLocale;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ArgumentFormatter {
    locale: DiagnosticLocale,
}

impl ArgumentFormatter {
    pub(crate) const fn new(locale: DiagnosticLocale) -> Self {
        Self { locale }
    }

    pub(crate) fn format_named_arg(
        self,
        args: &[DiagnosticArg],
        name: DiagnosticArgName,
    ) -> String {
        match args.iter().find(|arg| arg.name() == name) {
            Some(arg) => self.format_value(arg.value()),
            None => format_missing_arg(name),
        }
    }

    fn format_value(self, value: &DiagnosticArgValue) -> String {
        match self.locale {
            DiagnosticLocale::English => format_english_value(value),
        }
    }
}

pub(crate) fn format_source_span(locale: DiagnosticLocale, span: SourceSpan) -> String {
    match locale {
        DiagnosticLocale::English => format_english_source_span(span),
    }
}

pub(crate) const fn arg_name_key(name: DiagnosticArgName) -> &'static str {
    name.as_str()
}

fn format_missing_arg(name: DiagnosticArgName) -> String {
    format!("{{{}}}", arg_name_key(name))
}

fn format_english_value(value: &DiagnosticArgValue) -> String {
    match value {
        DiagnosticArgValue::Byte(byte) => format!("0x{byte:02X}"),
        DiagnosticArgValue::ByteCount(byte_count) => byte_count.to_string(),
        DiagnosticArgValue::Character(character) => format_english_character(*character),
        DiagnosticArgValue::FilePath(path) => path.display().to_string(),
        DiagnosticArgValue::InputIndex(input_index) => input_index.to_string(),
        DiagnosticArgValue::IoErrorKind(kind) => format_english_io_error_kind(*kind).to_owned(),
        DiagnosticArgValue::SourceName(name) => name.clone(),
        DiagnosticArgValue::SourceCount(source_count) => source_count.to_string(),
        DiagnosticArgValue::SourceInputKind(kind) => {
            format_english_source_input_kind(*kind).to_owned()
        }
        DiagnosticArgValue::TextOffset(offset) => offset.bytes().to_string(),
        DiagnosticArgValue::Uri(uri) => uri.clone(),
        DiagnosticArgValue::SourceSpan(span) => {
            format_source_span(DiagnosticLocale::English, *span)
        }
        DiagnosticArgValue::WorkerCount(worker_count) => worker_count.to_string(),
    }
}

fn format_english_character(character: char) -> String {
    if character.is_control() {
        return format!("U+{:04X}", u32::from(character));
    }

    format!("'{}'", character.escape_default())
}

const fn format_english_io_error_kind(kind: DiagnosticIoErrorKind) -> &'static str {
    match kind {
        DiagnosticIoErrorKind::AlreadyExists => "already exists",
        DiagnosticIoErrorKind::IsDirectory => "is a directory",
        DiagnosticIoErrorKind::InvalidData => "invalid data",
        DiagnosticIoErrorKind::InvalidInput => "invalid input",
        DiagnosticIoErrorKind::Interrupted => "interrupted",
        DiagnosticIoErrorKind::NotDirectory => "not a directory",
        DiagnosticIoErrorKind::NotFound => "not found",
        DiagnosticIoErrorKind::Other => "other I/O error",
        DiagnosticIoErrorKind::PermissionDenied => "permission denied",
        DiagnosticIoErrorKind::TimedOut => "timed out",
        DiagnosticIoErrorKind::UnexpectedEof => "unexpected end of file",
        DiagnosticIoErrorKind::WouldBlock => "would block",
    }
}

const fn format_english_source_input_kind(kind: SourceInputKind) -> &'static str {
    match kind {
        SourceInputKind::File => "file",
        SourceInputKind::VirtualText => "virtual text",
        SourceInputKind::GeneratedText => "generated text",
        SourceInputKind::LspOpenDocument => "LSP open document",
    }
}

fn format_english_source_span(span: SourceSpan) -> String {
    format!(
        "source {}:{}..{}",
        span.source_id().raw(),
        span.start().bytes(),
        span.end().bytes()
    )
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticIoErrorKind,
    };
    use bray_source::{SourceId, SourceInputKind, SourceSpan, TextRange, TextSize};

    use super::ArgumentFormatter;
    use crate::DiagnosticLocale;

    #[test]
    fn argument_formatter_formats_representative_english_values() {
        let span = SourceSpan::new(
            SourceId::new(2),
            TextRange::new(TextSize::new(3), TextSize::new(8)),
        );

        let args = vec![
            DiagnosticArg::new(DiagnosticArgName::Byte, DiagnosticArgValue::Byte(0xff)),
            DiagnosticArg::new(
                DiagnosticArgName::Character,
                DiagnosticArgValue::Character('x'),
            ),
            DiagnosticArg::new(
                DiagnosticArgName::IoErrorKind,
                DiagnosticArgValue::IoErrorKind(DiagnosticIoErrorKind::PermissionDenied),
            ),
            DiagnosticArg::new(
                DiagnosticArgName::SourceInputKind,
                DiagnosticArgValue::SourceInputKind(SourceInputKind::GeneratedText),
            ),
            DiagnosticArg::new(
                DiagnosticArgName::SourceSpan,
                DiagnosticArgValue::SourceSpan(span),
            ),
            DiagnosticArg::new(
                DiagnosticArgName::WorkerCount,
                DiagnosticArgValue::WorkerCount(4),
            ),
        ];

        let formatter = ArgumentFormatter::new(DiagnosticLocale::English);

        assert_eq!(
            formatter.format_named_arg(&args, DiagnosticArgName::Byte),
            "0xFF"
        );
        assert_eq!(
            formatter.format_named_arg(&args, DiagnosticArgName::Character),
            "'x'"
        );
        assert_eq!(
            formatter.format_named_arg(&args, DiagnosticArgName::IoErrorKind),
            "permission denied"
        );
        assert_eq!(
            formatter.format_named_arg(&args, DiagnosticArgName::SourceInputKind),
            "generated text"
        );
        assert_eq!(
            formatter.format_named_arg(&args, DiagnosticArgName::SourceSpan),
            "source 2:3..8"
        );
        assert_eq!(
            formatter.format_named_arg(&args, DiagnosticArgName::WorkerCount),
            "4"
        );
    }
}
