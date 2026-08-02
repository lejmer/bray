use bray_binder::BinderFactContext;
use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundMemberSelector, BoundReferenceTarget,
    BoundUnqualifiedVariantExpression, ConstructionDefaultProvider, ConstructionInputId,
    ConstructionTarget,
};
use bray_checker::{ConstructionInputSurface, OperationCandidate, OperationCandidateState};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind,
};
use bray_source::SourceSpan;
use bray_symbols::{
    AnySymbolId, MemberLookupResult, NamedTypeSymbolId, RuntimeDefaultPresence, SymbolFactRequest,
    SymbolName, TypeData, TypeId, UnionPayloadFieldTypeFact, UnionVariantSymbolId,
};

use super::super::super::Compilation;
use super::super::super::binder::{CompilationBinderFacts, binder_fact_error};
use crate::fact::FactQueryError;

enum ContextualVariantTarget {
    ExpectedTypeIsNotUnion,
    Missing,
    Found {
        variant: UnionVariantSymbolId,
        substitution: bray_symbols::GenericSubstitutionId,
    },
}

impl Compilation {
    pub(super) fn leading_dot_variant_candidate(
        &self,
        facts: &CompilationBinderFacts<'_>,
        result_type: TypeId,
        selector: Option<&BoundMemberSelector>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<OperationCandidate>, FactQueryError> {
        let Some(BoundMemberSelector::Name(name)) = selector else {
            return Ok(None);
        };

        let ContextualVariantTarget::Found {
            variant,
            substitution,
        } = self.contextual_variant_target(facts, result_type, name)?
        else {
            return Ok(None);
        };

        self.union_variant_candidate(facts, variant, result_type, substitution, diagnostics)
    }

    pub(super) fn unqualified_variant_candidate(
        &self,
        facts: &CompilationBinderFacts<'_>,
        result_type: TypeId,
        variant: &BoundUnqualifiedVariantExpression,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<OperationCandidate>, FactQueryError> {
        let target = self.contextual_variant_target(facts, result_type, variant.name())?;

        match target {
            ContextualVariantTarget::Found {
                variant: target,
                substitution,
            } => self.union_variant_candidate(
                facts,
                target,
                result_type,
                substitution,
                diagnostics,
            ),
            ContextualVariantTarget::ExpectedTypeIsNotUnion => {
                diagnostics.add(unqualified_variant_diagnostic(
                    variant,
                    DiagnosticKind::BindingUnresolvedName,
                ));

                Ok(None)
            }
            ContextualVariantTarget::Missing => {
                diagnostics.add(unqualified_variant_diagnostic(
                    variant,
                    DiagnosticKind::CheckingUnknownUnionVariant,
                ));

                Ok(None)
            }
        }
    }

    pub(super) fn union_variant_construction_candidate(
        &self,
        facts: &CompilationBinderFacts<'_>,
        unit: &bray_bound_tree::BoundUnit,
        callee: BoundExpressionId,
        result_type: TypeId,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<OperationCandidate>, FactQueryError> {
        let variant = match unit.view().expression(callee) {
            Some(BoundExpression::LeadingDotVariant(variant)) => {
                return self.leading_dot_variant_candidate(
                    facts,
                    result_type,
                    variant.selector(),
                    diagnostics,
                );
            }
            Some(BoundExpression::Name(name)) => {
                let BoundReferenceTarget::Surface(AnySymbolId::UnionVariant(variant)) =
                    name.target()
                else {
                    return Ok(None);
                };

                variant
            }
            Some(BoundExpression::UnqualifiedVariant(variant)) => {
                return self.unqualified_variant_candidate(
                    facts,
                    result_type,
                    variant,
                    diagnostics,
                );
            }
            Some(BoundExpression::MemberAccess(member)) => {
                return self.leading_dot_variant_candidate(
                    facts,
                    result_type,
                    member.selector(),
                    diagnostics,
                );
            }
            _ => return Ok(None),
        };

        let data = facts
            .semantic_values()
            .type_data(result_type)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let TypeData::Named {
            definition: NamedTypeSymbolId::Union(union),
            substitution,
        } = data.as_ref()
        else {
            return Ok(None);
        };

        let record = facts
            .symbols()
            .union_variant(variant)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        if record.union() != *union {
            return Ok(None);
        }

        self.union_variant_candidate(facts, variant, result_type, *substitution, diagnostics)
    }

    pub(super) fn unqualified_variant_reference<'unit>(
        &self,
        unit: &'unit bray_bound_tree::BoundUnit,
        expression: BoundExpressionId,
    ) -> Option<&'unit BoundUnqualifiedVariantExpression> {
        match unit.view().expression(expression) {
            Some(BoundExpression::UnqualifiedVariant(variant)) => Some(variant),
            Some(BoundExpression::Call(call)) => match unit.view().expression(call.callee()) {
                Some(BoundExpression::UnqualifiedVariant(variant)) => Some(variant),
                _ => None,
            },
            _ => None,
        }
    }

    fn contextual_variant_target(
        &self,
        facts: &CompilationBinderFacts<'_>,
        result_type: TypeId,
        name: &SymbolName,
    ) -> Result<ContextualVariantTarget, FactQueryError> {
        let data = facts
            .semantic_values()
            .type_data(result_type)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let TypeData::Named {
            definition: NamedTypeSymbolId::Union(union),
            substitution,
        } = data.as_ref()
        else {
            return Ok(ContextualVariantTarget::ExpectedTypeIsNotUnion);
        };

        let MemberLookupResult::Found(AnySymbolId::UnionVariant(variant)) =
            facts.symbols().lookup_member((*union).into(), name.as_str())
        else {
            return Ok(ContextualVariantTarget::Missing);
        };

        Ok(ContextualVariantTarget::Found {
            variant,
            substitution: *substitution,
        })
    }

    fn union_variant_candidate(
        &self,
        facts: &CompilationBinderFacts<'_>,
        variant: bray_symbols::UnionVariantSymbolId,
        result_type: TypeId,
        substitution: bray_symbols::GenericSubstitutionId,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<OperationCandidate>, FactQueryError> {
        let record = facts
            .symbols()
            .union_variant(variant)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let inputs = record
            .payload_fields()
            .iter()
            .copied()
            .map(|field| self.union_payload_input(facts, field, substitution, diagnostics))
            .collect::<Result<Option<Vec<_>>, _>>()?;

        let Some(inputs) = inputs else {
            return Ok(None);
        };

        let is_recovered = inputs.iter().any(|(_, is_recovered)| *is_recovered);
        let inputs = inputs.into_iter().map(|(input, _)| input);

        let key = facts
            .symbol_key(variant.into())
            .map_err(binder_fact_error)?
            .ok_or(FactQueryError::InfrastructureFailure)?;

        // The candidate owns the shared key returned by the immutable symbol table.
        Ok(Some(OperationCandidate::symbol_construction(
            key.clone(),
            ConstructionTarget::UnionVariant(variant),
            result_type,
            inputs,
            if is_recovered {
                OperationCandidateState::Recovered
            } else {
                OperationCandidateState::Available
            },
        )))
    }

    fn union_payload_input(
        &self,
        facts: &CompilationBinderFacts<'_>,
        field: bray_symbols::UnionPayloadFieldSymbolId,
        substitution: bray_symbols::GenericSubstitutionId,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<(ConstructionInputSurface, bool)>, FactQueryError> {
        let record = facts
            .symbols()
            .union_payload_field(field)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let Some(name) = facts.symbols().member_name(field.into()).cloned() else {
            return Ok(None);
        };

        let Some(ty) = self.resolve_member_type(
            facts,
            SymbolFactRequest::<UnionPayloadFieldTypeFact>::new(field),
            substitution,
            diagnostics,
        )?
        else {
            return Ok(None);
        };

        let presence = record.default_presence();

        let default = record
            .default_provider()
            .map(ConstructionDefaultProvider::UnionPayload);

        Ok(Some((
            ConstructionInputSurface::new(
                ConstructionInputId::UnionPayloadField(field),
                name,
                record.position(),
                ty,
                default,
                record.ordinal(),
            ),
            presence == RuntimeDefaultPresence::Recovered,
        )))
    }
}

pub(super) fn unqualified_variant_diagnostic(
    variant: &BoundUnqualifiedVariantExpression,
    kind: DiagnosticKind,
) -> Diagnostic {
    let anchor = variant.origin().source_anchor().syntax();
    let span = SourceSpan::new(anchor.source_id(), anchor.full_range());

    Diagnostic::new(
        DiagnosticId::new(anchor.full_range().start().bytes()),
        kind,
        SeverityKind::Error,
    )
    .with_primary_span(span)
    .with_arg(DiagnosticArg::referenced_name(variant.name().as_str()))
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{ConstructionTarget, SelectedOperation, SemanticSelection};
    use bray_diagnostics::{DiagnosticArg, DiagnosticKind};

    use crate::test_support::{
        compilation, diagnostic_kinds, source_callable_body_key, source_function_body_key,
    };

    #[test]
    fn qualified_leading_dot_and_unqualified_forms_select_union_construction() {
        let compilation = compilation(&union_source(
            "func main() -> Maybe",
            concat!(
                "    let qualified = Maybe.Some(value = 1);\n",
                "    let qualified_absent = Maybe.None;\n",
                "    let leading: Maybe = .Some(value = 2);\n",
                "    let contextual: Maybe = Some(value = 3);\n",
                "    let absent: Maybe = None;\n",
                "\n",
                "    return contextual;\n",
            ),
            "",
        ));

        let selections = compilation
            .semantic_selections(source_callable_body_key(&compilation))
            .unwrap_or_else(|error| panic!("variant selections must publish: {error:?}"));

        let variants = selections
            .value()
            .entries()
            .iter()
            .filter(|entry| {
                matches!(
                    entry.selection(),
                    SemanticSelection::Operation(SelectedOperation::Construction(construction))
                        if matches!(construction.target(), ConstructionTarget::UnionVariant(_))
                )
            })
            .count();

        assert_eq!(
            variants,
            5,
            "entries: {:?}; diagnostics: {:?}",
            selections.value().entries(),
            selections.diagnostics()
        );

        assert!(selections.diagnostics().is_empty(), "{selections:?}");
    }

    #[test]
    fn callable_result_type_provides_context_for_unqualified_variants() {
        let compilation = compilation(&union_source(
            "func main() -> Maybe",
            "    return Some(value = 1);\n",
            "",
        ));

        let selections = compilation
            .semantic_selections(source_callable_body_key(&compilation))
            .unwrap_or_else(|error| panic!("return variant selection must publish: {error:?}"));

        assert!(
            has_union_construction(selections.value()),
            "entries: {:?}; diagnostics: {:?}",
            selections.value().entries(),
            selections.diagnostics()
        );

        assert!(selections.diagnostics().is_empty(), "{selections:?}");
    }

    #[test]
    fn known_parameter_type_provides_context_for_unqualified_variants() {
        let compilation = compilation(&union_source(
            "func main()",
            "    accept(Some(value = 1));\n",
            concat!(
                "func accept(pos value: Maybe)\n",
                "{\n",
                "}\n",
            ),
        ));

        let key = source_function_body_key(&compilation, "main");

        let selections = compilation
            .semantic_selections(key)
            .unwrap_or_else(|error| panic!("argument variant selection must publish: {error:?}"));

        assert!(
            has_union_construction(selections.value()),
            "entries: {:?}; diagnostics: {:?}",
            selections.value().entries(),
            selections.diagnostics()
        );

        assert!(selections.diagnostics().is_empty(), "{selections:?}");
    }

    #[test]
    fn unqualified_variant_without_expected_union_remains_unresolved() {
        let compilation = compilation(&union_source(
            "func main()",
            "    let value = Some(value = 1);\n",
            "",
        ));

        let selections = compilation
            .semantic_selections(source_callable_body_key(&compilation))
            .unwrap_or_else(|error| panic!("unresolved variant must remain checkable: {error:?}"));

        assert!(!has_union_construction(selections.value()));

        assert!(
            diagnostic_kinds(selections.diagnostics())
                .contains(&DiagnosticKind::BindingUnresolvedName),
            "{selections:?}"
        );
    }

    #[test]
    fn unknown_unqualified_variant_reports_the_expected_union_failure() {
        let compilation = compilation(&union_source(
            "func main()",
            "    let value: Maybe = Missing(value = 1);\n",
            "",
        ));

        let selections = compilation
            .semantic_selections(source_callable_body_key(&compilation))
            .unwrap_or_else(|error| panic!("unknown variant must remain checkable: {error:?}"));

        let Some(diagnostic) = selections
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingUnknownUnionVariant)
            .next()
        else {
            panic!("unknown variant must produce a specific diagnostic: {selections:?}");
        };

        assert_eq!(
            diagnostic.args(),
            &[DiagnosticArg::referenced_name("Missing")]
        );
    }

    #[test]
    fn ordinary_callable_lookup_precedes_contextual_variant_lookup() {
        let compilation = compilation(&union_source(
            "func main() -> Maybe",
            "    return Some(value = 1);\n",
            concat!(
                "func Some(value: i32) -> Maybe\n",
                "{\n",
                "    return .Some(value = value);\n",
                "}\n",
            ),
        ));

        let selections = compilation
            .semantic_selections(source_callable_body_key(&compilation))
            .unwrap_or_else(|error| panic!("shadowing callable must select: {error:?}"));

        assert!(selections.value().entries().iter().any(|entry| {
            matches!(entry.selection(), SemanticSelection::Call(_))
        }));

        assert!(!has_union_construction(selections.value()));
        assert!(selections.diagnostics().is_empty(), "{selections:?}");
    }

    #[test]
    fn ordinary_value_lookup_precedes_contextual_variant_lookup() {
        let compilation = compilation(&union_source(
            "func main()",
            concat!(
                "    let None: i32 = 1;\n",
                "    let value: Maybe = None;\n",
            ),
            "",
        ));

        let selections = compilation
            .semantic_selections(source_callable_body_key(&compilation))
            .unwrap_or_else(|error| panic!("shadowing value must remain checkable: {error:?}"));

        assert!(!has_union_construction(selections.value()));

        assert!(
            diagnostic_kinds(selections.diagnostics())
                .contains(&DiagnosticKind::CheckingIncompatibleExpressionType),
            "{selections:?}"
        );
    }

    fn has_union_construction(selections: &bray_bound_tree::CheckedSemanticSelections) -> bool {
        selections.entries().iter().any(|entry| {
            matches!(
                entry.selection(),
                SemanticSelection::Operation(SelectedOperation::Construction(construction))
                    if matches!(construction.target(), ConstructionTarget::UnionVariant(_))
            )
        })
    }

    fn union_source(signature: &str, body: &str, declarations: &str) -> String {
        format!(
            concat!(
                "module app;\n",
                "union Maybe\n",
                "{{\n",
                "    Some(value: i32);\n",
                "    None;\n",
                "}}\n",
                "{}\n",
                "{{\n",
                "{}",
                "}}\n",
                "{}",
            ),
            signature,
            body,
            declarations,
        )
    }
}
