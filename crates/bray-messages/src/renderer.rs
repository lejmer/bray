use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticArgName, DiagnosticBag, DiagnosticKind, DiagnosticLabel,
    DiagnosticLabelStyle, DiagnosticNote, DiagnosticNoteKind, DiagnosticRelatedLocation,
    DiagnosticRelatedLocationKind, DiagnosticSuggestion, DiagnosticSuggestionKind, SeverityKind,
};
use bray_source::{SourceLocation, SourceSpan};

use crate::argument::{ArgumentFormatter, format_source_location, format_source_span};
use crate::catalog::{MessageCatalog, MessageTemplate, MessageTemplatePart};
use crate::locale::DiagnosticLocale;
use crate::rendered_diagnostic::{
    RenderedDiagnostic, RenderedDiagnosticLabel, RenderedDiagnosticNote,
    RenderedDiagnosticNoteKind, RenderedDiagnosticRelatedLocation, RenderedDiagnosticSuggestion,
};

/// Locale-aware renderer for structured diagnostics.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DiagnosticRenderer {
    locale: DiagnosticLocale,
}

impl DiagnosticRenderer {
    /// Creates a diagnostic renderer for `locale`.
    pub const fn new(locale: DiagnosticLocale) -> Self {
        Self { locale }
    }

    /// Creates a diagnostic renderer for English output.
    pub const fn english() -> Self {
        Self::new(DiagnosticLocale::English)
    }

    /// Returns the renderer locale.
    pub const fn locale(self) -> DiagnosticLocale {
        self.locale
    }

    /// Renders one structured diagnostic.
    pub fn render(self, diagnostic: &Diagnostic) -> RenderedDiagnostic {
        let catalog = MessageCatalog::new(self.locale);
        let template = catalog.diagnostic_template(diagnostic.kind());

        let labels = diagnostic
            .labels()
            .iter()
            .map(|label| self.render_label(label))
            .collect();

        let notes = diagnostic
            .notes()
            .iter()
            .map(|note| self.render_note(note))
            .collect();

        let related_locations = diagnostic
            .related_locations()
            .iter()
            .map(|location| self.render_related_location(location))
            .collect();

        let suggestions = diagnostic
            .suggestions()
            .iter()
            .map(|suggestion| self.render_suggestion(suggestion))
            .collect();

        RenderedDiagnostic::new(
            diagnostic.id(),
            diagnostic.kind(),
            diagnostic.severity(),
            diagnostic.primary_span(),
            self.render_template(template, diagnostic.args()),
            labels,
            notes,
            related_locations,
            suggestions,
        )
    }

    /// Renders all diagnostics in insertion order.
    pub fn render_bag(self, bag: &DiagnosticBag) -> Vec<RenderedDiagnostic> {
        bag.iter()
            .map(|diagnostic| self.render(diagnostic))
            .collect()
    }

    /// Renders a source span for terminal diagnostic output.
    pub fn render_source_span(self, span: SourceSpan) -> String {
        format_source_span(self.locale, span)
    }

    /// Renders a resolved source location for terminal diagnostic output.
    pub fn render_source_location(self, location: SourceLocation<'_>) -> String {
        format_source_location(self.locale, location)
    }

    /// Renders a severity heading for terminal diagnostic output.
    pub fn render_severity(self, severity: SeverityKind) -> &'static str {
        MessageCatalog::new(self.locale).severity_label(severity)
    }

    /// Renders a label style for terminal diagnostic output.
    pub fn render_label_style(self, style: DiagnosticLabelStyle) -> &'static str {
        MessageCatalog::new(self.locale).label_style(style)
    }

    /// Renders a note heading for terminal diagnostic output.
    pub fn render_note_heading(self, kind: RenderedDiagnosticNoteKind) -> &'static str {
        MessageCatalog::new(self.locale).note_heading(kind)
    }

    /// Renders the heading for an ordinary source label.
    pub fn render_label_heading(self) -> &'static str {
        MessageCatalog::new(self.locale).label_heading()
    }

    /// Renders the heading for a supporting source location.
    pub fn render_related_location_heading(self) -> &'static str {
        MessageCatalog::new(self.locale).related_location_heading()
    }

    /// Returns typed arguments referenced by a diagnostic's primary message template.
    pub fn diagnostic_argument_names(self, kind: DiagnosticKind) -> Vec<DiagnosticArgName> {
        template_argument_names(MessageCatalog::new(self.locale).diagnostic_template(kind))
    }

    /// Returns whether the primary template contains recovery or next-action prose.
    ///
    /// This English-catalog guardrail complements semantic review. A primary diagnostic message
    /// identifies what failed and why. Recovery belongs in structured notes or suggestions.
    pub fn primary_message_contains_recovery_instruction(self, kind: DiagnosticKind) -> bool {
        MessageCatalog::new(self.locale)
            .diagnostic_template(kind)
            .contains_recovery_instruction()
    }

    /// Returns typed arguments referenced by one note template.
    pub fn note_argument_names(self, kind: DiagnosticNoteKind) -> Vec<DiagnosticArgName> {
        template_argument_names(MessageCatalog::new(self.locale).note_template(kind))
    }

    /// Returns typed arguments referenced by one related-location template.
    pub fn related_location_argument_names(
        self,
        kind: DiagnosticRelatedLocationKind,
    ) -> Vec<DiagnosticArgName> {
        template_argument_names(MessageCatalog::new(self.locale).related_location_template(kind))
    }

    /// Returns typed arguments referenced by one suggestion template.
    pub fn suggestion_argument_names(
        self,
        kind: DiagnosticSuggestionKind,
    ) -> Vec<DiagnosticArgName> {
        template_argument_names(MessageCatalog::new(self.locale).suggestion_template(kind))
    }

    /// Returns every typed argument name consumed by an actually present rendered component.
    pub fn component_argument_names(self, diagnostic: &Diagnostic) -> Vec<DiagnosticArgName> {
        let catalog = MessageCatalog::new(self.locale);
        let mut names = template_argument_names(catalog.diagnostic_template(diagnostic.kind()));

        for label in diagnostic.labels() {
            names.extend(template_argument_names(
                catalog.label_template(label.kind()),
            ));
        }

        for note in diagnostic.notes() {
            names.extend(template_argument_names(catalog.note_template(note.kind())));
        }

        for location in diagnostic.related_locations() {
            names.extend(template_argument_names(
                catalog.related_location_template(location.kind()),
            ));
        }

        for suggestion in diagnostic.suggestions() {
            names.extend(template_argument_names(
                catalog.suggestion_template(suggestion.kind()),
            ));
        }

        names.sort_unstable();
        names.dedup();

        names
    }

    fn render_label(self, label: &DiagnosticLabel) -> RenderedDiagnosticLabel {
        let template = MessageCatalog::new(self.locale).label_template(label.kind());

        RenderedDiagnosticLabel::new(
            label.kind(),
            label.style(),
            label.span(),
            self.render_template(template, label.args()),
        )
    }

    fn render_note(self, note: &DiagnosticNote) -> RenderedDiagnosticNote {
        let catalog = MessageCatalog::new(self.locale);
        let template = catalog.note_template(note.kind());

        RenderedDiagnosticNote::new(
            note.kind(),
            catalog.note_kind(note.kind()),
            self.render_template(template, note.args()),
        )
    }

    fn render_related_location(
        self,
        location: &DiagnosticRelatedLocation,
    ) -> RenderedDiagnosticRelatedLocation {
        let template = MessageCatalog::new(self.locale).related_location_template(location.kind());

        RenderedDiagnosticRelatedLocation::new(
            location.kind(),
            location.span(),
            self.render_template(template, location.args()),
        )
    }

    fn render_suggestion(self, suggestion: &DiagnosticSuggestion) -> RenderedDiagnosticSuggestion {
        let template = MessageCatalog::new(self.locale).suggestion_template(suggestion.kind());

        RenderedDiagnosticSuggestion::new(
            suggestion.kind(),
            suggestion.applicability(),
            suggestion.edits().to_vec(),
            self.render_template(template, suggestion.args()),
        )
    }

    fn render_template(self, template: MessageTemplate, args: &[DiagnosticArg]) -> String {
        let formatter = ArgumentFormatter::new(self.locale);

        let mut message = String::new();

        for part in template.parts() {
            match part {
                MessageTemplatePart::Text(text) => message.push_str(text),
                MessageTemplatePart::Arg(name) => {
                    message.push_str(&formatter.format_named_arg(args, *name));
                }
            }
        }

        message
    }
}

