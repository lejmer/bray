use std::collections::BTreeMap;

use bray_compiler_known::RepresentationRole;
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticCopyContractProblem, DiagnosticId, DiagnosticKind,
    DiagnosticLabel, DiagnosticLabelKind, DiagnosticLayoutOption, DiagnosticLayoutProblem,
    DiagnosticNote, DiagnosticNoteKind, DiagnosticRelatedLocation,
    DiagnosticRelatedLocationKind, DiagnosticUnionTagProblem, SeverityKind,
};
use bray_source::SourceSpan;
use bray_symbols::{
    DeclarationExpressionTemplate, DeclaredCopyContract, DeclaredLayoutMode, DeclaredUnionTag,
    DirectiveArgumentName, DirectiveKind, IntegerConstant, IntegerSign, NamedTypeSymbolId,
};

use crate::{CheckerFactResult, CheckerSource};

use super::check::{Copyability, MemberRepresentation, RepresentationChecker};
use super::model::{DeclaredTypeDefinition, RepresentationIntegerType, TypeRepresentationContext};

impl<C> RepresentationChecker<'_, C>
where
    C: TypeRepresentationContext + ?Sized,
{
    pub(super) fn check_layout(
        &mut self,
        definition: &DeclaredTypeDefinition,
        members: &MemberRepresentation,
        recovered: &mut bool,
    ) -> CheckerFactResult<RequestedLayout> {
        let Some(directive) = definition
            .directives()
            .directives()
            .iter()
            .find(|directive| directive.kind() == DirectiveKind::Layout)
        else {
            return Ok(RequestedLayout::default());
        };

        let mut layout = RequestedLayout::default();
        let mut mode_span = None;
        let mut alignment_spans = Vec::new();
        let mut packing_spans = Vec::new();
        let mut tag_spans = Vec::new();

        for argument in directive.arguments() {
            match argument.name() {
                DirectiveArgumentName::Positional if mode_span.is_none() => {
                    mode_span = Some(expression_span(argument.expression()));

                    let text = self.argument_text(argument.expression())?;

                    layout.mode = match text {
                        "stable" => DeclaredLayoutMode::Stable,
                        "c" => DeclaredLayoutMode::C,
                        "transparent" => DeclaredLayoutMode::Transparent,
                        _ => {
                            self.add_layout_diagnostic(
                                DiagnosticLayoutProblem::UnsupportedMode(text.to_owned()),
                                expression_span(argument.expression()),
                                &[],
                            );

                            *recovered = true;

                            DeclaredLayoutMode::Default
                        }
                    };
                }
                DirectiveArgumentName::Named(name)
                    if name.as_str() == "align" && alignment_spans.is_empty() =>
                {
                    alignment_spans.push(expression_span(argument.expression()));

                    layout.alignment = self.check_power_of_two(
                        argument.expression(),
                        DiagnosticLayoutOption::Alignment,
                        recovered,
                    )?;
                }
                DirectiveArgumentName::Named(name)
                    if name.as_str() == "pack" && packing_spans.is_empty() =>
                {
                    packing_spans.push(expression_span(argument.expression()));

                    layout.packing = self.check_power_of_two(
                        argument.expression(),
                        DiagnosticLayoutOption::Packing,
                        recovered,
                    )?;
                }
                DirectiveArgumentName::Named(name) if name.as_str() == "tag" && tag_spans.is_empty() => {
                    tag_spans.push(expression_span(argument.expression()));
                    layout.tag_type = self.context.integer_type(argument.expression())?;

                    if layout.tag_type.is_none() {
                        self.add_union_tag_diagnostic(
                            DiagnosticUnionTagProblem::UnsupportedType(
                                self.argument_text(argument.expression())?.to_owned(),
                            ),
                            expression_span(argument.expression()),
                            &[],
                        );

                        *recovered = true;
                    }
                }
                DirectiveArgumentName::Positional => {
                    self.add_layout_diagnostic(
                        DiagnosticLayoutProblem::UnexpectedPositionalArgument,
                        expression_span(argument.expression()),
                        mode_span.as_slice(),
                    );

                    *recovered = true;
                }
                DirectiveArgumentName::Named(name) => {
                    let (problem, previous) = match name.as_str() {
                        "align" => (
                            DiagnosticLayoutProblem::DuplicateOption(
                                DiagnosticLayoutOption::Alignment,
                            ),
                            &mut alignment_spans,
                        ),
                        "pack" => (
                            DiagnosticLayoutProblem::DuplicateOption(
                                DiagnosticLayoutOption::Packing,
                            ),
                            &mut packing_spans,
                        ),
                        "tag" => (
                            DiagnosticLayoutProblem::DuplicateOption(DiagnosticLayoutOption::Tag),
                            &mut tag_spans,
                        ),
                        _ => {
                            self.add_layout_diagnostic(
                                DiagnosticLayoutProblem::UnknownOption(name.as_str().to_owned()),
                                expression_span(argument.expression()),
                                &[],
                            );

                            *recovered = true;

                            continue;
                        }
                    };

                    let span = expression_span(argument.expression());

                    self.add_layout_diagnostic(
                        problem,
                        span,
                        previous,
                    );

                    previous.push(span);

                    *recovered = true;
                }
                DirectiveArgumentName::Recovered => {}
            }
        }

        let is_union = matches!(definition.subject(), NamedTypeSymbolId::Union(_));

        let directive_span = directive_span(directive.syntax());
        let mut problems = Vec::new();

        if mode_span.is_none() {
            problems.push(DiagnosticLayoutProblem::MissingMode);
        }

        if layout.mode == DeclaredLayoutMode::Transparent {
            if is_union {
                problems.push(DiagnosticLayoutProblem::TransparentUnion);
            }

            if definition.fields().len() != 1 {
                problems.push(DiagnosticLayoutProblem::TransparentFieldCount {
                    actual: u64::try_from(definition.fields().len()).unwrap_or(u64::MAX),
                });
            }

            if layout.alignment.is_some() {
                problems.push(DiagnosticLayoutProblem::TransparentOption(
                    DiagnosticLayoutOption::Alignment,
                ));
            }

            if layout.packing.is_some() {
                problems.push(DiagnosticLayoutProblem::TransparentOption(
                    DiagnosticLayoutOption::Packing,
                ));
            }

            if layout.tag_type.is_some() {
                problems.push(DiagnosticLayoutProblem::TransparentOption(
                    DiagnosticLayoutOption::Tag,
                ));
            }
        }

        if layout.packing.is_some() && layout.mode != DeclaredLayoutMode::Stable {
            problems.push(DiagnosticLayoutProblem::PackingRequiresStable);
        }

        if layout.packing.is_some() && !members.plain {
            problems.push(DiagnosticLayoutProblem::PackingRequiresPlainStorage);
        }

        if layout.tag_type.is_some() && !is_union {
            problems.push(DiagnosticLayoutProblem::TagRequiresUnion);
        }

        if is_union && layout.mode == DeclaredLayoutMode::C && layout.tag_type.is_none() {
            problems.push(DiagnosticLayoutProblem::CUnionRequiresTag);
        }

        for problem in problems {
            let mut diagnostic = self.layout_diagnostic(problem, directive_span, &[]);

            if layout.mode == DeclaredLayoutMode::Transparent {
                for member in definition.fields() {
                    diagnostic = diagnostic.with_related_location(DiagnosticRelatedLocation::new(
                        DiagnosticRelatedLocationKind::RepresentationMember,
                        member.span(),
                    ));
                }
            }

            self.diagnostics.add(diagnostic);
            *recovered = true;
        }

        Ok(layout)
    }

    fn check_power_of_two(
        &mut self,
        expression: DeclarationExpressionTemplate,
        option: DiagnosticLayoutOption,
        recovered: &mut bool,
    ) -> CheckerFactResult<Option<u64>> {
        let result = self.context.unsigned_integer(expression)?;

        self.diagnostics
            .add_range(result.diagnostics().iter().cloned());

        match result.value() {
            Some(value) if value.is_power_of_two() => Ok(Some(*value)),
            Some(value) => {
                self.add_layout_diagnostic(
                    DiagnosticLayoutProblem::OptionNotPowerOfTwo {
                        option,
                        value: *value,
                    },
                    expression_span(expression),
                    &[],
                );

                *recovered = true;

                Ok(None)
            }
            None => {
                if result.diagnostics().is_empty() {
                    self.add_layout_diagnostic(
                        DiagnosticLayoutProblem::OptionNotConstant(option),
                        expression_span(expression),
                        &[],
                    );
                }

                *recovered = true;

                Ok(None)
            }
        }
    }

    pub(super) fn check_union_tags(
        &mut self,
        definition: &DeclaredTypeDefinition,
        layout: DeclaredLayoutMode,
        tag_type: Option<RepresentationIntegerType>,
        recovered: &mut bool,
    ) -> CheckerFactResult<(Vec<DeclaredUnionTag>, Option<RepresentationIntegerType>)> {
        if matches!(
            definition.subject(),
            bray_symbols::NamedTypeSymbolId::Struct(_)
        ) {
            return Ok((Vec::new(), None));
        }

        let explicitly_laid_out = layout != DeclaredLayoutMode::Default;

        let mut tags = Vec::with_capacity(definition.variants().len());
        let mut explicit_count = 0_usize;
        let mut values: BTreeMap<IntegerConstant, Vec<SourceSpan>> = BTreeMap::new();

        for (ordinal, variant) in definition.variants().iter().enumerate() {
            let declared = variant
                .directives()
                .directives()
                .iter()
                .find(|directive| directive.kind() == DirectiveKind::Tag);

            if !explicitly_laid_out && let Some(directive) = declared {
                self.add_union_tag_diagnostic(
                    DiagnosticUnionTagProblem::RequiresExplicitLayout,
                    directive_span(directive.syntax()),
                    &[],
                );

                *recovered = true;
            }

            let explicit = declared.filter(|_| explicitly_laid_out);

            let value = match explicit {
                Some(directive) => {
                    explicit_count += 1;

                    let [argument] = directive.arguments() else {
                        self.add_union_tag_diagnostic(
                            DiagnosticUnionTagProblem::ArgumentCount {
                                actual: u64::try_from(directive.arguments().len())
                                    .unwrap_or(u64::MAX),
                            },
                            directive_span(directive.syntax()),
                            &[],
                        );

                        *recovered = true;
                        continue;
                    };

                    let result = self
                        .context
                        .integer_constant(argument.expression(), None)?;

                    self.diagnostics
                        .add_range(result.diagnostics().iter().cloned());

                    let Some(value) = result.value() else {
                        if result.diagnostics().is_empty() {
                            self.add_union_tag_diagnostic(
                                DiagnosticUnionTagProblem::ValueNotConstant,
                                expression_span(argument.expression()),
                                &[],
                            );
                        }

                        *recovered = true;
                        continue;
                    };

                    // Tag constants are immutable and Arc-backed, so the durable tag shares
                    // arbitrary-width magnitude storage with the checked constant.
                    value.clone()
                }
                None => IntegerConstant::from_u64(u64::try_from(ordinal).unwrap_or(u64::MAX)),
            };

            // The uniqueness set and published tag share immutable magnitude storage.
            let previous = values.entry(value.clone()).or_default();

            if !previous.is_empty() {
                self.add_union_tag_diagnostic(
                    DiagnosticUnionTagProblem::DuplicateValue,
                    variant.span(),
                    previous,
                );

                *recovered = true;
            }

            previous.push(variant.span());

            if let Some(tag_type) = tag_type
                && !tag_type.accepts(&value)
            {
                self.add_union_tag_diagnostic(
                    tag_value_outside_type_problem(tag_type, &value),
                    variant.span(),
                    &[],
                );

                *recovered = true;
            }

            tags.push(DeclaredUnionTag::new(variant.id(), value));
        }

        if explicit_count != 0 && explicit_count != definition.variants().len() {
            self.add_union_tag_diagnostic(
                DiagnosticUnionTagProblem::PartialExplicitTags {
                    explicit: u64::try_from(explicit_count).unwrap_or(u64::MAX),
                    total: u64::try_from(definition.variants().len()).unwrap_or(u64::MAX),
                },
                definition.span(),
                &[],
            );

            *recovered = true;
        }

        let tag_type = match tag_type {
            Some(tag_type) => Some(tag_type),
            None if matches!(
                layout,
                DeclaredLayoutMode::Default | DeclaredLayoutMode::Stable
            ) =>
            {
                let Some(maximum) = tags.iter().map(DeclaredUnionTag::value).max() else {
                    return self
                        .context
                        .integer_type_for_role(RepresentationRole::ScalarU8)
                        .map(|tag_type| (tags, Some(tag_type)));
                };

                if maximum.sign() == IntegerSign::Negative {
                    self.add_union_tag_diagnostic(
                        DiagnosticUnionTagProblem::NegativeInferredValue,
                        definition.span(),
                        &[],
                    );

                    *recovered = true;

                    None
                } else {
                    let role = default_tag_role(maximum);

                    match role {
                        Some(role) => Some(self.context.integer_type_for_role(role)?),
                        None => {
                            self.add_union_tag_diagnostic(
                                DiagnosticUnionTagProblem::InferredValueTooWide {
                                    actual_bits: u64::try_from(crate::constant::significant_bits(
                                        maximum.magnitude(),
                                    ))
                                    .unwrap_or(u64::MAX),
                                    maximum_bits: 128,
                                },
                                definition.span(),
                                &[],
                            );

                            *recovered = true;

                            None
                        }
                    }
                }
            }
            None => None,
        };

        Ok((tags, tag_type))
    }

    pub(super) fn check_copy(
        &mut self,
        definition: &DeclaredTypeDefinition,
        members: &MemberRepresentation,
        recovered: &mut bool,
    ) -> DeclaredCopyContract {
        let Some(directive) = definition
            .directives()
            .directives()
            .iter()
            .find(|directive| directive.kind() == DirectiveKind::Copy)
        else {
            return DeclaredCopyContract::Absent;
        };

        let span = directive_span(directive.syntax());
        let mut problems = Vec::new();

        if definition.has_lifecycle() {
            problems.push(DiagnosticCopyContractProblem::LifecycleBehavior);
        }

        if !directive.arguments().is_empty() {
            problems.push(DiagnosticCopyContractProblem::UnexpectedArguments {
                actual: u64::try_from(directive.arguments().len()).unwrap_or(u64::MAX),
            });
        }

        if members.copyable == Copyability::Conditional && !definition.is_generic() {
            problems.push(DiagnosticCopyContractProblem::ConditionalMembersRequireGenericType);
        }

        if members.copyable == Copyability::Never {
            problems.push(DiagnosticCopyContractProblem::NonCopyableMember);
        }

        let copy_invalid = !problems.is_empty();

        for problem in problems {
            let mut diagnostic = self.copy_diagnostic(problem, span);

            if problem == DiagnosticCopyContractProblem::NonCopyableMember {
                for member in &members.non_copyable_members {
                    diagnostic = diagnostic.with_related_location(DiagnosticRelatedLocation::new(
                        DiagnosticRelatedLocationKind::NonCopyableMember,
                        *member,
                    ));
                }
            }

            self.diagnostics.add(diagnostic);
            *recovered = true;
        }

        if copy_invalid {
            return DeclaredCopyContract::Absent;
        }

        match members.copyable {
            Copyability::Always => DeclaredCopyContract::Unconditional,
            Copyability::Conditional => DeclaredCopyContract::Conditional,
            Copyability::Never => DeclaredCopyContract::Absent,
        }
    }

    fn add_layout_diagnostic(
        &mut self,
        problem: DiagnosticLayoutProblem,
        span: SourceSpan,
        previous: &[SourceSpan],
    ) {
        let diagnostic = self.layout_diagnostic(problem, span, previous);
        self.diagnostics.add(diagnostic);
    }

    fn layout_diagnostic(
        &self,
        problem: DiagnosticLayoutProblem,
        span: SourceSpan,
        previous: &[SourceSpan],
    ) -> Diagnostic {
        let mut diagnostic = self.representation_diagnostic(
            DiagnosticKind::CheckingInvalidLayoutDirective,
            span,
        )
        .with_arg(DiagnosticArg::layout_problem(problem))
        .with_note(DiagnosticNote::new(
            DiagnosticNoteKind::TypeLayoutDirectiveForms,
        ));

        for previous in previous.iter().copied().filter(|previous| *previous != span) {
            diagnostic = diagnostic.with_related_location(DiagnosticRelatedLocation::new(
                DiagnosticRelatedLocationKind::FirstDirective,
                previous,
            ));
        }

        diagnostic
    }

    fn add_union_tag_diagnostic(
        &mut self,
        problem: DiagnosticUnionTagProblem,
        span: SourceSpan,
        previous: &[SourceSpan],
    ) {
        let mut diagnostic = self
            .representation_diagnostic(DiagnosticKind::CheckingInvalidUnionTag, span)
            .with_arg(DiagnosticArg::union_tag_problem(problem))
            .with_note(DiagnosticNote::new(
                DiagnosticNoteKind::UnionTagDirectiveForms,
            ));

        for previous in previous.iter().copied().filter(|previous| *previous != span) {
            diagnostic = diagnostic.with_related_location(DiagnosticRelatedLocation::new(
                DiagnosticRelatedLocationKind::FirstDirective,
                previous,
            ));
        }

        self.diagnostics.add(diagnostic);
    }

    fn copy_diagnostic(
        &self,
        problem: DiagnosticCopyContractProblem,
        span: SourceSpan,
    ) -> Diagnostic {
        self.representation_diagnostic(DiagnosticKind::CheckingInvalidCopyContract, span)
            .with_arg(DiagnosticArg::copy_contract_problem(problem))
            .with_note(DiagnosticNote::new(
                DiagnosticNoteKind::CopyContractRequirements,
            ))
    }

    fn representation_diagnostic(&self, kind: DiagnosticKind, span: SourceSpan) -> Diagnostic {
        let id = u32::try_from(self.diagnostics.len()).unwrap_or(u32::MAX);

        Diagnostic::new(DiagnosticId::new(id), kind, SeverityKind::Error)
            .with_primary_span(span)
            .with_label(DiagnosticLabel::primary(
                DiagnosticLabelKind::InvalidTypeRepresentationContract,
                span,
            ))
    }

    fn argument_text(&self, expression: DeclarationExpressionTemplate) -> CheckerFactResult<&str> {
        self.context
            .source(expression.syntax())
            .map(CheckerSource::text)
            .map_err(crate::CheckerFactError::Infrastructure)
    }
}

