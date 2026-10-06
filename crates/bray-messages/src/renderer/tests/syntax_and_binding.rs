use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticId, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
    DiagnosticModuleTrust, DiagnosticNameKind, DiagnosticNote, DiagnosticNoteKind,
    DiagnosticRelatedLocation, DiagnosticRelatedLocationKind, DiagnosticVisibility, SeverityKind,
};
use bray_source::{SourceId, SourceSpan, TextRange, TextSize};
use bray_syntax::SyntaxKind;

use crate::RenderedDiagnosticNoteKind;
use crate::renderer::DiagnosticRenderer;

#[test]
fn generic_application_errors_identify_the_source_and_required_correction() {
    let span = SourceSpan::new(
        SourceId::new(0),
        TextRange::new(TextSize::new(10), TextSize::new(16)),
    );

    for (kind, args, note, message, help) in [
        (
            DiagnosticKind::BindingGenericArgumentCountMismatch,
            vec![
                DiagnosticArg::token_text("Result"),
                DiagnosticArg::expected_count(2),
                DiagnosticArg::actual_count(0),
            ],
            DiagnosticNoteKind::GenericArgumentCountMustMatch,
            "generic argument count for 'Result' does not match its declaration: expected 2, received 0",
            "supply one generic argument for each declared generic parameter",
        ),
        (
            DiagnosticKind::BindingGenericArgumentMustBeType,
            vec![DiagnosticArg::token_text("1")],
            DiagnosticNoteKind::GenericArgumentRequiresType,
            "generic argument '1' must be a type",
            "replace this argument with a type, such as bool or a declared type name",
        ),
        (
            DiagnosticKind::BindingGenericApplicationRequiresName,
            vec![DiagnosticArg::token_text("(bool)")],
            DiagnosticNoteKind::GenericApplicationRequiresDeclaredName,
            "generic arguments cannot be applied to '(bool)' because it is not a declaration name",
            "apply generic arguments directly to a generic type or trait name",
        ),
    ] {
        let mut diagnostic = Diagnostic::new(DiagnosticId::new(0), kind, SeverityKind::Error)
            .with_primary_span(span)
            .with_label(DiagnosticLabel::primary(
                DiagnosticLabelKind::GenericApplication,
                span,
            ))
            .with_note(DiagnosticNote::new(note));

        for arg in args {
            diagnostic = diagnostic.with_arg(arg);
        }

        let rendered = DiagnosticRenderer::english().render(&diagnostic);

        assert_eq!(rendered.message(), message);
        assert_eq!(rendered.primary_span(), Some(span));
        assert_eq!(rendered.notes().len(), 1);

        assert_eq!(
            rendered.notes()[0].rendered_kind(),
            RenderedDiagnosticNoteKind::Help
        );

        assert_eq!(rendered.notes()[0].message(), help);
        assert_eq!(rendered.labels().len(), 1);
        assert_eq!(rendered.labels()[0].message(), "generic application");
        assert!(rendered.suggestions().is_empty());
    }
}

#[test]
fn renderer_renders_syntax_diagnostics_from_catalog() {
    let span = SourceSpan::empty(SourceId::new(0), TextSize::new(5));
    let expected = DiagnosticArg::expected_syntax_kind(SyntaxKind::FuncKeyword);

    let diagnostic = Diagnostic::new(
        DiagnosticId::new(5),
        DiagnosticKind::SyntaxExpectedToken,
        SeverityKind::Error,
    )
    .with_primary_span(span)
    .with_arg(expected.clone())
    .with_arg(DiagnosticArg::actual_syntax_kind(
        SyntaxKind::EndOfFileToken,
    ))
    .with_label(
        DiagnosticLabel::primary(DiagnosticLabelKind::ExpectedTokenInsertionPoint, span)
            .with_arg(expected),
    );

    let rendered = DiagnosticRenderer::english().render(&diagnostic);

    assert_eq!(
        rendered.message(),
        "expected func keyword but found end of file token"
    );

    let [label] = rendered.labels() else {
        panic!("expected one rendered label: {rendered:?}");
    };

    assert_eq!(label.message(), "insert func keyword here");
}

