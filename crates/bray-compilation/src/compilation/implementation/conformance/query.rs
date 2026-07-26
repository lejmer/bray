use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_binder::SymbolFactProvider;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticId, DiagnosticKind, DiagnosticResult, SeverityKind,
};
use bray_source::SourceSpan;
use bray_symbols::{
    CallableSignatureFact, ConstantDefinitionState, ImplementationCoherenceFact,
    ImplementationSymbolId, PredicateDefinitionState, SemanticFactResult, SymbolFactRequest,
    TraitConstantMemberDefinitionFact, TraitImplementationConformance,
    TraitImplementationConformanceFact, TraitMemberFulfillmentId, TraitMemberRequirementId,
    TraitPredicateMemberDefinitionFact, TraitRequirementConformance, TraitRequirementResolution,
    TraitTypeFulfillmentValueFact, TypeExpressionTemplate,
};

use super::compatibility::{
    CompatibilityContext, fulfillment_is_compatible, subject_lifecycle_is_compatible,
};
use crate::compilation::{Compilation, binder::CompilationBinderFacts};
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError};

impl Compilation {
    /// Returns fulfillment validity for one trait implementation declaration.
    pub fn trait_implementation_conformance(
        &self,
        implementation: ImplementationSymbolId,
    ) -> Result<Arc<SemanticFactResult<TraitImplementationConformanceFact>>, FactQueryError> {
        self.trait_implementation_conformance_with_cancellation(
            implementation,
            &self.state.cancellation,
        )
    }

    pub(in crate::compilation) fn trait_implementation_conformance_with_cancellation(
        &self,
        implementation: ImplementationSymbolId,
        cancellation: &CancellationToken,
    ) -> Result<Arc<SemanticFactResult<TraitImplementationConformanceFact>>, FactQueryError> {
        if matches!(implementation, ImplementationSymbolId::Inherent(_)) {
            return Err(FactQueryError::InfrastructureFailure);
        }

        let cell = self
            .state
            .trait_implementation_conformance
            .cell(implementation)?;

        let result = cell.get_or_compute(
            &self.state.fact_runtime,
            CompilationFactKey::TraitImplementationConformance(implementation),
            cancellation,
            || {
                self.compute_trait_implementation_conformance(implementation, cancellation)
                    .map(Arc::new)
            },
        )?;

        Ok(Arc::clone(result))
    }