fn default_tag_role(maximum: &IntegerConstant) -> Option<RepresentationRole> {
    match crate::constant::significant_bits(maximum.magnitude()) {
        0..=8 => Some(RepresentationRole::ScalarU8),
        9..=16 => Some(RepresentationRole::ScalarU16),
        17..=32 => Some(RepresentationRole::ScalarU32),
        33..=64 => Some(RepresentationRole::ScalarU64),
        65..=128 => Some(RepresentationRole::ScalarU128),
        _ => None,
    }
}

fn tag_value_outside_type_problem(
    tag_type: RepresentationIntegerType,
    value: &IntegerConstant,
) -> DiagnosticUnionTagProblem {
    let (signed, width_bits) = match tag_type.representation() {
        bray_compiler_known::IntegerRepresentation::Signed(bits) => (true, bits),
        bray_compiler_known::IntegerRepresentation::Unsigned(bits) => (false, bits),
        bray_compiler_known::IntegerRepresentation::TargetSigned => {
            (true, tag_type.target_width().get())
        }
        bray_compiler_known::IntegerRepresentation::TargetUnsigned => {
            (false, tag_type.target_width().get())
        }
    };

    DiagnosticUnionTagProblem::ValueOutsideSelectedType {
        signed,
        width_bits,
        value_negative: value.sign() == IntegerSign::Negative,
        value_bits: u64::try_from(crate::constant::significant_bits(value.magnitude()))
            .unwrap_or(u64::MAX),
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct RequestedLayout {
    pub(super) mode: DeclaredLayoutMode,
    pub(super) alignment: Option<u64>,
    pub(super) packing: Option<u64>,
    pub(super) tag_type: Option<RepresentationIntegerType>,
}

fn expression_span(expression: DeclarationExpressionTemplate) -> SourceSpan {
    directive_span(expression.syntax())
}

fn directive_span(syntax: SyntaxAnchor) -> SourceSpan {
    SourceSpan::new(syntax.source_id(), syntax.full_range())
}