fn template_argument_names(template: MessageTemplate) -> Vec<DiagnosticArgName> {
    template
        .parts()
        .iter()
        .filter_map(|part| match part {
            MessageTemplatePart::Text(_) => None,
            MessageTemplatePart::Arg(name) => Some(*name),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        Diagnostic, DiagnosticAlignmentKind, DiagnosticArg, DiagnosticArgName, DiagnosticArgValue,
        DiagnosticArrayGeneratorCardinalityProblem, DiagnosticArrayLength,
        DiagnosticArtifactDigest, DiagnosticArtifactDigestAlgorithm, DiagnosticArtifactKind,
        DiagnosticBag, DiagnosticCallbackStateProblem, DiagnosticCheckerFailure,
        DiagnosticCheckerNode, DiagnosticCheckerSymbol, DiagnosticCodegenVerificationStage,
        DiagnosticConstructionInputRejection, DiagnosticDependencyRequirementKind,
        DiagnosticDependencySubjectKind, DiagnosticEmissionEvaluationFailure,
        DiagnosticEmissionFailure, DiagnosticExpressionCategory, DiagnosticExternalToolExit,
        DiagnosticId, DiagnosticIoErrorKind, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
        DiagnosticLabelStyle, DiagnosticLayoutOption, DiagnosticLayoutProblem,
        DiagnosticMemoryOperation, DiagnosticModuleTrust, DiagnosticNameKind, DiagnosticNamedType,
        DiagnosticNote, DiagnosticNoteKind, DiagnosticOutputSink, DiagnosticPatternCoverage,
        DiagnosticPatternMissingCase, DiagnosticProjectManifestField, DiagnosticPropagationProblem,
        DiagnosticRefinementCapacity, DiagnosticRefinementCapacitySurface,
        DiagnosticRejectedSelectionCandidate, DiagnosticRelatedLocation,
        DiagnosticRelatedLocationKind, DiagnosticRuntimeAbiVersion,
        DiagnosticRuntimeArtifactProblem, DiagnosticSelectionCandidate,
        DiagnosticSelectionCandidateIdentity, DiagnosticSelectionCandidateSignature,
        DiagnosticSelectionCandidates, DiagnosticSelectionKind, DiagnosticSelectionRejectionReason,
        DiagnosticSelectionRejections, DiagnosticSourceInput, DiagnosticSourceInputOrigin,
        DiagnosticStorageAccess, DiagnosticStorageAccessPurpose, DiagnosticStorageFlowFailure,
        DiagnosticStorageProjection, DiagnosticStorageRoot, DiagnosticSuggestion,
        DiagnosticSuggestionKind, DiagnosticTargetPredicateValueKind, DiagnosticType,
        DiagnosticTypeArgument, DiagnosticUnionTagProblem, DiagnosticVisibility,
        DiagnosticYieldCardinality, SeverityKind,
    };
    use bray_source::{SourceId, SourceSpan, TextRange, TextSize};
    use bray_syntax::SyntaxKind;

    use super::DiagnosticRenderer;
    use crate::catalog::{
        INTERNAL_COMPILER_ERROR, forbidden_internal_term, forbidden_ordinary_diagnostic_term,
    };
    use crate::{
        DiagnosticLocale, RenderedDiagnostic, RenderedDiagnosticLabel, RenderedDiagnosticNoteKind,
    };

    #[test]
    fn primary_templates_render_every_required_contract_argument() {
        let renderer = DiagnosticRenderer::english();

        for &kind in DiagnosticKind::ALL {
            let required = kind.quality_contract().primary_message_args();
            let rendered = renderer.diagnostic_argument_names(kind);

            assert!(
                required.iter().all(|name| rendered.contains(name)),
                "{kind:?} primary template omits required context: required {required:?}, rendered {rendered:?}",
            );
        }
    }

    #[test]
    fn renderer_renders_every_diagnostic_kind() {
        let renderer = DiagnosticRenderer::english();

        for (index, &kind) in DiagnosticKind::ALL.iter().enumerate() {
            let id = u32::try_from(index)
                .map(DiagnosticId::new)
                .unwrap_or_else(|_| panic!("diagnostic inventory must fit diagnostic IDs"));

            let span = SourceSpan::new(
                SourceId::new(1),
                TextRange::new(TextSize::new(2), TextSize::new(3)),
            );

            let diagnostic = Diagnostic::new(id, kind, SeverityKind::Error)
                .with_label(DiagnosticLabel::primary(
                    DiagnosticLabelKind::InvalidCharacter,
                    span,
                ))
                .with_note(DiagnosticNote::new(
                    DiagnosticNoteKind::SourceFileMustBeReadable,
                ))
                .with_related_location(DiagnosticRelatedLocation::new(
                    DiagnosticRelatedLocationKind::FirstDeclaration,
                    span,
                ))
                .with_suggestion(DiagnosticSuggestion::manual(
                    DiagnosticSuggestionKind::FormatSource,
                ));

            let rendered = renderer.render(&diagnostic);

            assert_eq!(rendered.kind(), kind);
            assert!(!rendered.message().is_empty(), "{kind:?}");

            for (component, message) in std::iter::once(("primary", rendered.message()))
                .chain(
                    rendered
                        .labels()
                        .iter()
                        .map(|label| ("label", label.message())),
                )
                .chain(rendered.notes().iter().map(|note| ("note", note.message())))
                .chain(
                    rendered
                        .related_locations()
                        .iter()
                        .map(|location| ("related location", location.message())),
                )
                .chain(
                    rendered
                        .suggestions()
                        .iter()
                        .map(|suggestion| ("suggestion", suggestion.message())),
                )
            {
                assert!(
                    !message.contains([';', '—']),
                    "{kind:?} {component} uses prohibited prose punctuation: {message:?}"
                );

                let forbidden = if kind.as_str().starts_with("inspection_") {
                    forbidden_internal_term(message)
                } else {
                    forbidden_ordinary_diagnostic_term(message)
                };

                assert_eq!(
                    forbidden, None,
                    "{kind:?} {component} exposes compiler implementation language: {message:?}"
                );
            }
        }
    }

    #[test]
    fn primary_messages_never_contain_recovery_instructions() {
        let renderer = DiagnosticRenderer::english();

        for &kind in DiagnosticKind::ALL {
            assert!(
                !renderer.primary_message_contains_recovery_instruction(kind),
                "{kind:?} must put recovery guidance in a typed note or suggestion"
            );
        }
    }

    #[test]
    fn renderer_renders_diagnostic_messages_from_structured_catalog() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(7),
            DiagnosticKind::SourceFileReadFailed,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::source_input(DiagnosticSourceInput::new(
            0,
            bray_source::SourceInputKind::File,
            DiagnosticSourceInputOrigin::File("main.bray".into()),
        )))
        .with_arg(DiagnosticArg::io_error_kind(
            DiagnosticIoErrorKind::NotFound,
        ))
        .with_note(DiagnosticNote::new(
            DiagnosticNoteKind::SourceFileMustBeReadable,
        ));

        let rendered = DiagnosticRenderer::english().render(&diagnostic);

        assert_eq!(rendered.id(), DiagnosticId::new(7));
        assert_eq!(rendered.kind(), DiagnosticKind::SourceFileReadFailed);
        assert_eq!(rendered.severity(), SeverityKind::Error);

        assert_eq!(
            rendered.message(),
            "could not read file source input 0 at main.bray: not found"
        );

        let [note] = rendered.notes() else {
            panic!("expected one rendered note: {rendered:?}");
        };

        assert_eq!(note.kind(), DiagnosticNoteKind::SourceFileMustBeReadable);
        assert_eq!(note.rendered_kind(), RenderedDiagnosticNoteKind::Help);

        assert_eq!(
            note.message(),
            "source files must be readable before compilation"
        );
    }

    #[test]
    fn emission_checker_failures_name_the_product_type_and_reporting_action() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(8),
            DiagnosticKind::EmissionFailed,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::actual_product_identity(
            "example.application",
        ))
        .with_arg(DiagnosticArg::target_triple("x86_64-unknown-linux-gnu"))
        .with_arg(DiagnosticArg::emission_failure(
            DiagnosticEmissionFailure::Evaluation(DiagnosticEmissionEvaluationFailure::Checker(
                DiagnosticCheckerFailure::CompilerKnownRepresentationUnavailable("ScalarU32"),
            )),
        ))
        .with_note(DiagnosticNote::new(
            DiagnosticNoteKind::ReportCompilerDefect,
        ));

        let rendered = DiagnosticRenderer::english().render(&diagnostic);

        assert!(rendered.message().contains("example.application"));
        assert!(rendered.message().contains("x86_64-unknown-linux-gnu"));
        assert!(rendered.message().contains("`u32`"));
        assert!(!rendered.message().contains("ScalarU32"));
        assert_eq!(rendered.notes().len(), 1);

        assert!(
            rendered.notes()[0]
                .message()
                .contains("report this compiler defect")
        );

        assert_eq!(forbidden_ordinary_diagnostic_term(rendered.message()), None);
    }

    #[test]
    fn backend_module_rejections_preserve_the_exact_report_and_stage() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(9),
            DiagnosticKind::CodegenBackendRejectedModule,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::target_triple("x86_64-unknown-linux-gnu"))
        .with_arg(DiagnosticArg::codegen_backend_identity("llvm"))
        .with_arg(DiagnosticArg::codegen_backend_report(
            "value representation mismatch",
        ))
        .with_arg(DiagnosticArg::codegen_verification_stage(
            DiagnosticCodegenVerificationStage::BeforeOptimization,
        ))
        .with_note(DiagnosticNote::new(
            DiagnosticNoteKind::ReportCompilerDefect,
        ));

        let rendered = DiagnosticRenderer::english().render(&diagnostic);

        assert!(rendered.message().starts_with(INTERNAL_COMPILER_ERROR));
        assert!(rendered.message().contains("x86_64-unknown-linux-gnu"));
        assert!(rendered.message().contains("value representation mismatch"));
        assert!(rendered.message().contains("before optimization"));
        assert_eq!(rendered.notes().len(), 1);
    }

    #[test]
    fn backend_support_program_failures_render_captured_output_as_text() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(10),
            DiagnosticKind::CodegenBackendToolExited,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::target_triple("x86_64-pc-windows-msvc"))
        .with_arg(DiagnosticArg::codegen_backend_identity("llvm"))
        .with_arg(DiagnosticArg::file_path("C:/toolchain/opt.exe"))
        .with_arg(DiagnosticArg::external_tool_exit(
            DiagnosticExternalToolExit::new(Some(1), &[], b"permission denied\n"),
        ));

        let rendered = DiagnosticRenderer::english().render(&diagnostic);

        assert!(rendered.message().contains("C:/toolchain/opt.exe"));
        assert!(rendered.message().contains("exit code 1"));
        assert!(rendered.message().contains("permission denied"));
        assert!(!rendered.message().contains("[112, 101, 114"));
    }

    #[test]
    fn checker_compiler_defects_hide_internal_identities_and_retain_source_context() {
        use DiagnosticCheckerFailure as CheckerFailure;
        use DiagnosticStorageFlowFailure as StorageFlowFailure;

        let expression = DiagnosticCheckerNode::new("expression", 17, 23);
        let block = DiagnosticCheckerNode::new("block", 19, 29);
        let pattern = DiagnosticCheckerNode::new("pattern", 31, 37);
        let callable = DiagnosticCheckerSymbol::new("callable_overload", 41);

        let cases = [
            CheckerFailure::StorageFlow(StorageFlowFailure::IncompatibleInput {
                input: "expression_types",
                expected_unit: 1,
                expected_kind: "callable_body",
                actual_unit: 2,
                actual_kind: "constant_template",
            }),
            CheckerFailure::StorageFlow(StorageFlowFailure::IncompatibleInput {
                input: "future_input",
                expected_unit: 1,
                expected_kind: "future_expected_kind",
                actual_unit: 2,
                actual_kind: "future_actual_kind",
            }),
            CheckerFailure::StorageFlow(StorageFlowFailure::FlowConstruction(
                "duplicate_suspension",
            )),
            CheckerFailure::StorageFlow(StorageFlowFailure::FlowConstruction("future_reason")),
            CheckerFailure::StorageFlow(StorageFlowFailure::ForeignDependencyContract),
            CheckerFailure::StorageFlow(StorageFlowFailure::DependencyContractsConstruction(
                "invalid_borrow",
            )),
            CheckerFailure::StorageFlow(StorageFlowFailure::DependencyContractsConstruction(
                "future_reason",
            )),
            CheckerFailure::StorageFlow(StorageFlowFailure::AsyncConstruction("foreign_unit")),
            CheckerFailure::StorageFlow(StorageFlowFailure::AsyncConstruction("future_reason")),
            CheckerFailure::StorageFlow(StorageFlowFailure::MissingAwaitDependencyContract {
                expression,
            }),
            CheckerFailure::StorageFlow(StorageFlowFailure::MissingDependencyContract {
                expression,
                contract_unit: 43,
                contract: 47,
            }),
            CheckerFailure::StorageFlow(StorageFlowFailure::CallableParameterCountMismatch {
                callable,
                signature_parameters: 3,
                type_parameters: 2,
            }),
            CheckerFailure::StorageFlow(StorageFlowFailure::CallableTypeNotCallable { callable }),
            CheckerFailure::StorageFlow(StorageFlowFailure::MissingBorrowCapability {
                unit: 53,
                borrow: 59,
            }),
            CheckerFailure::StorageFlow(StorageFlowFailure::MissingExitOrigin { exit: expression }),
            CheckerFailure::StorageFlow(StorageFlowFailure::MissingBlock { block }),
            CheckerFailure::StorageFlow(StorageFlowFailure::MissingStorageAccess {
                unit: 61,
                access: 67,
            }),
            CheckerFailure::StorageFlow(StorageFlowFailure::MissingStorageIdentity {
                unit: 71,
                identity: 73,
            }),
            CheckerFailure::StorageFlow(StorageFlowFailure::MissingStorageSymbolName {
                symbol: callable,
            }),
            CheckerFailure::StorageFlow(StorageFlowFailure::UnbalancedScopes {
                open_scope: Some(block),
            }),
            CheckerFailure::StorageFlow(StorageFlowFailure::UnbalancedScopes { open_scope: None }),
            CheckerFailure::StorageFlow(StorageFlowFailure::MissingPattern { pattern }),
            CheckerFailure::InvalidStorageOperation {
                expression,
                access: 79,
                status: "conflicting_borrow",
            },
            CheckerFailure::InvalidStorageOperation {
                expression,
                access: 83,
                status: "future_status",
            },
        ];

        let span = SourceSpan::new(
            SourceId::new(5),
            TextRange::new(TextSize::new(8), TextSize::new(13)),
        );

        for (index, failure) in cases.into_iter().enumerate() {
            let id = u32::try_from(index)
                .map(DiagnosticId::new)
                .unwrap_or_else(|_| panic!("checker failure inventory must fit diagnostic IDs"));

            let diagnostic = Diagnostic::new(
                id,
                DiagnosticKind::CheckingCompilerDefect,
                SeverityKind::Error,
            )
            .with_primary_span(span)
            .with_arg(DiagnosticArg::emission_failure(
                DiagnosticEmissionFailure::Evaluation(
                    DiagnosticEmissionEvaluationFailure::Checker(failure),
                ),
            ))
            .with_label(DiagnosticLabel::primary(
                DiagnosticLabelKind::CompilerDefectSource,
                span,
            ))
            .with_note(DiagnosticNote::new(
                DiagnosticNoteKind::ReportCompilerDefect,
            ));

            let rendered = DiagnosticRenderer::english().render(&diagnostic);

            assert_eq!(rendered.primary_span(), Some(span));

            assert!(rendered.message().starts_with(INTERNAL_COMPILER_ERROR));

            let [label] = rendered.labels() else {
                panic!("expected one compiler-defect source label: {rendered:?}");
            };

            assert_eq!(label.span(), span);

            assert!(!label.message().is_empty());

            let [note] = rendered.notes() else {
                panic!("expected one compiler-defect reporting note: {rendered:?}");
            };

            assert_eq!(note.kind(), DiagnosticNoteKind::ReportCompilerDefect);
            assert_eq!(note.rendered_kind(), RenderedDiagnosticNoteKind::Note);

            assert!(!note.message().is_empty());

            assert_eq!(forbidden_internal_term(rendered.message()), None);
            assert!(!rendered.message().contains('#'));
        }
    }

    #[test]
    fn renderer_localizes_checker_diagnostics() {
        let incompatible = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::CheckingIncompatibleExpressionType,
            SeverityKind::Error,
        )
        .with_arg(bray_diagnostics::DiagnosticArg::expected_type(
            bray_diagnostics::DiagnosticType::Boolean,
        ))
        .with_arg(bray_diagnostics::DiagnosticArg::actual_type(
            bray_diagnostics::DiagnosticType::Tuple(2),
        ));

        let unresolved = Diagnostic::new(
            DiagnosticId::new(1),
            DiagnosticKind::CheckingCannotInferExpressionType,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::expression_category(
            DiagnosticExpressionCategory::NameReference,
        ));

        let ambiguous = Diagnostic::new(
            DiagnosticId::new(4),
            DiagnosticKind::CheckingAmbiguousCandidate,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::selection_kind(
            DiagnosticSelectionKind::Operator,
        ))
        .with_arg(DiagnosticArg::selection_candidates(
            DiagnosticSelectionCandidates::new([
                DiagnosticSelectionCandidate::new(
                    DiagnosticSelectionCandidateIdentity::BuiltIn,
                    DiagnosticSelectionCandidateSignature::Operation {
                        operand_types: Box::new([DiagnosticType::I32, DiagnosticType::I32]),
                        result_type: Some(DiagnosticType::I32),
                    },
                ),
                DiagnosticSelectionCandidate::new(
                    DiagnosticSelectionCandidateIdentity::ExpressionValue,
                    DiagnosticSelectionCandidateSignature::Callable {
                        parameter_types: Box::new([DiagnosticType::I32, DiagnosticType::I32]),
                        result_type: DiagnosticType::I32,
                    },
                ),
            ]),
        ));

        let incompatible_candidate = Diagnostic::new(
            DiagnosticId::new(10),
            DiagnosticKind::CheckingIncompatibleCandidate,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::selection_kind(
            DiagnosticSelectionKind::Construction,
        ))
        .with_arg(DiagnosticArg::selection_rejections(
            DiagnosticSelectionRejections::try_from_prefix(
                [DiagnosticRejectedSelectionCandidate::new(
                    DiagnosticSelectionCandidate::new(
                        DiagnosticSelectionCandidateIdentity::BuiltIn,
                        DiagnosticSelectionCandidateSignature::Operation {
                            operand_types: Box::new([DiagnosticType::I32]),
                            result_type: Some(DiagnosticType::I32),
                        },
                    ),
                    DiagnosticSelectionRejectionReason::ConstructionInput(
                        DiagnosticConstructionInputRejection::UnknownName {
                            provided: String::from("y"),
                            accepted: Box::new([String::from("x")]),
                        },
                    ),
                )],
                1,
            )
            .unwrap_or_else(|_| panic!("one rejection must fit the diagnostic bound")),
        ));

        let target_alignment = Diagnostic::new(
            DiagnosticId::new(5),
            DiagnosticKind::CheckingTargetAlignmentUnsupported,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::alignment_kind(
            DiagnosticAlignmentKind::Storage,
        ))
        .with_arg(DiagnosticArg::required_alignment(64))
        .with_arg(DiagnosticArg::maximum_alignment(16));

        let target_alignment =
            target_alignment.with_arg(DiagnosticArg::target_triple("x86_64-unknown-linux-gnu"));

        let incompatible_pattern = Diagnostic::new(
            DiagnosticId::new(6),
            DiagnosticKind::CheckingIncompatiblePattern,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::actual_type(
            bray_diagnostics::DiagnosticType::Boolean,
        ));

        let non_exhaustive_match = Diagnostic::new(
            DiagnosticId::new(7),
            DiagnosticKind::CheckingNonExhaustiveMatch,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::pattern_coverage(
            DiagnosticPatternCoverage::new(
                DiagnosticType::Boolean,
                [DiagnosticPatternMissingCase::Boolean(false)],
                0,
            ),
        ));

        let named_type_mismatch = Diagnostic::new(
            DiagnosticId::new(8),
            DiagnosticKind::CheckingIncompatibleExpressionType,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::expected_type(DiagnosticType::Named(
            DiagnosticNamedType::new(
                [String::from("collection"), String::from("MoveCursor")],
                [DiagnosticTypeArgument::Type(DiagnosticType::I32)],
            ),
        )))
        .with_arg(DiagnosticArg::actual_type(DiagnosticType::Named(
            DiagnosticNamedType::new(
                [String::from("collection"), String::from("ReadCursor")],
                [DiagnosticTypeArgument::Type(DiagnosticType::I32)],
            ),
        )));

        let unavailable_await_dependency = Diagnostic::new(
            DiagnosticId::new(9),
            DiagnosticKind::CheckingUnavailableAwaitDependency,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::dependency_subject_kind(
            DiagnosticDependencySubjectKind::Storage,
        ))
        .with_arg(DiagnosticArg::dependency_requirement_kind(
            DiagnosticDependencyRequirementKind::StorageAlive,
        ));

        let renderer = DiagnosticRenderer::english();

        assert_eq!(
            renderer.render(&incompatible).message(),
            "expected bool, but found tuple type with 2 elements"
        );

        assert_eq!(
            renderer.render(&named_type_mismatch).message(),
            "expected collection.MoveCursor<i32>, but found collection.ReadCursor<i32>"
        );

        assert_eq!(
            renderer.render(&unavailable_await_dependency).message(),
            "await requires storage to remain alive"
        );

        assert_eq!(
            renderer.render(&unresolved).message(),
            "cannot infer the type of this name reference"
        );

        assert_eq!(
            renderer.render(&ambiguous).message(),
            concat!(
                "operator selection is ambiguous between built-in operation with signature ",
                "(i32, i32) -> i32 and callable expression value with signature ",
                "(i32, i32) -> i32"
            )
        );

        assert_eq!(
            renderer.render(&incompatible_candidate).message(),
            concat!(
                "construction operation candidates reject the supplied expressions: ",
                "built-in operation with signature (i32) -> i32: input name y is not accepted. ",
                "Candidate input names are x"
            )
        );

        assert_eq!(
            renderer.render(&target_alignment).message(),
            "required storage alignment 64 exceeds target 'x86_64-unknown-linux-gnu' maximum of 16"
        );

        assert_eq!(
            renderer.render(&incompatible_pattern).message(),
            "pattern is incompatible with bool"
        );

        assert_eq!(
            renderer.render(&non_exhaustive_match).message(),
            "match coverage is incomplete: bool is missing false"
        );
    }

    #[test]
    fn renderer_localizes_type_representation_causes() {
        let invalid_layout = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::CheckingInvalidLayoutDirective,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::layout_problem(
            DiagnosticLayoutProblem::OptionNotPowerOfTwo {
                option: DiagnosticLayoutOption::Alignment,
                value: 6,
            },
        ));

        let invalid_tag = Diagnostic::new(
            DiagnosticId::new(1),
            DiagnosticKind::CheckingInvalidUnionTag,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::union_tag_problem(
            DiagnosticUnionTagProblem::ValueOutsideSelectedType {
                signed: false,
                width_bits: 8,
                value_negative: false,
                value_bits: 9,
            },
        ));

        let renderer = DiagnosticRenderer::english();

        assert_eq!(
            renderer.render(&invalid_layout).message(),
            "the alignment value 6 is not a power of two"
        );

        assert_eq!(
            renderer.render(&invalid_tag).message(),
            "the nonnegative variant tag requires 9 magnitude bits, outside the selected unsigned 8-bit range 0 through 255"
        );
    }

    #[test]
    fn renderer_localizes_propagation_and_array_cardinality_causes() {
        let result = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::CheckingNoCompatiblePropagationBoundary,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::propagation_problem(
            DiagnosticPropagationProblem::ResultBoundaryUnavailable {
                source_error: DiagnosticType::I32,
                available_errors: vec![DiagnosticType::I64].into_boxed_slice(),
            },
        ));

        let nullable = Diagnostic::new(
            DiagnosticId::new(1),
            DiagnosticKind::CheckingNoCompatiblePropagationBoundary,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::propagation_problem(
            DiagnosticPropagationProblem::NullableBoundaryUnavailable {
                operand: DiagnosticType::Nullable,
                available_boundaries: vec![DiagnosticType::I32].into_boxed_slice(),
            },
        ));

        let unavailable_count = Diagnostic::new(
            DiagnosticId::new(2),
            DiagnosticKind::CheckingArrayGeneratorCardinalityNotProvable,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::array_generator_cardinality_problem(
            DiagnosticArrayGeneratorCardinalityProblem::SourceCountUnavailable {
                source: DiagnosticType::Named(DiagnosticNamedType::new(
                    [String::from("app"), String::from("Items")],
                    [],
                )),
                element: DiagnosticType::Boolean,
                required: DiagnosticArrayLength::Exact(2),
            },
        ));

        let divergent_yield = Diagnostic::new(
            DiagnosticId::new(3),
            DiagnosticKind::CheckingArrayGeneratorCardinalityNotProvable,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::array_generator_cardinality_problem(
            DiagnosticArrayGeneratorCardinalityProblem::YieldCountNotExact {
                element: DiagnosticType::Boolean,
                source_length: DiagnosticArrayLength::Symbolic,
                required: DiagnosticArrayLength::Exact(2),
                actual: DiagnosticYieldCardinality::Multiple,
            },
        ));

        let renderer = DiagnosticRenderer::english();

        assert_eq!(
            renderer.render(&result).message(),
            "cannot propagate error type i32 because no enclosing result boundary accepts it. Available boundaries accept i64"
        );

        assert_eq!(
            renderer.render(&nullable).message(),
            "cannot propagate nullable type because no enclosing boundary returns a nullable type. Available boundaries return i32"
        );

        assert_eq!(
            renderer.render(&unavailable_count).message(),
            "fixed-array generator of bool from app.Items requires 2 elements, but the source iteration count is not statically known"
        );

        assert_eq!(
            renderer.render(&divergent_yield).message(),
            "fixed-array generator of bool over a symbolic number of elements can yield multiple values per iteration but requires a result of 2 elements"
        );
    }

    #[test]
    fn renderer_localizes_exact_refinement_capacity() {
        let capacity = DiagnosticRefinementCapacity::try_new(
            DiagnosticRefinementCapacitySurface::PublishedRefinements,
            16_777_217,
            16_777_216,
        )
        .unwrap_or_else(|| panic!("limit plus one must be a capacity violation"));

        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::CheckingRefinementCapacityExceeded,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::refinement_capacity(capacity));

        assert_eq!(
            DiagnosticRenderer::english().render(&diagnostic).message(),
            "flow-sensitive analysis requires 16777217 published refinements, exceeding the configured maximum of 16777216"
        );
    }

    #[test]
    fn renderer_localizes_memory_operation_and_callback_causes() {
        let unavailable = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::CheckingTargetMemoryOperationUnavailable,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::target_triple("wasm32-unknown-unknown"))
        .with_arg(DiagnosticArg::memory_operation(
            DiagnosticMemoryOperation::PointerRead,
        ));

        let invalid = Diagnostic::new(
            DiagnosticId::new(1),
            DiagnosticKind::CheckingInvalidTargetControlContract,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::target_triple("x86_64-unknown-linux-gnu"))
        .with_arg(DiagnosticArg::memory_operation(
            DiagnosticMemoryOperation::InlineAssembly,
        ));

        let callback = Diagnostic::new(
            DiagnosticId::new(2),
            DiagnosticKind::CheckingInvalidCallbackStateContext,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::callback_state_problem(
            DiagnosticCallbackStateProblem::ContextParameterNotFirst { actual_ordinal: 2 },
        ))
        .with_note(DiagnosticNote::new(
            DiagnosticNoteKind::CallbackStateRequirements,
        ));

        let renderer = DiagnosticRenderer::english();

        assert_eq!(
            renderer.render(&unavailable).message(),
            "target 'wasm32-unknown-unknown' does not provide the required pointer read"
        );

        assert_eq!(
            renderer.render(&invalid).message(),
            "the inline assembly contract is invalid for target 'x86_64-unknown-linux-gnu'"
        );

        let callback = renderer.render(&callback);

        assert_eq!(
            callback.message(),
            "callback state context references parameter 3, not the first parameter"
        );

        assert_eq!(
            callback.notes()[0].message(),
            "use the first context parameter of a trusted C or system ABI callable with a native symbol directive"
        );
    }

    #[test]
    fn renderer_localizes_exact_storage_access_path() {
        let access = DiagnosticStorageAccess::new(
            DiagnosticStorageAccessPurpose::MutableBorrow,
            DiagnosticStorageRoot::Parameter,
            [
                DiagnosticStorageProjection::ProductField(String::from("payload")),
                DiagnosticStorageProjection::TupleElement(1),
            ],
            DiagnosticType::I32,
        );

        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::CheckingMissingMutationAuthority,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::storage_access(access));

        assert_eq!(
            DiagnosticRenderer::english().render(&diagnostic).message(),
            "mutable borrow of parameter storage.payload.1 with type i32 has no mutable access to the reached storage"
        );
    }

    #[test]
    fn renderer_localizes_structured_emission_diagnostics() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::EmissionArtifactWriteFailed,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::artifact_kind(
            DiagnosticArtifactKind::PackageInterface,
        ))
        .with_arg(DiagnosticArg::artifact_ordinal(0))
        .with_arg(DiagnosticArg::output_sink(DiagnosticOutputSink::Memory(
            "host.output".to_owned(),
        )))
        .with_arg(DiagnosticArg::io_error_kind(DiagnosticIoErrorKind::Other));

        let rendered = DiagnosticRenderer::english().render(&diagnostic);

        assert_eq!(
            rendered.message(),
            "could not write package interface artifact 0 output memory collector 'host.output': other I/O error"
        );
    }

    #[test]
    fn renderer_localizes_artifact_digest_mismatch_context() {
        let expected =
            DiagnosticArtifactDigest::new(DiagnosticArtifactDigestAlgorithm::Sha256, [0_u8; 32]);

        let actual =
            DiagnosticArtifactDigest::new(DiagnosticArtifactDigestAlgorithm::Sha256, [1_u8; 32]);

        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::EmissionArtifactDigestMismatch,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::artifact_kind(
            DiagnosticArtifactKind::PackageInterface,
        ))
        .with_arg(DiagnosticArg::artifact_ordinal(0))
        .with_arg(DiagnosticArg::expected_artifact_digest(expected))
        .with_arg(DiagnosticArg::actual_artifact_digest(actual));

        let rendered = DiagnosticRenderer::english().render(&diagnostic);

        let expected = format!(
            "package interface artifact 0 declared digest SHA-256 {}, but content digest was SHA-256 {}",
            "00".repeat(32),
            "01".repeat(32)
        );

        assert_eq!(rendered.message(), expected);
    }

    #[test]
    fn renderer_localizes_standard_library_artifact_and_abi_failures() {
        let length = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::StandardLibraryArtifactLengthMismatch,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::file_path("targets/test/1.0/libstd.a"))
        .with_arg(DiagnosticArg::actual_byte_count(7))
        .with_arg(DiagnosticArg::expected_byte_count(9));

        let abi = Diagnostic::new(
            DiagnosticId::new(1),
            DiagnosticKind::StandardLibraryRuntimeAbiMismatch,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::target_triple("test-target"))
        .with_arg(DiagnosticArg::expected_runtime_abi(
            DiagnosticRuntimeAbiVersion::new(2, 1),
        ))
        .with_arg(DiagnosticArg::actual_runtime_abi(
            DiagnosticRuntimeAbiVersion::new(1, 4),
        ));

        let renderer = DiagnosticRenderer::english();

        assert_eq!(
            renderer.render(&length).message(),
            "standard library artifact targets/test/1.0/libstd.a has 7 bytes but expected 9"
        );

        assert_eq!(
            renderer.render(&abi).message(),
            "target 'test-target' requires runtime ABI 2.1 but the standard library provides 1.4"
        );
    }

    #[test]
    fn renderer_explains_valid_project_initialization_identities() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::ProjectInitializationIdentityInvalid,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::referenced_name("Invalid Package"))
        .with_note(DiagnosticNote::new(
            DiagnosticNoteKind::PackageIdentityMustBeValid,
        ));

        let rendered = DiagnosticRenderer::english().render(&diagnostic);

        assert_eq!(
            rendered.message(),
            "cannot use 'Invalid Package' as a Bray package identity"
        );

        let [note] = rendered.notes() else {
            panic!("expected package identity help: {rendered:?}");
        };

        assert_eq!(note.rendered_kind(), RenderedDiagnosticNoteKind::Help);

        assert_eq!(
            note.message(),
            concat!(
                "use a non-reserved lowercase name or dot-separated names, starting each name ",
                "with a letter and using only letters, digits, underscores, or hyphens",
            )
        );
    }

    #[test]
    fn renderer_preserves_both_target_predicate_failure_shapes() {
        let unknown = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::ProjectManifestUnknownTargetPredicateProperty,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::file_path("bray-package.json"))
        .with_arg(DiagnosticArg::project_manifest_field(
            DiagnosticProjectManifestField::TargetPredicate,
        ))
        .with_arg(DiagnosticArg::referenced_name("target.unknown"));

        let mismatch = Diagnostic::new(
            DiagnosticId::new(1),
            DiagnosticKind::ProjectManifestTargetPredicateValueKindMismatch,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::file_path("bray-package.json"))
        .with_arg(DiagnosticArg::project_manifest_field(
            DiagnosticProjectManifestField::TargetPredicate,
        ))
        .with_arg(DiagnosticArg::referenced_name("target.pointer.BITS"))
        .with_arg(DiagnosticArg::expected_target_predicate_value_kind(
            DiagnosticTargetPredicateValueKind::UnsignedInteger,
        ))
        .with_arg(DiagnosticArg::actual_target_predicate_value_kind(
            DiagnosticTargetPredicateValueKind::String,
        ));

        let renderer = DiagnosticRenderer::english();

        assert_eq!(
            renderer.render(&unknown).message(),
            "target predicate property 'target.unknown' is not defined for target predicate in bray-package.json"
        );

        assert_eq!(
            renderer.render(&mismatch).message(),
            "target predicate property 'target.pointer.BITS' in target predicate of bray-package.json accepts unsigned integer values, but received a string value"
        );
    }

    #[test]
    fn renderer_formats_typed_arguments_for_utf8_diagnostics() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(2),
            DiagnosticKind::SourceInvalidUtf8,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::source_input(DiagnosticSourceInput::new(
            0,
            bray_source::SourceInputKind::VirtualText,
            DiagnosticSourceInputOrigin::Name("test source".to_owned()),
        )))
        .with_arg(DiagnosticArg::text_offset(TextSize::new(4)))
        .with_note(DiagnosticNote::new(DiagnosticNoteKind::SourceMustBeUtf8));

        let rendered = DiagnosticRenderer::new(DiagnosticLocale::English).render(&diagnostic);

        assert_eq!(
            rendered.message(),
            "virtual text source input 0 named 'test source' contains invalid UTF-8 starting at byte offset 4"
        );

        let [note] = rendered.notes() else {
            panic!("expected one rendered note: {rendered:?}");
        };

        assert_eq!(note.rendered_kind(), RenderedDiagnosticNoteKind::Help);
        assert_eq!(note.message(), "source inputs must be valid UTF-8");
    }

    #[test]
    fn renderer_localizes_typed_runtime_metadata_failures_and_recovery() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::RuntimeArtifactMetadataInvalid,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::artifact_path("runtime/bray-runtime.brayrt"))
        .with_arg(DiagnosticArg::runtime_artifact_problem(
            DiagnosticRuntimeArtifactProblem::InvalidComponentDependency {
                component: "runtime.scheduler".to_owned(),
                dependency: "runtime.reactor".to_owned(),
            },
        ))
        .with_note(DiagnosticNote::new(
            DiagnosticNoteKind::RuntimeArtifactMustBeUsable,
        ));

        let rendered = DiagnosticRenderer::english().render(&diagnostic);

        assert_eq!(
            rendered.message(),
            "runtime artifact metadata is invalid: runtime/bray-runtime.brayrt: component `runtime.scheduler` has invalid dependency `runtime.reactor`"
        );

        let [note] = rendered.notes() else {
            panic!("expected one rendered note: {rendered:?}");
        };

        assert_eq!(note.rendered_kind(), RenderedDiagnosticNoteKind::Help);

        assert_eq!(
            note.message(),
            "select a readable runtime artifact built for the selected target and runtime ABI"
        );
    }

    #[test]
    fn renderer_localizes_dependency_interface_context() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::InterfaceInvalidMagic,
            SeverityKind::Error,
        )
        .with_note(
            DiagnosticNote::new(DiagnosticNoteKind::InterfaceDependencyContext)
                .with_arg(DiagnosticArg::expected_package_identity(
                    "example.dependency",
                ))
                .with_arg(DiagnosticArg::expected_product_identity("library"))
                .with_arg(DiagnosticArg::artifact_path("dependency.brayi")),
        );

        let rendered = DiagnosticRenderer::english().render(&diagnostic);

        let [note] = rendered.notes() else {
            panic!("expected one rendered note: {rendered:?}");
        };

        assert_eq!(note.rendered_kind(), RenderedDiagnosticNoteKind::Note);

        assert_eq!(
            note.message(),
            "while loading package 'example.dependency' product 'library' from dependency.brayi"
        );
    }

    #[test]
    fn renderer_renders_labels_with_spans_styles_and_typed_args() {
        let span = SourceSpan::new(
            SourceId::new(1),
            TextRange::new(TextSize::new(3), TextSize::new(4)),
        );

        let label = DiagnosticLabel::primary(DiagnosticLabelKind::InvalidCharacter, span).with_arg(
            DiagnosticArg::new(
                DiagnosticArgName::Character,
                DiagnosticArgValue::Character('\u{7f}'),
            ),
        );

        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::LexicalInvalidCharacter,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::new(
            DiagnosticArgName::Character,
            DiagnosticArgValue::Character('\u{7f}'),
        ))
        .with_label(label);

        let rendered = DiagnosticRenderer::english().render(&diagnostic);

        assert_eq!(rendered.message(), "invalid character U+007F");

        let [label] = rendered.labels() else {
            panic!("expected one rendered label: {rendered:?}");
        };

        assert_eq!(label.kind(), DiagnosticLabelKind::InvalidCharacter);
        assert_eq!(label.style(), DiagnosticLabelStyle::Primary);
        assert_eq!(label.span(), span);
        assert_eq!(label.message(), "invalid character U+007F");
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

    #[test]
    fn renderer_localizes_semantic_analysis_limits() {
        let recursion = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::CheckingTypeRepresentationRecursionLimitExceeded,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::actual_count(18))
        .with_arg(DiagnosticArg::maximum_count(17));

        assert_eq!(
            DiagnosticRenderer::english().render(&recursion).message(),
            "type representation analysis required 18 nested declarations but the limit is 17"
        );

        let cases = [
            (
                DiagnosticKind::CheckingImplementationCoherenceLimitExceeded,
                "implementation coherence comparison 18 exceeds the configured limit of 17",
            ),
            (
                DiagnosticKind::CheckingCallableOverloadLimitExceeded,
                "callable overload comparison 18 exceeds the configured limit of 17",
            ),
        ];

        for (kind, expected) in cases {
            let diagnostic = Diagnostic::new(DiagnosticId::new(0), kind, SeverityKind::Error)
                .with_arg(DiagnosticArg::actual_count(18))
                .with_arg(DiagnosticArg::maximum_count(17));

            let rendered = DiagnosticRenderer::english().render(&diagnostic);

            assert_eq!(rendered.message(), expected);
        }

        let singular = Diagnostic::new(
            DiagnosticId::new(1),
            DiagnosticKind::CheckingCallableOverloadLimitExceeded,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::actual_count(2))
        .with_arg(DiagnosticArg::maximum_count(1));

        assert_eq!(
            DiagnosticRenderer::english().render(&singular).message(),
            "callable overload comparison 2 exceeds the configured limit of 1"
        );
    }

    #[test]
    fn renderer_keeps_diagnostic_bag_order() {
        let first = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::RequestMissingSourceInput,
            SeverityKind::Error,
        );

        let second = Diagnostic::new(
            DiagnosticId::new(1),
            DiagnosticKind::RequestInvalidWorkerBudget,
            SeverityKind::Error,
        );

        let bag = DiagnosticBag::from(vec![first, second]);
        let rendered = DiagnosticRenderer::english().render_bag(&bag);

        let ids = rendered
            .iter()
            .map(RenderedDiagnostic::id)
            .collect::<Vec<_>>();

        assert_eq!(ids, vec![DiagnosticId::new(0), DiagnosticId::new(1)]);
    }

    #[test]
    fn renderer_exposes_missing_arguments_as_deterministic_placeholders() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::SourceTextTooLarge,
            SeverityKind::Error,
        );

        let rendered = DiagnosticRenderer::english().render(&diagnostic);

        assert_eq!(
            rendered.message(),
            "{source_input} is too large: {byte_count} bytes"
        );
    }

    #[test]
    fn renderer_formats_terminal_output_headings() {
        let renderer = DiagnosticRenderer::english();

        assert_eq!(renderer.render_severity(SeverityKind::Error), "error");

        assert_eq!(
            renderer.render_label_style(DiagnosticLabelStyle::Secondary),
            "secondary source"
        );

        assert_eq!(
            renderer.render_note_heading(RenderedDiagnosticNoteKind::Note),
            "note"
        );

        assert_eq!(
            renderer.render_note_heading(RenderedDiagnosticNoteKind::Help),
            "help"
        );
    }

    #[test]
    fn rendered_values_are_send_and_sync() {
        assert_send_sync::<DiagnosticRenderer>();
        assert_send_sync::<RenderedDiagnostic>();
        assert_send_sync::<RenderedDiagnosticLabel>();
    }

    fn assert_send_sync<T: Send + Sync>() {}
}
