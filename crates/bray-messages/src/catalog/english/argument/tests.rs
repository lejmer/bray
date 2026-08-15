use bray_diagnostics::{
    DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticCallableOverloadArm,
    DiagnosticCallableOverloadProblem, DiagnosticInterfaceDeclarationIdentity,
    DiagnosticInterfaceLimit, DiagnosticInterfaceSymbolIdentity, DiagnosticInterfaceSymbolKind,
    DiagnosticInterfaceSynthesizedIdentity, DiagnosticIoErrorKind, DiagnosticType,
};
use bray_source::{
    LineIndex, SourceId, SourceIdentity, SourceInputKind, SourceLocation, SourceOrigin,
    SourceSnapshot, SourceSpan, SourceVersion, TextRange, TextSize,
};
use bray_syntax::SyntaxKind;

use crate::DiagnosticLocale;
use crate::argument::ArgumentFormatter;

use super::format_value;

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
        DiagnosticArg::expected_syntax_kind(SyntaxKind::FuncKeyword),
        DiagnosticArg::trait_member_kind(SyntaxKind::EnterKeyword),
        DiagnosticArg::token_text("main\n"),
        DiagnosticArg::new(
            DiagnosticArgName::SourceSpan,
            DiagnosticArgValue::SourceSpan(span),
        ),
        DiagnosticArg::new(
            DiagnosticArgName::WorkerCount,
            DiagnosticArgValue::WorkerCount(4),
        ),
        DiagnosticArg::new(
            DiagnosticArgName::InterfaceLimit,
            DiagnosticArgValue::InterfaceLimit(DiagnosticInterfaceLimit::ImplementationEntryCount),
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
        formatter.format_named_arg(&args, DiagnosticArgName::ExpectedSyntaxKind),
        "func keyword"
    );

    assert_eq!(
        formatter.format_named_arg(&args, DiagnosticArgName::TraitMemberName),
        "'enter'"
    );

    assert_eq!(
        formatter.format_named_arg(&args, DiagnosticArgName::TokenText),
        r#"'main\n'"#
    );

    assert_eq!(
        formatter.format_named_arg(&args, DiagnosticArgName::SourceSpan),
        "source 2:3..8"
    );

    assert_eq!(
        formatter.format_named_arg(&args, DiagnosticArgName::WorkerCount),
        "4"
    );

    assert_eq!(
        formatter.format_named_arg(&args, DiagnosticArgName::InterfaceLimit),
        "implementation artifact entry count"
    );
}

#[test]
fn nested_interface_symbol_identities_render_as_one_semantic_path() {
    let identity = DiagnosticInterfaceSymbolIdentity::Synthesized {
        owner: Box::new(DiagnosticInterfaceSymbolIdentity::Declaration {
            owner: Box::new(DiagnosticInterfaceSymbolIdentity::Package(
                "example.math".to_owned(),
            )),
            kind: DiagnosticInterfaceSymbolKind::Function,
            identity: DiagnosticInterfaceDeclarationIdentity::Name("sum".to_owned()),
        }),
        identity: DiagnosticInterfaceSynthesizedIdentity::CallableParameter(2),
    };

    assert_eq!(
        format_value(
            DiagnosticArgName::InterfaceSymbolIdentity,
            &DiagnosticArgValue::InterfaceSymbolIdentity(identity),
        ),
        "'example.math::function sum::callable parameter 2'"
    );
}

#[test]
fn callable_overload_problems_render_exact_arm_signatures() {
    let arm = DiagnosticCallableOverloadArm::new(
        DiagnosticInterfaceSymbolIdentity::Package("example.first".to_owned()),
        false,
        vec![DiagnosticType::Boolean],
        DiagnosticType::Unit,
    );

    let conflicting = DiagnosticCallableOverloadArm::new(
        DiagnosticInterfaceSymbolIdentity::Package("example.second".to_owned()),
        false,
        vec![DiagnosticType::Boolean],
        DiagnosticType::Unit,
    );

    assert_eq!(
        format_value(
            DiagnosticArgName::CallableOverloadProblem,
            &DiagnosticArgValue::CallableOverloadProblem(
                DiagnosticCallableOverloadProblem::ConflictingSignatures { arm, conflicting },
            ),
        ),
        "'example.first' with signature (bool) -> unit has the same call surface as 'example.second' with signature (bool) -> unit"
    );
}

#[test]
fn source_locations_format_as_line_column_ranges() {
    let snapshot = match SourceSnapshot::new(
        SourceId::new(0),
        SourceIdentity::new(0),
        SourceOrigin::file("main.bray"),
        SourceVersion::new(0),
        "ok\n$",
    ) {
        Ok(snapshot) => snapshot,
        Err(error) => panic!("test source should fit in TextSize: {error:?}"),
    };

    let line_index = match LineIndex::new(snapshot.text()) {
        Ok(line_index) => line_index,
        Err(error) => panic!("test source should index: {error:?}"),
    };

    let span = SourceSpan::new(
        snapshot.source_id(),
        TextRange::new(TextSize::new(3), TextSize::new(4)),
    );

    let location = match SourceLocation::resolve(&snapshot, &line_index, span) {
        Some(location) => location,
        None => panic!("test span should resolve"),
    };

    assert_eq!(
        super::format_source_location(location),
        "main.bray:2:1..2:1"
    );
}