    fn compute_trait_implementation_conformance(
        &self,
        implementation: ImplementationSymbolId,
        cancellation: &CancellationToken,
    ) -> Result<SemanticFactResult<TraitImplementationConformanceFact>, FactQueryError> {
        cancellation.check()?;

        let symbols = self.symbol_graph()?;
        let values = self.semantic_value_store()?;
        let facts = self.binder_facts(cancellation)?;

        let coherence = facts
            .symbol_fact(SymbolFactRequest::<ImplementationCoherenceFact>::new(
                implementation,
            ))
            .map_err(crate::compilation::binder::binder_fact_error)?;

        let imported = self.imported_symbol_skeleton_result_with_cancellation(cancellation)?;

        let mut diagnostics = coherence.diagnostics().merged(imported.diagnostics());
        let imported = imported.value().as_deref();

        let Some(trait_application) = coherence.value().trait_application() else {
            return Err(FactQueryError::InfrastructureFailure);
        };

        let trait_application_data = values
            .trait_application_data(trait_application)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let trait_symbol = symbols
            .trait_symbol(trait_application_data.definition())
            .or_else(|| {
                imported
                    .and_then(|symbols| symbols.trait_symbol(trait_application_data.definition()))
            })
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let mut requirements = trait_requirements(trait_symbol);
        let mut fulfillments = implementation_fulfillments(symbols, implementation)?;

        sort_by_symbol_key(symbols, imported, &mut requirements, |requirement| {
            requirement.symbol()
        })?;

        sort_by_symbol_key(symbols, imported, &mut fulfillments, |fulfillment| {
            fulfillment.symbol()
        })?;

        let mut fulfillments_by_slot = BTreeMap::<_, Vec<_>>::new();

        for fulfillment in &fulfillments {
            let slot = member_slot(
                symbols,
                imported,
                fulfillment.symbol(),
                fulfillment_kind(*fulfillment),
            )?;

            fulfillments_by_slot
                .entry(slot)
                .or_default()
                .push(*fulfillment);
        }

        let type_bindings = type_fulfillment_bindings(
            symbols,
            imported,
            &facts,
            &requirements,
            &fulfillments_by_slot,
            &mut diagnostics,
        )?;

        let subject_lifecycle = subject_lifecycle_fulfillments(
            self,
            values,
            coherence.value().subject(),
            cancellation,
            &mut diagnostics,
        )?;

        let compatibility = CompatibilityContext::new(
            symbols,
            imported,
            values,
            &facts,
            trait_application,
            &type_bindings,
        );

        let mut checked = Vec::with_capacity(requirements.len());
        let mut requirement_slots = BTreeSet::new();
        let mut duplicate_fulfillments = Vec::new();

        for requirement in requirements {
            cancellation.check()?;

            let slot = member_slot(
                symbols,
                imported,
                requirement.symbol(),
                requirement_kind(requirement),
            )?;

            if !matches!(
                requirement,
                TraitMemberRequirementId::Finalizer(_) | TraitMemberRequirementId::Destructor(_)
            ) {
                requirement_slots.insert(slot.clone());
            }

            let matching_fulfillments = fulfillments_by_slot.get(&slot);

            let fulfillment = matching_fulfillments
                .and_then(|matches| matches.first())
                .copied();

            let resolution = if matches!(
                requirement,
                TraitMemberRequirementId::Finalizer(_) | TraitMemberRequirementId::Destructor(_)
            ) {
                match subject_lifecycle.get(&slot).copied() {
                    Some((symbol, fulfillment))
                        if subject_lifecycle_is_compatible(
                            &compatibility,
                            requirement,
                            fulfillment,
                            &mut diagnostics,
                        )? =>
                    {
                        TraitRequirementResolution::SubjectLifecycle(symbol)
                    }
                    Some((symbol, _)) => {
                        diagnostics.add(conformance_diagnostic(
                            symbols,
                            DiagnosticKind::CheckingIncompatibleTraitFulfillment,
                            implementation.into_any(),
                            &slot,
                        )?);

                        TraitRequirementResolution::Incompatible(symbol)
                    }
                    None => {
                        diagnostics.add(conformance_diagnostic(
                            symbols,
                            DiagnosticKind::CheckingMissingTraitFulfillment,
                            implementation.into_any(),
                            &slot,
                        )?);

                        TraitRequirementResolution::Missing
                    }
                }
            } else {
                match fulfillment {
                    Some(fulfillment) => {
                        if fulfillment_is_compatible(
                            &compatibility,
                            requirement,
                            fulfillment,
                            &mut diagnostics,
                        )? {
                            TraitRequirementResolution::Explicit(fulfillment)
                        } else {
                            diagnostics.add(conformance_diagnostic(
                                symbols,
                                DiagnosticKind::CheckingIncompatibleTraitFulfillment,
                                fulfillment.symbol(),
                                &slot,
                            )?);

                            TraitRequirementResolution::Incompatible(fulfillment.symbol())
                        }
                    }
                    None if requirement_has_default(&facts, requirement, &mut diagnostics)? => {
                        TraitRequirementResolution::TraitDefault
                    }
                    None => {
                        diagnostics.add(conformance_diagnostic(
                            symbols,
                            DiagnosticKind::CheckingMissingTraitFulfillment,
                            implementation.into_any(),
                            &slot,
                        )?);

                        TraitRequirementResolution::Missing
                    }
                }
            };

            checked.push(TraitRequirementConformance::new(requirement, resolution));

            if let Some(matches) = matching_fulfillments {
                for duplicate in matches.iter().skip(1).copied() {
                    diagnostics.add(conformance_diagnostic(
                        symbols,
                        DiagnosticKind::CheckingDuplicateTraitFulfillment,
                        duplicate.symbol(),
                        &slot,
                    )?);

                    duplicate_fulfillments.push(duplicate);
                }
            }
        }

        let mut extra_fulfillments = Vec::new();

        for fulfillment in fulfillments {
            let slot = member_slot(
                symbols,
                imported,
                fulfillment.symbol(),
                fulfillment_kind(fulfillment),
            )?;

            if !requirement_slots.contains(&slot) {
                extra_fulfillments.push(fulfillment);
            }
        }

        for fulfillment in &extra_fulfillments {
            let slot = member_slot(
                symbols,
                imported,
                fulfillment.symbol(),
                fulfillment_kind(*fulfillment),
            )?;

            diagnostics.add(conformance_diagnostic(
                symbols,
                DiagnosticKind::CheckingExtraTraitFulfillment,
                fulfillment.symbol(),
                &slot,
            )?);
        }

        let conformance = TraitImplementationConformance::new(
            implementation,
            trait_application,
            checked,
            extra_fulfillments,
            duplicate_fulfillments,
        );

        Ok(DiagnosticResult::new(conformance, diagnostics))
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum MemberKind {
    Callable,
    Constant,
    Type,
    Predicate,
    Constructor,
    CallableOverload,
    Finalizer,
    Destructor,
    ScopeEnter,
    ScopeExit,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum MemberSlot {
    Named(MemberKind, bray_symbols::SymbolName),
    Constructor,
    Finalizer,
    Destructor,
    ScopeEnter,
    ScopeExit,
}

fn trait_requirements(trait_symbol: &bray_symbols::TraitSymbol) -> Vec<TraitMemberRequirementId> {
    trait_symbol
        .callable_members()
        .iter()
        .copied()
        .map(TraitMemberRequirementId::Callable)
        .chain(
            trait_symbol
                .constant_members()
                .iter()
                .copied()
                .map(TraitMemberRequirementId::Constant),
        )
        .chain(
            trait_symbol
                .type_members()
                .iter()
                .copied()
                .map(TraitMemberRequirementId::Type),
        )
        .chain(
            trait_symbol
                .predicate_members()
                .iter()
                .copied()
                .map(TraitMemberRequirementId::Predicate),
        )
        .chain(
            trait_symbol
                .finalizer_requirements()
                .iter()
                .copied()
                .map(TraitMemberRequirementId::Finalizer),
        )
        .chain(
            trait_symbol
                .destructor_requirements()
                .iter()
                .copied()
                .map(TraitMemberRequirementId::Destructor),
        )
        .chain(
            trait_symbol
                .scope_enter_requirements()
                .iter()
                .copied()
                .map(TraitMemberRequirementId::ScopeEnter),
        )
        .chain(
            trait_symbol
                .scope_exit_requirements()
                .iter()
                .copied()
                .map(TraitMemberRequirementId::ScopeExit),
        )
        .collect()
}

fn implementation_fulfillments(
    symbols: &bray_symbols::SymbolGraph,
    implementation: ImplementationSymbolId,
) -> Result<Vec<TraitMemberFulfillmentId>, FactQueryError> {
    macro_rules! collect {
        ($implementation:expr) => {{
            let implementation = $implementation.ok_or(FactQueryError::InfrastructureFailure)?;

            implementation
                .callable_fulfillments()
                .iter()
                .copied()
                .map(TraitMemberFulfillmentId::Callable)
                .chain(
                    implementation
                        .constant_fulfillments()
                        .iter()
                        .copied()
                        .map(TraitMemberFulfillmentId::Constant),
                )
                .chain(
                    implementation
                        .type_fulfillments()
                        .iter()
                        .copied()
                        .map(TraitMemberFulfillmentId::Type),
                )
                .chain(
                    implementation
                        .predicate_fulfillments()
                        .iter()
                        .copied()
                        .map(TraitMemberFulfillmentId::Predicate),
                )
                .chain(
                    implementation
                        .constructors()
                        .iter()
                        .copied()
                        .map(TraitMemberFulfillmentId::Constructor),
                )
                .chain(
                    implementation
                        .callable_overloads()
                        .iter()
                        .copied()
                        .map(TraitMemberFulfillmentId::CallableOverload),
                )
                .chain(
                    implementation
                        .finalizers()
                        .iter()
                        .copied()
                        .map(TraitMemberFulfillmentId::Finalizer),
                )
                .chain(
                    implementation
                        .destructors()
                        .iter()
                        .copied()
                        .map(TraitMemberFulfillmentId::Destructor),
                )
                .chain(
                    implementation
                        .scope_enter_fulfillments()
                        .iter()
                        .copied()
                        .map(TraitMemberFulfillmentId::ScopeEnter),
                )
                .chain(
                    implementation
                        .scope_exit_fulfillments()
                        .iter()
                        .copied()
                        .map(TraitMemberFulfillmentId::ScopeExit),
                )
                .collect()
        }};
    }

    Ok(match implementation {
        ImplementationSymbolId::UnnamedTrait(id) => {
            collect!(symbols.unnamed_trait_implementation(id))
        }
        ImplementationSymbolId::NamedTrait(id) => {
            collect!(symbols.named_trait_implementation(id))
        }
        ImplementationSymbolId::Inherent(_) => {
            return Err(FactQueryError::InfrastructureFailure);
        }
    })
}

fn sort_by_symbol_key<T>(
    symbols: &bray_symbols::SymbolGraph,
    imported: Option<&bray_symbols::ImportedSymbolSkeleton>,
    values: &mut [T],
    symbol: impl Fn(&T) -> bray_symbols::AnySymbolId,
) -> Result<(), FactQueryError> {
    let mut missing_key = false;

    values.sort_by(|left, right| {
        let left = symbols
            .symbol_key(symbol(left))
            .or_else(|| imported.and_then(|symbols| symbols.symbol_key(symbol(left))));

        let right = symbols
            .symbol_key(symbol(right))
            .or_else(|| imported.and_then(|symbols| symbols.symbol_key(symbol(right))));

        match (left, right) {
            (Some(left), Some(right)) => left.cmp(right),
            _ => {
                missing_key = true;
                std::cmp::Ordering::Equal
            }
        }
    });

    if missing_key {
        return Err(FactQueryError::InfrastructureFailure);
    }

    Ok(())
}

fn member_slot(
    symbols: &bray_symbols::SymbolGraph,
    imported: Option<&bray_symbols::ImportedSymbolSkeleton>,
    symbol: bray_symbols::AnySymbolId,
    kind: MemberKind,
) -> Result<MemberSlot, FactQueryError> {
    match kind {
        MemberKind::Constructor => return Ok(MemberSlot::Constructor),
        MemberKind::Finalizer => return Ok(MemberSlot::Finalizer),
        MemberKind::Destructor => return Ok(MemberSlot::Destructor),
        MemberKind::ScopeEnter => return Ok(MemberSlot::ScopeEnter),
        MemberKind::ScopeExit => return Ok(MemberSlot::ScopeExit),
        MemberKind::Callable
        | MemberKind::Constant
        | MemberKind::Type
        | MemberKind::Predicate
        | MemberKind::CallableOverload => {}
    }

    let name = symbols
        .member_name(symbol)
        .or_else(|| imported.and_then(|symbols| symbols.member_name(symbol)))
        .cloned()
        .ok_or(FactQueryError::InfrastructureFailure)?;

    Ok(MemberSlot::Named(kind, name))
}

const fn requirement_kind(requirement: TraitMemberRequirementId) -> MemberKind {
    match requirement {
        TraitMemberRequirementId::Callable(_) => MemberKind::Callable,
        TraitMemberRequirementId::Constant(_) => MemberKind::Constant,
        TraitMemberRequirementId::Type(_) => MemberKind::Type,
        TraitMemberRequirementId::Predicate(_) => MemberKind::Predicate,
        TraitMemberRequirementId::Finalizer(_) => MemberKind::Finalizer,
        TraitMemberRequirementId::Destructor(_) => MemberKind::Destructor,
        TraitMemberRequirementId::ScopeEnter(_) => MemberKind::ScopeEnter,
        TraitMemberRequirementId::ScopeExit(_) => MemberKind::ScopeExit,
    }
}

const fn fulfillment_kind(fulfillment: TraitMemberFulfillmentId) -> MemberKind {
    match fulfillment {
        TraitMemberFulfillmentId::Callable(_) => MemberKind::Callable,
        TraitMemberFulfillmentId::Constant(_) => MemberKind::Constant,
        TraitMemberFulfillmentId::Type(_) => MemberKind::Type,
        TraitMemberFulfillmentId::Predicate(_) => MemberKind::Predicate,
        TraitMemberFulfillmentId::Constructor(_) => MemberKind::Constructor,
        TraitMemberFulfillmentId::CallableOverload(_) => MemberKind::CallableOverload,
        TraitMemberFulfillmentId::Finalizer(_) => MemberKind::Finalizer,
        TraitMemberFulfillmentId::Destructor(_) => MemberKind::Destructor,
        TraitMemberFulfillmentId::ScopeEnter(_) => MemberKind::ScopeEnter,
        TraitMemberFulfillmentId::ScopeExit(_) => MemberKind::ScopeExit,
    }
}

fn type_fulfillment_bindings(
    symbols: &bray_symbols::SymbolGraph,
    imported: Option<&bray_symbols::ImportedSymbolSkeleton>,
    facts: &CompilationBinderFacts<'_>,
    requirements: &[TraitMemberRequirementId],
    fulfillments: &BTreeMap<MemberSlot, Vec<TraitMemberFulfillmentId>>,
    diagnostics: &mut bray_diagnostics::DiagnosticBag,
) -> Result<BTreeMap<bray_symbols::TraitTypeMemberSymbolId, TypeExpressionTemplate>, FactQueryError>
{
    let mut bindings = BTreeMap::new();

    for requirement in requirements {
        let TraitMemberRequirementId::Type(member) = requirement else {
            continue;
        };

        let slot = member_slot(symbols, imported, requirement.symbol(), MemberKind::Type)?;

        let Some(TraitMemberFulfillmentId::Type(fulfillment)) =
            fulfillments.get(&slot).and_then(|matches| matches.first())
        else {
            continue;
        };

        let value = facts
            .symbol_fact(SymbolFactRequest::<TraitTypeFulfillmentValueFact>::new(
                *fulfillment,
            ))
            .map_err(crate::compilation::binder::binder_fact_error)?;

        *diagnostics = diagnostics.merged(value.diagnostics());

        bindings.insert(*member, value.value().clone());
    }

    Ok(bindings)
}

fn subject_lifecycle_fulfillments(
    compilation: &Compilation,
    values: &bray_symbols::SemanticValueStore,
    subject: bray_symbols::TypeId,
    cancellation: &CancellationToken,
    diagnostics: &mut bray_diagnostics::DiagnosticBag,
) -> Result<
    BTreeMap<MemberSlot, (bray_symbols::AnySymbolId, bray_symbols::CallableSymbolId)>,
    FactQueryError,
> {
    let subject = values
        .type_data(subject)
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    let bray_symbols::TypeData::Named { definition, .. } = subject.as_ref() else {
        return Ok(BTreeMap::new());
    };

    let surface =
        compilation.type_associated_surface_result_with_cancellation(*definition, cancellation)?;

    *diagnostics = diagnostics.merged(surface.diagnostics());

    let mut fulfillments = BTreeMap::new();

    for member in surface.value().lifecycle_members() {
        let slot = match member.slot() {
            bray_symbols::TypeAssociatedLifecycleSlot::Finalizer => MemberSlot::Finalizer,
            bray_symbols::TypeAssociatedLifecycleSlot::Destructor => MemberSlot::Destructor,
            bray_symbols::TypeAssociatedLifecycleSlot::PrimaryConstructor
            | bray_symbols::TypeAssociatedLifecycleSlot::ScopeEnter
            | bray_symbols::TypeAssociatedLifecycleSlot::ScopeExit => continue,
        };

        let callable = bray_symbols::CallableSymbolId::try_from_any(member.id())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        fulfillments.insert(slot, (member.id(), callable));
    }

    Ok(fulfillments)
}

fn requirement_has_default(
    facts: &CompilationBinderFacts<'_>,
    requirement: TraitMemberRequirementId,
    diagnostics: &mut bray_diagnostics::DiagnosticBag,
) -> Result<bool, FactQueryError> {
    match requirement {
        TraitMemberRequirementId::Callable(requirement) => {
            let signature = facts
                .symbol_fact(SymbolFactRequest::<CallableSignatureFact>::new(
                    requirement.into(),
                ))
                .map_err(crate::compilation::binder::binder_fact_error)?;

            *diagnostics = diagnostics.merged(signature.diagnostics());

            Ok(signature.value().has_body())
        }
        TraitMemberRequirementId::Constant(requirement) => {
            let definition = facts
                .symbol_fact(SymbolFactRequest::<TraitConstantMemberDefinitionFact>::new(
                    requirement,
                ))
                .map_err(crate::compilation::binder::binder_fact_error)?;

            *diagnostics = diagnostics.merged(definition.diagnostics());

            Ok(matches!(
                definition.value(),
                ConstantDefinitionState::Defined(_)
            ))
        }
        TraitMemberRequirementId::Predicate(requirement) => {
            let definition = facts
                .symbol_fact(
                    SymbolFactRequest::<TraitPredicateMemberDefinitionFact>::new(requirement),
                )
                .map_err(crate::compilation::binder::binder_fact_error)?;

            *diagnostics = diagnostics.merged(definition.diagnostics());

            Ok(matches!(
                definition.value(),
                PredicateDefinitionState::Defined(_)
            ))
        }
        TraitMemberRequirementId::Type(_)
        | TraitMemberRequirementId::Finalizer(_)
        | TraitMemberRequirementId::Destructor(_)
        | TraitMemberRequirementId::ScopeEnter(_)
        | TraitMemberRequirementId::ScopeExit(_) => Ok(false),
    }
}

fn conformance_diagnostic(
    symbols: &bray_symbols::SymbolGraph,
    kind: DiagnosticKind,
    span_symbol: bray_symbols::AnySymbolId,
    slot: &MemberSlot,
) -> Result<Diagnostic, FactQueryError> {
    let anchor = symbols
        .declaration_syntax_anchor(span_symbol)
        .ok_or(FactQueryError::InfrastructureFailure)?;

    let member = match slot {
        MemberSlot::Named(_, name) => DiagnosticArg::trait_member_name(name.as_str()),
        MemberSlot::Constructor => {
            DiagnosticArg::trait_member_kind(bray_syntax::SyntaxKind::ConstructKeyword)
        }
        MemberSlot::Finalizer => {
            DiagnosticArg::trait_member_kind(bray_syntax::SyntaxKind::FinalizeKeyword)
        }
        MemberSlot::Destructor => {
            DiagnosticArg::trait_member_kind(bray_syntax::SyntaxKind::DestructKeyword)
        }
        MemberSlot::ScopeEnter => {
            DiagnosticArg::trait_member_kind(bray_syntax::SyntaxKind::EnterKeyword)
        }
        MemberSlot::ScopeExit => {
            DiagnosticArg::trait_member_kind(bray_syntax::SyntaxKind::ExitKeyword)
        }
    };

    Ok(
        Diagnostic::new(DiagnosticId::new(0), kind, SeverityKind::Error)
            .with_primary_span(SourceSpan::new(anchor.source_id(), anchor.full_range()))
            .with_arg(member),
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_diagnostics::DiagnosticKind;
    use bray_symbols::{ImplementationSymbolId, SymbolOrigin, TraitRequirementResolution};

    use crate::fact::CompilationFactKey;
    use crate::test_support::compilation;

    #[test]
    fn complete_trait_implementations_publish_exact_fulfillment_links_once() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "struct Holder\n",
            "{\n",
            "}\n",
            "\n",
            "trait Provides\n",
            "{\n",
            "    const enabled: bool;\n",
            "    type Item;\n",
            "    func get() -> bool;\n",
            "    predicate valid(value: bool);\n",
            "    consume enter() -> bool;\n",
            "    exit(pos lease: bool);\n",
            "}\n",
            "\n",
            "impl Holder(Provides)\n",
            "{\n",
            "    const enabled: bool = true;\n",
            "    type Item = bool;\n",
            "    predicate valid(value: bool) = value;\n",
            "\n",
            "    func get() -> bool\n",
            "    {\n",
            "        return true;\n",
            "    }\n",
            "\n",
            "    consume enter() -> bool\n",
            "    {\n",
            "        return true;\n",
            "    }\n",
            "\n",
            "    exit(pos lease: bool)\n",
            "    {\n",
            "    }\n",
            "}\n",
        ));

        let implementation = source_implementation(&compilation);

        assert_eq!(
            compilation
                .state
                .trait_implementation_conformance
                .is_published(&implementation),
            Ok(false)
        );

        let first = compilation
            .trait_implementation_conformance(implementation)
            .unwrap_or_else(|error| panic!("conformance must publish: {error:?}"));

        let second = compilation
            .trait_implementation_conformance(implementation)
            .unwrap_or_else(|error| panic!("conformance must remain available: {error:?}"));

        assert!(Arc::ptr_eq(&first, &second));
        assert!(first.diagnostics().is_empty(), "{:?}", first.diagnostics());
        assert!(first.value().is_valid());

        assert!(
            first
                .value()
                .requirements()
                .iter()
                .all(|entry| matches!(entry.resolution(), TraitRequirementResolution::Explicit(_)))
        );

        let dependencies = compilation
            .state
            .fact_runtime
            .dependencies(&CompilationFactKey::TraitImplementationConformance(
                implementation,
            ))
            .unwrap_or_else(|error| panic!("dependencies must be readable: {error:?}"))
            .unwrap_or_else(|| panic!("conformance must be published"));

        assert!(dependencies.contains(&CompilationFactKey::SymbolGraph));
    }

    #[test]
    fn missing_extra_and_incompatible_fulfillments_are_distinct() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "struct Holder\n",
            "{\n",
            "}\n",
            "\n",
            "trait Provides\n",
            "{\n",
            "    const enabled: bool;\n",
            "    type Item;\n",
            "    func get() -> bool;\n",
            "}\n",
            "\n",
            "impl Holder(Provides)\n",
            "{\n",
            "    const other: bool = true;\n",
            "\n",
            "    func get() -> i32\n",
            "    {\n",
            "        return 0;\n",
            "    }\n",
            "\n",
            "    overload get = {get}\n",
            "}\n",
        ));

        let result = compilation
            .trait_implementation_conformance(source_implementation(&compilation))
            .unwrap_or_else(|error| panic!("conformance must publish: {error:?}"));

        let kinds = result
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.kind())
            .collect::<Vec<_>>();

        assert_eq!(
            kinds,
            [
                DiagnosticKind::CheckingIncompatibleTraitFulfillment,
                DiagnosticKind::CheckingMissingTraitFulfillment,
                DiagnosticKind::CheckingMissingTraitFulfillment,
                DiagnosticKind::CheckingExtraTraitFulfillment,
                DiagnosticKind::CheckingExtraTraitFulfillment,
            ]
        );

        assert!(!result.value().is_valid());
    }

    #[test]
    fn omitted_callable_and_constant_defaults_remain_trait_owned() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "struct Holder\n",
            "{\n",
            "}\n",
            "\n",
            "trait Provides\n",
            "{\n",
            "    const enabled: bool = true;\n",
            "\n",
            "    func get() -> bool\n",
            "    {\n",
            "        return true;\n",
            "    }\n",
            "}\n",
            "\n",
            "impl Holder(Provides)\n",
            "{\n",
            "}\n",
        ));

        let result = compilation
            .trait_implementation_conformance(source_implementation(&compilation))
            .unwrap_or_else(|error| panic!("conformance must publish: {error:?}"));

        assert!(result.diagnostics().is_empty());
        assert!(result.value().is_valid());

        assert!(
            result
                .value()
                .requirements()
                .iter()
                .all(|entry| entry.resolution() == TraitRequirementResolution::TraitDefault)
        );
    }

    #[test]
    fn generic_parameter_categories_must_match() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "struct Holder\n",
            "{\n",
            "}\n",
            "\n",
            "trait Provides\n",
            "{\n",
            "    func get<T>() -> bool;\n",
            "}\n",
            "\n",
            "impl Holder(Provides)\n",
            "{\n",
            "    func get<const N: usize>() -> bool\n",
            "    {\n",
            "        return true;\n",
            "    }\n",
            "}\n",
        ));

        let result = compilation
            .trait_implementation_conformance(source_implementation(&compilation))
            .unwrap_or_else(|error| panic!("conformance must publish: {error:?}"));

        assert_eq!(
            result
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.kind())
                .collect::<Vec<_>>(),
            [DiagnosticKind::CheckingIncompatibleTraitFulfillment]
        );
    }

    #[test]
    fn generic_parameter_names_do_not_change_fulfillment_identity() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "struct Holder\n",
            "{\n",
            "}\n",
            "\n",
            "trait Provides\n",
            "{\n",
            "    func identity<T>(pos value: T) -> T;\n",
            "}\n",
            "\n",
            "impl Holder(Provides)\n",
            "{\n",
            "    func identity<U>(pos value: U) -> U\n",
            "    {\n",
            "        loop\n",
            "        {\n",
            "        }\n",
            "    }\n",
            "}\n",
        ));

        let result = compilation
            .trait_implementation_conformance(source_implementation(&compilation))
            .unwrap_or_else(|error| panic!("conformance must publish: {error:?}"));

        assert!(
            result.diagnostics().is_empty(),
            "{:?}",
            result.diagnostics()
        );

        assert!(result.value().is_valid());
    }

    #[test]
    fn duplicate_fulfillments_are_not_reported_as_unknown_trait_members() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "struct Holder\n",
            "{\n",
            "}\n",
            "\n",
            "trait Provides\n",
            "{\n",
            "    func get() -> bool;\n",
            "}\n",
            "\n",
            "impl Holder(Provides)\n",
            "{\n",
            "    func get() -> bool\n",
            "    {\n",
            "        return true;\n",
            "    }\n",
            "\n",
            "    func get() -> bool\n",
            "    {\n",
            "        return false;\n",
            "    }\n",
            "}\n",
        ));

        let result = compilation
            .trait_implementation_conformance(source_implementation(&compilation))
            .unwrap_or_else(|error| panic!("conformance must publish: {error:?}"));

        assert_eq!(
            result
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.kind())
                .collect::<Vec<_>>(),
            [DiagnosticKind::CheckingDuplicateTraitFulfillment]
        );

        assert!(result.value().extra_fulfillments().is_empty());
        assert_eq!(result.value().duplicate_fulfillments().len(), 1);
        assert!(!result.value().is_valid());
    }

    #[test]
    fn subject_lifecycle_declarations_satisfy_finalizer_and_destructor_requirements() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "struct Holder\n",
            "{\n",
            "    async finalize()\n",
            "    {\n",
            "    }\n",
            "\n",
            "    destruct()\n",
            "    {\n",
            "    }\n",
            "}\n",
            "\n",
            "trait Provides\n",
            "{\n",
            "    async finalize();\n",
            "    destruct();\n",
            "}\n",
            "\n",
            "impl Holder(Provides)\n",
            "{\n",
            "}\n",
        ));

        let result = compilation
            .trait_implementation_conformance(source_implementation(&compilation))
            .unwrap_or_else(|error| panic!("conformance must publish: {error:?}"));

        assert!(
            result.diagnostics().is_empty(),
            "{:?}",
            result.diagnostics()
        );

        assert!(result.value().is_valid());

        assert!(result.value().requirements().iter().all(|entry| matches!(
            entry.resolution(),
            TraitRequirementResolution::SubjectLifecycle(_)
        )));
    }

    fn source_implementation(compilation: &crate::Compilation) -> ImplementationSymbolId {
        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("symbol graph must be available: {error:?}"));

        symbols
            .unnamed_trait_implementations()
            .iter()
            .find(|symbol| symbol.origin() == SymbolOrigin::Source)
            .map(|symbol| ImplementationSymbolId::from(symbol.id()))
            .unwrap_or_else(|| panic!("test source must declare one trait implementation"))
    }
}
