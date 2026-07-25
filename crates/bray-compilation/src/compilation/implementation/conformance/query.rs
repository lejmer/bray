use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_binder::SymbolFactProvider;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticId, DiagnosticKind, DiagnosticResult, SeverityKind,
};
use bray_source::SourceSpan;
use bray_symbols::{
    ImplementationCoherenceFact, ImplementationSymbolId, SemanticFactResult, SymbolFactRequest,
    TraitImplementationConformance, TraitImplementationConformanceFact, TraitMemberFulfillmentId,
    TraitMemberRequirementId, TraitRequirementConformance, TraitRequirementResolution,
    TraitTypeFulfillmentValueFact, TypeExpressionTemplate,
};
use bray_syntax::{TraitCallableMemberDeclarationSyntax, TraitConstantMemberDeclarationSyntax};

use super::compatibility::fulfillment_is_compatible;
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

        let Some(trait_application) = coherence.value().trait_application() else {
            return Err(FactQueryError::InfrastructureFailure);
        };

        let trait_application_data = values
            .trait_application_data(trait_application)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let trait_symbol = symbols
            .trait_symbol(trait_application_data.definition())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let mut requirements = trait_requirements(trait_symbol);
        let mut fulfillments = implementation_fulfillments(symbols, implementation)?;

        sort_by_symbol_key(symbols, &mut requirements, |requirement| {
            requirement.symbol()
        })?;

        sort_by_symbol_key(symbols, &mut fulfillments, |fulfillment| {
            fulfillment.symbol()
        })?;

        let mut fulfillments_by_slot = BTreeMap::<_, Vec<_>>::new();

        for fulfillment in &fulfillments {
            let slot = member_slot(
                symbols,
                fulfillment.symbol(),
                fulfillment_kind(*fulfillment),
            )?;

            fulfillments_by_slot
                .entry(slot)
                .or_default()
                .push(*fulfillment);
        }

        let type_bindings =
            type_fulfillment_bindings(symbols, &facts, &requirements, &fulfillments_by_slot)?;

        let mut used = BTreeSet::new();
        let mut checked = Vec::with_capacity(requirements.len());
        let mut diagnostics = coherence.diagnostics().clone();

        for requirement in requirements {
            cancellation.check()?;

            let slot = member_slot(symbols, requirement.symbol(), requirement_kind(requirement))?;

            let fulfillment = fulfillments_by_slot
                .get(&slot)
                .and_then(|matches| matches.first())
                .copied();

            let resolution = match fulfillment {
                Some(fulfillment) => {
                    used.insert(fulfillment);

                    if fulfillment_is_compatible(
                        symbols,
                        values,
                        &facts,
                        trait_application,
                        requirement,
                        fulfillment,
                        &type_bindings,
                    )? {
                        TraitRequirementResolution::Explicit(fulfillment)
                    } else {
                        diagnostics.add(conformance_diagnostic(
                            symbols,
                            DiagnosticKind::CheckingIncompatibleTraitFulfillment,
                            fulfillment.symbol(),
                            &slot,
                        )?);

                        TraitRequirementResolution::Incompatible(fulfillment)
                    }
                }
                None if requirement_has_default(self, symbols, requirement)? => {
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
            };

            checked.push(TraitRequirementConformance::new(requirement, resolution));
        }

        let extra_fulfillments = fulfillments
            .into_iter()
            .filter(|fulfillment| !used.contains(fulfillment))
            .collect::<Vec<_>>();

        for fulfillment in &extra_fulfillments {
            let slot = member_slot(
                symbols,
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
    ScopeEnter,
    ScopeExit,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum MemberSlot {
    Named(MemberKind, bray_symbols::SymbolName),
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
    values: &mut [T],
    symbol: impl Fn(&T) -> bray_symbols::AnySymbolId,
) -> Result<(), FactQueryError> {
    let mut missing_key = false;

    values.sort_by(|left, right| {
        let left = symbols.symbol_key(symbol(left));
        let right = symbols.symbol_key(symbol(right));

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
    symbol: bray_symbols::AnySymbolId,
    kind: MemberKind,
) -> Result<MemberSlot, FactQueryError> {
    match kind {
        MemberKind::ScopeEnter => return Ok(MemberSlot::ScopeEnter),
        MemberKind::ScopeExit => return Ok(MemberSlot::ScopeExit),
        MemberKind::Callable | MemberKind::Constant | MemberKind::Type | MemberKind::Predicate => {}
    }

    let name = symbols
        .member_name(symbol)
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
        TraitMemberFulfillmentId::ScopeEnter(_) => MemberKind::ScopeEnter,
        TraitMemberFulfillmentId::ScopeExit(_) => MemberKind::ScopeExit,
    }
}

fn type_fulfillment_bindings(
    symbols: &bray_symbols::SymbolGraph,
    facts: &CompilationBinderFacts<'_>,
    requirements: &[TraitMemberRequirementId],
    fulfillments: &BTreeMap<MemberSlot, Vec<TraitMemberFulfillmentId>>,
) -> Result<BTreeMap<bray_symbols::TraitTypeMemberSymbolId, TypeExpressionTemplate>, FactQueryError>
{
    let mut bindings = BTreeMap::new();

    for requirement in requirements {
        let TraitMemberRequirementId::Type(member) = requirement else {
            continue;
        };

        let slot = member_slot(symbols, requirement.symbol(), MemberKind::Type)?;

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

        bindings.insert(*member, value.value().clone());
    }

    Ok(bindings)
}

fn requirement_has_default(
    compilation: &Compilation,
    symbols: &bray_symbols::SymbolGraph,
    requirement: TraitMemberRequirementId,
) -> Result<bool, FactQueryError> {
    let Some(anchor) = symbols.declaration_syntax_anchor(requirement.symbol()) else {
        return Ok(false);
    };

    match requirement {
        TraitMemberRequirementId::Callable(_) => Ok(anchor
            .find_descendant::<TraitCallableMemberDeclarationSyntax>(compilation.syntax_tree())
            .and_then(|declaration| declaration.callable_body_block_expression())
            .is_some()),
        TraitMemberRequirementId::Constant(_) => Ok(anchor
            .find_descendant::<TraitConstantMemberDeclarationSyntax>(compilation.syntax_tree())
            .and_then(|declaration| declaration.expression())
            .is_some()),
        TraitMemberRequirementId::Type(_)
        | TraitMemberRequirementId::Predicate(_)
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