#[test]
fn renderer_renders_declaration_diagnostics_from_catalog() {
    let duplicate_span = SourceSpan::new(
        SourceId::new(0),
        TextRange::new(TextSize::new(6), TextSize::new(11)),
    );

    let duplicate = Diagnostic::new(
        DiagnosticId::new(6),
        DiagnosticKind::DeclarationDuplicateName,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::declaration_name("Point"))
    .with_label(DiagnosticLabel::primary(
        DiagnosticLabelKind::DuplicateDeclaration,
        duplicate_span,
    ))
    .with_related_location(DiagnosticRelatedLocation::new(
        DiagnosticRelatedLocationKind::FirstDeclaration,
        SourceSpan::new(
            SourceId::new(0),
            TextRange::new(TextSize::new(0), TextSize::new(5)),
        ),
    ));

    let visibility = Diagnostic::new(
        DiagnosticId::new(12),
        DiagnosticKind::DeclarationConflictingModuleVisibility,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::declaration_name("core"))
    .with_arg(DiagnosticArg::expected_visibility(
        DiagnosticVisibility::Public,
    ))
    .with_arg(DiagnosticArg::actual_visibility(
        DiagnosticVisibility::Internal,
    ));

    let trust = Diagnostic::new(
        DiagnosticId::new(13),
        DiagnosticKind::DeclarationConflictingModuleTrust,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::declaration_name("core"))
    .with_arg(DiagnosticArg::expected_module_trust(
        DiagnosticModuleTrust::Trusted,
    ))
    .with_arg(DiagnosticArg::actual_module_trust(
        DiagnosticModuleTrust::Ordinary,
    ));

    let modifier = Diagnostic::new(
        DiagnosticId::new(14),
        DiagnosticKind::DeclarationInvalidModifier,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::modifier_kind(SyntaxKind::PublicKeyword));

    let directives = Diagnostic::new(
        DiagnosticId::new(15),
        DiagnosticKind::DeclarationIncompatibleDirectives,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::directive_kind(
        SyntaxKind::EntrypointDirective,
    ))
    .with_arg(DiagnosticArg::conflicting_directive_kind(
        SyntaxKind::TestDirective,
    ));

    let renderer = DiagnosticRenderer::english();
    let duplicate = renderer.render(&duplicate);

    assert_eq!(duplicate.message(), "duplicate declaration of 'Point'");

    let [duplicate_label] = duplicate.labels() else {
        panic!("expected one duplicate declaration label: {duplicate:?}");
    };

    assert_eq!(duplicate_label.message(), "duplicate declaration");

    let [first_declaration] = duplicate.related_locations() else {
        panic!("expected the first declaration location: {duplicate:?}");
    };

    assert_eq!(first_declaration.message(), "first declared here");

    assert_eq!(
        renderer.render(&visibility).message(),
        "module 'core' has conflicting visibility: expected public, found internal"
    );

    assert_eq!(
        renderer.render(&trust).message(),
        "module 'core' has conflicting trust state: expected trusted, found non-trusted"
    );

    assert_eq!(
        renderer.render(&modifier).message(),
        "public modifier is not valid on this declaration"
    );

    assert_eq!(
        renderer.render(&directives).message(),
        "entrypoint and test directives cannot be combined"
    );
}

#[test]
fn renderer_renders_expected_expression_diagnostics_from_catalog() {
    let span = SourceSpan::new(
        SourceId::new(0),
        TextRange::new(TextSize::ZERO, TextSize::new(1)),
    );

    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::SyntaxExpectedExpression,
        SeverityKind::Error,
    )
    .with_primary_span(span)
    .with_arg(DiagnosticArg::expected_syntax_kind(SyntaxKind::Expression))
    .with_arg(DiagnosticArg::actual_syntax_kind(SyntaxKind::AtToken))
    .with_arg(DiagnosticArg::token_text("@"))
    .with_label(
        DiagnosticLabel::primary(DiagnosticLabelKind::ExpectedExpression, span)
            .with_arg(DiagnosticArg::expected_syntax_kind(SyntaxKind::Expression)),
    );

    let rendered = DiagnosticRenderer::english().render(&diagnostic);

    assert_eq!(rendered.message(), "expected expression");

    let [label] = rendered.labels() else {
        panic!("expected one rendered label: {rendered:?}");
    };

    assert_eq!(label.message(), "expected expression here");
}

#[test]
fn renderer_renders_syntax_nesting_limits_from_typed_counts() {
    let diagnostic = Diagnostic::new(
        DiagnosticId::new(1),
        DiagnosticKind::SyntaxNestingLimitExceeded,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::maximum_count(128));

    let rendered = DiagnosticRenderer::english().render(&diagnostic);

    assert_eq!(
        rendered.message(),
        "syntax nesting exceeds the maximum depth of 128"
    );
}

#[test]
fn renderer_renders_binding_diagnostics_from_structured_arguments() {
    let diagnostic = Diagnostic::new(
        DiagnosticId::new(14),
        DiagnosticKind::BindingWrongNameKind,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::referenced_name("Size"))
    .with_arg(DiagnosticArg::expected_name_kind(DiagnosticNameKind::Type));

    let rendered = DiagnosticRenderer::english().render(&diagnostic);

    assert_eq!(rendered.message(), "name 'Size' does not refer to a type");

    let shadowing = Diagnostic::new(
        DiagnosticId::new(15),
        DiagnosticKind::BindingNameAlreadyDefined,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::referenced_name("value"));

    let incoherent = Diagnostic::new(
        DiagnosticId::new(16),
        DiagnosticKind::BindingIncoherentAlternativePattern,
        SeverityKind::Error,
    );

    assert_eq!(
        DiagnosticRenderer::english().render(&shadowing).message(),
        "name is already defined: 'value'"
    );

    assert_eq!(
        DiagnosticRenderer::english().render(&incoherent).message(),
        "alternative patterns must bind the same names"
    );
}
