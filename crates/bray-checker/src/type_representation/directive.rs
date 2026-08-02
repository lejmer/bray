use std::collections::BTreeSet;

use bray_compiler_known::RepresentationRole;
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::DiagnosticKind;
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
        let mut mode_present = false;
        let mut alignment_present = false;
        let mut packing_present = false;
        let mut tag_present = false;

        for argument in directive.arguments() {
            match argument.name() {
                DirectiveArgumentName::Positional if !mode_present => {
                    mode_present = true;

                    let text = self.argument_text(argument.expression())?;

                    layout.mode = match text {
                        "stable" => DeclaredLayoutMode::Stable,
                        "c" => DeclaredLayoutMode::C,
                        "transparent" => DeclaredLayoutMode::Transparent,
                        _ => {
                            self.add_diagnostic(
                                DiagnosticKind::CheckingInvalidLayoutDirective,
                                expression_span(argument.expression()),
                            );

                            *recovered = true;

                            DeclaredLayoutMode::Default
                        }
                    };
                }
                DirectiveArgumentName::Named(name)
                    if name.as_str() == "align" && !alignment_present =>
                {
                    alignment_present = true;
                    layout.alignment = self.check_power_of_two(argument.expression(), recovered)?;
                }
                DirectiveArgumentName::Named(name)
                    if name.as_str() == "pack" && !packing_present =>
                {
                    packing_present = true;
                    layout.packing = self.check_power_of_two(argument.expression(), recovered)?;
                }
                DirectiveArgumentName::Named(name) if name.as_str() == "tag" && !tag_present => {
                    tag_present = true;
                    layout.tag_type = self.context.integer_type(argument.expression())?;

                    if layout.tag_type.is_none() {
                        self.add_diagnostic(
                            DiagnosticKind::CheckingInvalidUnionTag,
                            expression_span(argument.expression()),
                        );

                        *recovered = true;
                    }
                }
                DirectiveArgumentName::Positional
                | DirectiveArgumentName::Named(_)
                | DirectiveArgumentName::Recovered => {
                    self.add_diagnostic(
                        DiagnosticKind::CheckingInvalidLayoutDirective,
                        expression_span(argument.expression()),
                    );

                    *recovered = true;
                }
            }
        }

        let is_union = matches!(definition.subject(), NamedTypeSymbolId::Union(_));

        let invalid = !mode_present
            || layout.mode == DeclaredLayoutMode::Transparent
                && (is_union
                    || definition.fields().len() != 1
                    || layout.alignment.is_some()
                    || layout.packing.is_some()
                    || layout.tag_type.is_some())
            || layout.packing.is_some()
                && (layout.mode != DeclaredLayoutMode::Stable || !members.plain)
            || layout.tag_type.is_some() && !is_union
            || is_union && layout.mode == DeclaredLayoutMode::C && layout.tag_type.is_none();

        if invalid {
            self.add_diagnostic(
                DiagnosticKind::CheckingInvalidLayoutDirective,
                directive_span(directive.syntax()),
            );

            *recovered = true;
        }

        Ok(layout)
    }

    fn check_power_of_two(
        &mut self,
        expression: DeclarationExpressionTemplate,
        recovered: &mut bool,
    ) -> CheckerFactResult<Option<u64>> {
        let result = self.context.unsigned_integer(expression)?;

        self.diagnostics
            .add_range(result.diagnostics().iter().cloned());

        match result.value() {
            Some(value) if value.is_power_of_two() => Ok(Some(*value)),
            Some(_) | None => {
                self.add_diagnostic(
                    DiagnosticKind::CheckingInvalidLayoutDirective,
                    expression_span(expression),
                );

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
        if matches!(definition.subject(), bray_symbols::NamedTypeSymbolId::Struct(_)) {
            return Ok((Vec::new(), None));
        }

        let explicitly_laid_out = layout != DeclaredLayoutMode::Default;

        let mut tags = Vec::with_capacity(definition.variants().len());
        let mut explicit_count = 0_usize;
        let mut values = BTreeSet::new();

        for (ordinal, variant) in definition.variants().iter().enumerate() {
            let declared = variant
                .directives()
                .directives()
                .iter()
                .find(|directive| directive.kind() == DirectiveKind::Tag);

            if !explicitly_laid_out && let Some(directive) = declared {
                self.add_diagnostic(
                    DiagnosticKind::CheckingInvalidUnionTag,
                    directive_span(directive.syntax()),
                );

                *recovered = true;
            }

            let explicit = declared.filter(|_| explicitly_laid_out);

            let value = match explicit {
                Some(directive) => {
                    explicit_count += 1;

                    let [argument] = directive.arguments() else {
                        self.add_diagnostic(
                            DiagnosticKind::CheckingInvalidUnionTag,
                            directive_span(directive.syntax()),
                        );

                        *recovered = true;
                        continue;
                    };

                    let result = self
                        .context
                        .integer_constant(argument.expression(), tag_type)?;

                    self.diagnostics
                        .add_range(result.diagnostics().iter().cloned());

                    let Some(value) = result.value() else {
                        self.add_diagnostic(
                            DiagnosticKind::CheckingInvalidUnionTag,
                            expression_span(argument.expression()),
                        );

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
            if !values.insert(value.clone()) {
                self.add_diagnostic(DiagnosticKind::CheckingInvalidUnionTag, variant.span());

                *recovered = true;
            }

            if tag_type.is_some_and(|tag_type| !tag_type.accepts(&value)) {
                self.add_diagnostic(DiagnosticKind::CheckingInvalidUnionTag, variant.span());

                *recovered = true;
            }

            tags.push(DeclaredUnionTag::new(variant.id(), value));
        }

        if explicit_count != 0 && explicit_count != definition.variants().len() {
            self.add_diagnostic(DiagnosticKind::CheckingInvalidUnionTag, definition.span());

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
                    self.add_diagnostic(DiagnosticKind::CheckingInvalidUnionTag, definition.span());
                    *recovered = true;

                    None
                } else {
                    let role = default_tag_role(maximum);

                    match role {
                        Some(role) => Some(self.context.integer_type_for_role(role)?),
                        None => {
                            self.add_diagnostic(
                                DiagnosticKind::CheckingInvalidUnionTag,
                                definition.span(),
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

        let copy = if definition.has_lifecycle() || !directive.arguments().is_empty() {
            None
        } else {
            match members.copyable {
                Copyability::Always => Some(DeclaredCopyContract::Unconditional),
                Copyability::Conditional if definition.is_generic() => {
                    Some(DeclaredCopyContract::Conditional)
                }
                Copyability::Conditional | Copyability::Never => None,
            }
        };

        match copy {
            Some(copy) => copy,
            None => {
                self.add_diagnostic(
                    DiagnosticKind::CheckingInvalidCopyContract,
                    directive_span(directive.syntax()),
                );

                *recovered = true;

                DeclaredCopyContract::Absent
            }
        }
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
