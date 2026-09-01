// rust-style: allow(module-too-large, reason = "trait implementation conformance is one lazy query pipeline with exhaustive source-correlated failure conversion")

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_binder::SymbolQueryProvider;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticCallableBehaviorComponent,
    DiagnosticCallableBehaviorPhase, DiagnosticCallableConstness,
    DiagnosticCallableContractClauseCategory, DiagnosticCallableContractMismatch,
    DiagnosticCallableContractSurface, DiagnosticCallableParameterMode, DiagnosticCallablePosition,
    DiagnosticCallableTrust, DiagnosticConstraintCategory, DiagnosticGenericConstraintMismatch,
    DiagnosticGenericParameterCategory, DiagnosticId, DiagnosticKind, DiagnosticLabel,
    DiagnosticLabelKind, DiagnosticReceiverMode, DiagnosticRelatedLocation,
    DiagnosticRelatedLocationKind, DiagnosticResult, DiagnosticTraitFulfillmentMismatch,
    SeverityKind,
};
use bray_source::SourceSpan;
use bray_symbols::{
    CallableSignatureQuery, ConstantDefinitionState, ImplementationCoherenceQuery,
    ImplementationSymbolId, PredicateDefinitionState, SymbolQueryRequest,
    TraitConstantMemberDefinitionQuery, TraitImplementationConformance,
    TraitImplementationConformanceQuery, TraitMemberFulfillmentId, TraitMemberRequirementId,
    TraitPredicateMemberDefinitionQuery, TraitRequirementConformance, TraitRequirementResolution,
    TraitTypeFulfillmentValueQuery, TypeExpressionTemplate, diagnostic_callable_abi,
    diagnostic_callable_execution,
};

use super::compatibility::{
    CallableBehaviorComponent, CallableBehaviorPhase, CallableContractClauseCategory,
    CallableContractMismatch, CallableContractSurface, CompatibilityContext, ConstraintCategory,
    GenericConstraintMismatch, GenericParameterCategory, GenericSurfaceMismatch,
    TraitFulfillmentMismatch, fulfillment_is_compatible, subject_lifecycle_is_compatible,
};
use crate::compilation::{Compilation, binder::CompilationBindingContext};
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError};

impl Compilation {
    /// Returns fulfillment validity for one trait implementation declaration.
    pub fn trait_implementation_conformance(
        &self,
        implementation: ImplementationSymbolId,
    ) -> Result<
        Arc<
            bray_diagnostics::DiagnosticResult<
                <TraitImplementationConformanceQuery as bray_symbols::SemanticQueryContract>::Value,
            >,
        >,
        FactQueryError,
    > {
        self.trait_implementation_conformance_with_cancellation(
            implementation,
            &self.state.cancellation,
        )
    }

    pub(in crate::compilation) fn trait_implementation_conformance_with_cancellation(
        &self,
        implementation: ImplementationSymbolId,
        cancellation: &CancellationToken,
    ) -> Result<
        Arc<
            bray_diagnostics::DiagnosticResult<
                <TraitImplementationConformanceQuery as bray_symbols::SemanticQueryContract>::Value,
            >,
        >,
        FactQueryError,
    > {
        if matches!(implementation, ImplementationSymbolId::Inherent(_)) {
            let symbol = implementation.into_any();

            return Err(crate::compilation::SemanticQueryFailure::contract(
                crate::compilation::SemanticQueryContext::Symbol(symbol),
                crate::compilation::SemanticQueryViolation::UnexpectedSymbolKind {
                    expected: crate::compilation::SemanticSymbolCategory::TraitImplementation,
                    actual: symbol.kind(),
                },
            )
            .into());
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

    // rust-style: allow(function-too-large, reason = "conformance evaluates and publishes every trait requirement in one deterministic transaction")
    fn compute_trait_implementation_conformance(
        &self,
        implementation: ImplementationSymbolId,
        cancellation: &CancellationToken,
    ) -> Result<
        bray_diagnostics::DiagnosticResult<
            <TraitImplementationConformanceQuery as bray_symbols::SemanticQueryContract>::Value,
        >,
        FactQueryError,
    > {
        cancellation.check()?;

        let symbols = self.symbol_graph()?;
        let values = self.semantic_value_store()?;
        let binding_context = self.binding_context(cancellation)?;

        let coherence = binding_context
            .resolve_symbol_query(SymbolQueryRequest::<ImplementationCoherenceQuery>::new(
                implementation,
            ))
            .map_err(crate::compilation::binder::binding_query_error)?;

        let imported = self.imported_symbol_skeleton_result_with_cancellation(cancellation)?;

        let mut diagnostics = coherence.diagnostics().merged(imported.diagnostics());
        let imported = imported.value().as_deref();

        let Some(trait_application) = coherence.value().trait_application() else {
            return Err(crate::compilation::SemanticQueryFailure::contract(
                crate::compilation::SemanticQueryContext::Symbol(implementation.into_any()),
                crate::compilation::SemanticQueryViolation::Missing(
                    crate::compilation::SemanticDataKind::TraitApplication,
                ),
            )
            .into());
        };

        let trait_application_data = values
            .trait_application_data(trait_application)
            .map_err(FactQueryError::SemanticValueStore)?;

        let trait_symbol = symbols
            .trait_symbol(trait_application_data.definition())
            .or_else(|| {
                imported
                    .and_then(|symbols| symbols.trait_symbol(trait_application_data.definition()))
            })
            .ok_or_else(|| {
                crate::compilation::SemanticQueryFailure::contract(
                    crate::compilation::SemanticQueryContext::Symbol(
                        trait_application_data.definition().into(),
                    ),
                    crate::compilation::SemanticQueryViolation::Missing(
                        crate::compilation::SemanticDataKind::Symbol,
                    ),
                )
            })?;

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
            &binding_context,
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
            &binding_context,
            coherence.value().subject(),
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
                    Some((symbol, fulfillment)) => match subject_lifecycle_is_compatible(
                        &compatibility,
                        requirement,
                        fulfillment,
                        &mut diagnostics,
                    )? {
                        None => TraitRequirementResolution::SubjectLifecycle(symbol),
                        Some(mismatch) => {
                            diagnostics.add(conformance_diagnostic(
                                symbols,
                                DiagnosticKind::CheckingIncompatibleTraitFulfillment,
                                implementation.into_any(),
                                &slot,
                                Some(self.diagnostic_trait_mismatch(
                                    implementation,
                                    mismatch,
                                    cancellation,
                                )?),
                                [(
                                    DiagnosticRelatedLocationKind::RequirementOrigin,
                                    requirement.symbol(),
                                )],
                            )?);

                            TraitRequirementResolution::Incompatible(symbol)
                        }
                    },
                    None => {
                        diagnostics.add(conformance_diagnostic(
                            symbols,
                            DiagnosticKind::CheckingMissingTraitFulfillment,
                            implementation.into_any(),
                            &slot,
                            None,
                            [(
                                DiagnosticRelatedLocationKind::RequirementOrigin,
                                requirement.symbol(),
                            )],
                        )?);

                        TraitRequirementResolution::Missing
                    }
                }
            } else {
                match fulfillment {
                    Some(fulfillment) => {
                        match fulfillment_is_compatible(
                            &compatibility,
                            requirement,
                            fulfillment,
                            &mut diagnostics,
                        )? {
                            None => TraitRequirementResolution::Explicit(fulfillment),
                            Some(mismatch) => {
                                diagnostics.add(conformance_diagnostic(
                                    symbols,
                                    DiagnosticKind::CheckingIncompatibleTraitFulfillment,
                                    fulfillment.symbol(),
                                    &slot,
                                    Some(self.diagnostic_trait_mismatch(
                                        implementation,
                                        mismatch,
                                        cancellation,
                                    )?),
                                    [(
                                        DiagnosticRelatedLocationKind::RequirementOrigin,
                                        requirement.symbol(),
                                    )],
                                )?);

                                TraitRequirementResolution::Incompatible(fulfillment.symbol())
                            }
                        }
                    }
                    None if requirement_has_default(
                        &binding_context,
                        requirement,
                        &mut diagnostics,
                    )? =>
                    {
                        TraitRequirementResolution::TraitDefault
                    }
                    None => {
                        diagnostics.add(conformance_diagnostic(
                            symbols,
                            DiagnosticKind::CheckingMissingTraitFulfillment,
                            implementation.into_any(),
                            &slot,
                            None,
                            [(
                                DiagnosticRelatedLocationKind::RequirementOrigin,
                                requirement.symbol(),
                            )],
                        )?);

                        TraitRequirementResolution::Missing
                    }
                }
            };

            checked.push(TraitRequirementConformance::new(requirement, resolution));

            if let Some(matches) = matching_fulfillments {
                for (index, duplicate) in matches.iter().copied().enumerate().skip(1) {
                    diagnostics.add(conformance_diagnostic(
                        symbols,
                        DiagnosticKind::CheckingDuplicateTraitFulfillment,
                        duplicate.symbol(),
                        &slot,
                        None,
                        matches[..index].iter().copied().map(|previous| {
                            (
                                DiagnosticRelatedLocationKind::FirstDeclaration,
                                previous.symbol(),
                            )
                        }),
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
                None,
                [],
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

    fn diagnostic_trait_mismatch(
        &self,
        implementation: ImplementationSymbolId,
        mismatch: TraitFulfillmentMismatch,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticTraitFulfillmentMismatch, FactQueryError> {
        let diagnostic_count = |value| diagnostic_count(implementation, value);

        let type_value = |template: &TypeExpressionTemplate| {
            crate::compilation::foreign::diagnostic::template_diagnostic_type(
                self,
                template,
                cancellation,
            )
        };

        let mismatch = match mismatch {
            TraitFulfillmentMismatch::MemberCategory => {
                DiagnosticTraitFulfillmentMismatch::MemberCategory
            }
            TraitFulfillmentMismatch::Generic(mismatch) => match mismatch {
                GenericSurfaceMismatch::FulfillmentIsNotGeneric => {
                    DiagnosticTraitFulfillmentMismatch::FulfillmentIsNotGeneric
                }
                GenericSurfaceMismatch::ParameterCount { required, provided } => {
                    DiagnosticTraitFulfillmentMismatch::GenericParameterCount {
                        required: diagnostic_count(required)?,
                        provided: diagnostic_count(provided)?,
                    }
                }
                GenericSurfaceMismatch::ParameterCategory {
                    ordinal,
                    required,
                    provided,
                } => DiagnosticTraitFulfillmentMismatch::GenericParameterCategory {
                    ordinal: diagnostic_count(ordinal)?,
                    required: diagnostic_generic_category(required),
                    provided: diagnostic_generic_category(provided),
                },
                GenericSurfaceMismatch::ConstantParameterType {
                    ordinal,
                    required,
                    provided,
                } => DiagnosticTraitFulfillmentMismatch::GenericConstantParameterType {
                    ordinal: diagnostic_count(ordinal)?,
                    required: type_value(&required)?,
                    provided: type_value(&provided)?,
                },
                GenericSurfaceMismatch::Constraints(mismatch) => {
                    DiagnosticTraitFulfillmentMismatch::GenericConstraints(
                        diagnostic_generic_constraint_mismatch(implementation, mismatch)?,
                    )
                }
            },
            TraitFulfillmentMismatch::Receiver { required, provided } => {
                DiagnosticTraitFulfillmentMismatch::Receiver {
                    required: required.map(diagnostic_receiver_mode),
                    provided: provided.map(diagnostic_receiver_mode),
                }
            }
            TraitFulfillmentMismatch::CallableConstness { required, provided } => {
                DiagnosticTraitFulfillmentMismatch::CallableConstness {
                    required: diagnostic_callable_constness(required),
                    provided: diagnostic_callable_constness(provided),
                }
            }
            TraitFulfillmentMismatch::CallableExecution { required, provided } => {
                DiagnosticTraitFulfillmentMismatch::CallableExecution {
                    required: diagnostic_callable_execution(required),
                    provided: diagnostic_callable_execution(provided),
                }
            }
            TraitFulfillmentMismatch::CallableTrust { required, provided } => {
                DiagnosticTraitFulfillmentMismatch::CallableTrust {
                    required: diagnostic_callable_trust(required),
                    provided: diagnostic_callable_trust(provided),
                }
            }
            TraitFulfillmentMismatch::CallableAbi { required, provided } => {
                DiagnosticTraitFulfillmentMismatch::CallableAbi {
                    required: diagnostic_callable_abi(required),
                    provided: diagnostic_callable_abi(provided),
                }
            }
            TraitFulfillmentMismatch::CallableParameterCount { required, provided } => {
                DiagnosticTraitFulfillmentMismatch::CallableParameterCount {
                    required: diagnostic_count(required)?,
                    provided: diagnostic_count(provided)?,
                }
            }
            TraitFulfillmentMismatch::CallableParameterName {
                ordinal,
                required,
                provided,
            } => DiagnosticTraitFulfillmentMismatch::CallableParameterName {
                ordinal: diagnostic_count(ordinal)?,
                required,
                provided,
            },
            TraitFulfillmentMismatch::CallableParameterPosition {
                ordinal,
                required,
                provided,
            } => DiagnosticTraitFulfillmentMismatch::CallableParameterPosition {
                ordinal: diagnostic_count(ordinal)?,
                required: diagnostic_callable_position(required),
                provided: diagnostic_callable_position(provided),
            },
            TraitFulfillmentMismatch::CallableParameterMode {
                ordinal,
                required,
                provided,
            } => DiagnosticTraitFulfillmentMismatch::CallableParameterMode {
                ordinal: diagnostic_count(ordinal)?,
                required: diagnostic_callable_parameter_mode(required),
                provided: diagnostic_callable_parameter_mode(provided),
            },
            TraitFulfillmentMismatch::CallableParameterType {
                ordinal,
                required,
                provided,
            } => DiagnosticTraitFulfillmentMismatch::CallableParameterType {
                ordinal: diagnostic_count(ordinal)?,
                required: type_value(&required)?,
                provided: type_value(&provided)?,
            },
            TraitFulfillmentMismatch::CallableResultType { required, provided } => {
                DiagnosticTraitFulfillmentMismatch::CallableResultType {
                    required: type_value(&required)?,
                    provided: type_value(&provided)?,
                }
            }
            TraitFulfillmentMismatch::CallableParameterDefault {
                ordinal,
                required,
                provided,
            } => DiagnosticTraitFulfillmentMismatch::CallableParameterDefault {
                ordinal: diagnostic_count(ordinal)?,
                required,
                provided,
            },
            TraitFulfillmentMismatch::CallableContract(mismatch) => {
                DiagnosticTraitFulfillmentMismatch::CallableContract(
                    diagnostic_callable_contract_mismatch(implementation, mismatch)?,
                )
            }
            TraitFulfillmentMismatch::ConstantType { required, provided } => {
                DiagnosticTraitFulfillmentMismatch::ConstantType {
                    required: type_value(&required)?,
                    provided: type_value(&provided)?,
                }
            }
            TraitFulfillmentMismatch::TypeValueUnavailable => {
                DiagnosticTraitFulfillmentMismatch::TypeValueUnavailable
            }
            TraitFulfillmentMismatch::PredicateTrust { required, provided } => {
                DiagnosticTraitFulfillmentMismatch::PredicateTrust { required, provided }
            }
            TraitFulfillmentMismatch::PredicateParameterCount { required, provided } => {
                DiagnosticTraitFulfillmentMismatch::PredicateParameterCount {
                    required: diagnostic_count(required)?,
                    provided: diagnostic_count(provided)?,
                }
            }
            TraitFulfillmentMismatch::PredicateParameterName {
                ordinal,
                required,
                provided,
            } => DiagnosticTraitFulfillmentMismatch::PredicateParameterName {
                ordinal: diagnostic_count(ordinal)?,
                required,
                provided,
            },
            TraitFulfillmentMismatch::PredicateParameterType {
                ordinal,
                required,
                provided,
            } => DiagnosticTraitFulfillmentMismatch::PredicateParameterType {
                ordinal: diagnostic_count(ordinal)?,
                required: type_value(&required)?,
                provided: type_value(&provided)?,
            },
        };

        Ok(mismatch)
    }
}

fn diagnostic_count(
    implementation: ImplementationSymbolId,
    value: usize,
) -> Result<u64, FactQueryError> {
    u64::try_from(value).map_err(|_| {
        crate::compilation::SemanticQueryFailure::contract(
            crate::compilation::SemanticQueryContext::Symbol(implementation.into_any()),
            crate::compilation::SemanticQueryViolation::CapacityExceeded {
                data: crate::compilation::SemanticDataKind::Diagnostic,
                value,
            },
        )
        .into()
    })
}

fn diagnostic_generic_constraint_mismatch(
    implementation: ImplementationSymbolId,
    mismatch: GenericConstraintMismatch,
) -> Result<DiagnosticGenericConstraintMismatch, FactQueryError> {
    let diagnostic_count = |value| diagnostic_count(implementation, value);

    let mismatch = match mismatch {
        GenericConstraintMismatch::Count { required, provided } => {
            DiagnosticGenericConstraintMismatch::Count {
                required: diagnostic_count(required)?,
                provided: diagnostic_count(provided)?,
            }
        }
        GenericConstraintMismatch::Ordinal { index } => {
            DiagnosticGenericConstraintMismatch::Ordinal(diagnostic_count(index)?)
        }
        GenericConstraintMismatch::Category {
            index,
            required,
            provided,
        } => DiagnosticGenericConstraintMismatch::Category {
            index: diagnostic_count(index)?,
            required: diagnostic_constraint_category(required),
            provided: diagnostic_constraint_category(provided),
        },
        GenericConstraintMismatch::PredicateDependencies { index } => {
            DiagnosticGenericConstraintMismatch::PredicateDependencies(diagnostic_count(index)?)
        }
        GenericConstraintMismatch::TraitSatisfaction { index } => {
            DiagnosticGenericConstraintMismatch::TraitSatisfaction(diagnostic_count(index)?)
        }
        GenericConstraintMismatch::TypeEquality { index } => {
            DiagnosticGenericConstraintMismatch::TypeEquality(diagnostic_count(index)?)
        }
    };

    Ok(mismatch)
}

fn diagnostic_callable_contract_mismatch(
    implementation: ImplementationSymbolId,
    mismatch: CallableContractMismatch,
) -> Result<DiagnosticCallableContractMismatch, FactQueryError> {
    let diagnostic_count = |value| diagnostic_count(implementation, value);

    let mismatch = match mismatch {
        CallableContractMismatch::ClauseCount {
            surface,
            required,
            provided,
        } => DiagnosticCallableContractMismatch::ClauseCount {
            surface: diagnostic_callable_contract_surface(surface),
            required: diagnostic_count(required)?,
            provided: diagnostic_count(provided)?,
        },
        CallableContractMismatch::ClauseOrdinal { surface, index } => {
            DiagnosticCallableContractMismatch::ClauseOrdinal {
                surface: diagnostic_callable_contract_surface(surface),
                index: diagnostic_count(index)?,
            }
        }
        CallableContractMismatch::ClauseKind { surface, index } => {
            DiagnosticCallableContractMismatch::ClauseKind {
                surface: diagnostic_callable_contract_surface(surface),
                index: diagnostic_count(index)?,
            }
        }
        CallableContractMismatch::ClauseCategory {
            surface,
            index,
            required,
            provided,
        } => DiagnosticCallableContractMismatch::ClauseCategory {
            surface: diagnostic_callable_contract_surface(surface),
            index: diagnostic_count(index)?,
            required: diagnostic_callable_clause_category(required),
            provided: diagnostic_callable_clause_category(provided),
        },
        CallableContractMismatch::PredicateDependencies { surface, index } => {
            DiagnosticCallableContractMismatch::PredicateDependencies {
                surface: diagnostic_callable_contract_surface(surface),
                index: diagnostic_count(index)?,
            }
        }
        CallableContractMismatch::TraitSatisfaction { surface, index } => {
            DiagnosticCallableContractMismatch::TraitSatisfaction {
                surface: diagnostic_callable_contract_surface(surface),
                index: diagnostic_count(index)?,
            }
        }
        CallableContractMismatch::Behavior { phase, component } => {
            DiagnosticCallableContractMismatch::Behavior {
                phase: diagnostic_callable_behavior_phase(phase),
                component: diagnostic_callable_behavior_component(component),
            }
        }
        CallableContractMismatch::DeferredExecutionPresence => {
            DiagnosticCallableContractMismatch::DeferredExecutionPresence
        }
    };

    Ok(mismatch)
}

const fn diagnostic_constraint_category(value: ConstraintCategory) -> DiagnosticConstraintCategory {
    match value {
        ConstraintCategory::Predicate => DiagnosticConstraintCategory::Predicate,
        ConstraintCategory::TraitSatisfaction => DiagnosticConstraintCategory::TraitSatisfaction,
        ConstraintCategory::TypeEquality => DiagnosticConstraintCategory::TypeEquality,
    }
}

const fn diagnostic_callable_clause_category(
    value: CallableContractClauseCategory,
) -> DiagnosticCallableContractClauseCategory {
    match value {
        CallableContractClauseCategory::Predicate => {
            DiagnosticCallableContractClauseCategory::Predicate
        }
        CallableContractClauseCategory::TraitSatisfaction => {
            DiagnosticCallableContractClauseCategory::TraitSatisfaction
        }
    }
}

const fn diagnostic_callable_behavior_phase(
    value: CallableBehaviorPhase,
) -> DiagnosticCallableBehaviorPhase {
    match value {
        CallableBehaviorPhase::Invocation => DiagnosticCallableBehaviorPhase::Invocation,
        CallableBehaviorPhase::DeferredExecution => {
            DiagnosticCallableBehaviorPhase::DeferredExecution
        }
    }
}

const fn diagnostic_callable_behavior_component(
    value: CallableBehaviorComponent,
) -> DiagnosticCallableBehaviorComponent {
    match value {
        CallableBehaviorComponent::Effects => DiagnosticCallableBehaviorComponent::Effects,
        CallableBehaviorComponent::Capabilities => {
            DiagnosticCallableBehaviorComponent::Capabilities
        }
        CallableBehaviorComponent::TrustedCapabilities => {
            DiagnosticCallableBehaviorComponent::TrustedCapabilities
        }
        CallableBehaviorComponent::ExecutionRequirements => {
            DiagnosticCallableBehaviorComponent::ExecutionRequirements
        }
        CallableBehaviorComponent::LifecycleObligations => {
            DiagnosticCallableBehaviorComponent::LifecycleObligations
        }
        CallableBehaviorComponent::Dependencies => {
            DiagnosticCallableBehaviorComponent::Dependencies
        }
    }
}

const fn diagnostic_generic_category(
    category: GenericParameterCategory,
) -> DiagnosticGenericParameterCategory {
    match category {
        GenericParameterCategory::Type => DiagnosticGenericParameterCategory::Type,
        GenericParameterCategory::Constant => DiagnosticGenericParameterCategory::Constant,
    }
}

const fn diagnostic_receiver_mode(mode: bray_symbols::ReceiverMode) -> DiagnosticReceiverMode {
    match mode {
        bray_symbols::ReceiverMode::Shared => DiagnosticReceiverMode::Shared,
        bray_symbols::ReceiverMode::Mutable => DiagnosticReceiverMode::Mutable,
        bray_symbols::ReceiverMode::Consuming => DiagnosticReceiverMode::Consuming,
        bray_symbols::ReceiverMode::ConsumingMutable => DiagnosticReceiverMode::ConsumingMutable,
    }
}

const fn diagnostic_callable_constness(
    value: bray_symbols::CallableConstness,
) -> DiagnosticCallableConstness {
    match value {
        bray_symbols::CallableConstness::Runtime => DiagnosticCallableConstness::Runtime,
        bray_symbols::CallableConstness::Constant => DiagnosticCallableConstness::Constant,
    }
}

const fn diagnostic_callable_trust(value: bray_symbols::CallableTrust) -> DiagnosticCallableTrust {
    match value {
        bray_symbols::CallableTrust::Safe => DiagnosticCallableTrust::Safe,
        bray_symbols::CallableTrust::Trusted => DiagnosticCallableTrust::Trusted,
    }
}

const fn diagnostic_callable_position(
    value: bray_symbols::CallablePosition,
) -> DiagnosticCallablePosition {
    match value {
        bray_symbols::CallablePosition::NamedOnly => DiagnosticCallablePosition::NamedOnly,
        bray_symbols::CallablePosition::PositionalOrNamed => {
            DiagnosticCallablePosition::PositionalOrNamed
        }
    }
}

const fn diagnostic_callable_parameter_mode(
    value: bray_symbols::CallableParameterMode,
) -> DiagnosticCallableParameterMode {
    match value {
        bray_symbols::CallableParameterMode::Immutable => {
            DiagnosticCallableParameterMode::Immutable
        }
        bray_symbols::CallableParameterMode::Mutable => DiagnosticCallableParameterMode::Mutable,
    }
}

const fn diagnostic_callable_contract_surface(
    value: CallableContractSurface,
) -> DiagnosticCallableContractSurface {
    match value {
        CallableContractSurface::InvocationPreconditions => {
            DiagnosticCallableContractSurface::InvocationPreconditions
        }
        CallableContractSurface::StaticConstraints => {
            DiagnosticCallableContractSurface::StaticConstraints
        }
        CallableContractSurface::CompletionPostconditions => {
            DiagnosticCallableContractSurface::CompletionPostconditions
        }
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
            let implementation = $implementation.ok_or_else(|| {
                crate::compilation::SemanticQueryFailure::contract(
                    crate::compilation::SemanticQueryContext::Symbol(implementation.into_any()),
                    crate::compilation::SemanticQueryViolation::Missing(
                        crate::compilation::SemanticDataKind::Implementation,
                    ),
                )
            })?;

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
            let symbol = implementation.into_any();

            return Err(crate::compilation::SemanticQueryFailure::contract(
                crate::compilation::SemanticQueryContext::Symbol(symbol),
                crate::compilation::SemanticQueryViolation::UnexpectedSymbolKind {
                    expected: crate::compilation::SemanticSymbolCategory::TraitImplementation,
                    actual: symbol.kind(),
                },
            )
            .into());
        }
    })
}

fn sort_by_symbol_key<T>(
    symbols: &bray_symbols::SymbolGraph,
    imported: Option<&bray_symbols::ImportedSymbolSkeleton>,
    values: &mut [T],
    symbol: impl Fn(&T) -> bray_symbols::AnySymbolId,
) -> Result<(), FactQueryError> {
    for value in values.iter() {
        let symbol = symbol(value);

        let key = symbols
            .symbol_key(symbol)
            .or_else(|| imported.and_then(|symbols| symbols.symbol_key(symbol)));

        if key.is_none() {
            return Err(crate::compilation::SemanticQueryFailure::contract(
                crate::compilation::SemanticQueryContext::Symbol(symbol),
                crate::compilation::SemanticQueryViolation::Missing(
                    crate::compilation::SemanticDataKind::SymbolKey,
                ),
            )
            .into());
        }
    }

    values.sort_by(|left, right| {
        let left = symbols
            .symbol_key(symbol(left))
            .or_else(|| imported.and_then(|symbols| symbols.symbol_key(symbol(left))));

        let right = symbols
            .symbol_key(symbol(right))
            .or_else(|| imported.and_then(|symbols| symbols.symbol_key(symbol(right))));

        match (left, right) {
            (Some(left), Some(right)) => left.cmp(right),
            // Both graphs are immutable across this sort, so prevalidation makes this fallback
            // unreachable without hiding a query failure in the comparison callback.
            _ => std::cmp::Ordering::Equal,
        }
    });

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
        .ok_or_else(|| {
            crate::compilation::SemanticQueryFailure::contract(
                crate::compilation::SemanticQueryContext::Symbol(symbol),
                crate::compilation::SemanticQueryViolation::Missing(
                    crate::compilation::SemanticDataKind::MemberName,
                ),
            )
        })?;

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
    binding_context: &CompilationBindingContext<'_>,
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

        let value = binding_context
            .resolve_symbol_query(SymbolQueryRequest::<TraitTypeFulfillmentValueQuery>::new(
                *fulfillment,
            ))
            .map_err(crate::compilation::binder::binding_query_error)?;

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
        .map_err(FactQueryError::SemanticValueStore)?;

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

        let callable =
            bray_symbols::CallableSymbolId::try_from_any(member.id()).ok_or_else(|| {
                crate::compilation::SemanticQueryFailure::contract(
                    crate::compilation::SemanticQueryContext::Symbol(member.id()),
                    crate::compilation::SemanticQueryViolation::UnexpectedSymbolKind {
                        expected: crate::compilation::SemanticSymbolCategory::Callable,
                        actual: member.id().kind(),
                    },
                )
            })?;

        fulfillments.insert(slot, (member.id(), callable));
    }

    Ok(fulfillments)
}

fn requirement_has_default(
    binding_context: &CompilationBindingContext<'_>,
    requirement: TraitMemberRequirementId,
    diagnostics: &mut bray_diagnostics::DiagnosticBag,
) -> Result<bool, FactQueryError> {
    match requirement {
        TraitMemberRequirementId::Callable(requirement) => {
            let signature = binding_context
                .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(
                    requirement.into(),
                ))
                .map_err(crate::compilation::binder::binding_query_error)?;

            *diagnostics = diagnostics.merged(signature.diagnostics());

            Ok(signature.value().has_body())
        }
        TraitMemberRequirementId::Constant(requirement) => {
            let definition = binding_context
                .resolve_symbol_query(
                    SymbolQueryRequest::<TraitConstantMemberDefinitionQuery>::new(requirement),
                )
                .map_err(crate::compilation::binder::binding_query_error)?;

            *diagnostics = diagnostics.merged(definition.diagnostics());

            Ok(matches!(
                definition.value(),
                ConstantDefinitionState::Defined(_)
            ))
        }
        TraitMemberRequirementId::Predicate(requirement) => {
            let definition = binding_context
                .resolve_symbol_query(
                    SymbolQueryRequest::<TraitPredicateMemberDefinitionQuery>::new(requirement),
                )
                .map_err(crate::compilation::binder::binding_query_error)?;

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
    mismatch: Option<DiagnosticTraitFulfillmentMismatch>,
    related: impl IntoIterator<Item = (DiagnosticRelatedLocationKind, bray_symbols::AnySymbolId)>,
) -> Result<Diagnostic, FactQueryError> {
    let anchor = symbols
        .declaration_syntax_anchor(span_symbol)
        .ok_or_else(|| {
            crate::compilation::SemanticQueryFailure::contract(
                crate::compilation::SemanticQueryContext::Symbol(span_symbol),
                crate::compilation::SemanticQueryViolation::Missing(
                    crate::compilation::SemanticDataKind::SourceAnchor,
                ),
            )
        })?;

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

    let primary_span = SourceSpan::new(anchor.source_id(), anchor.full_range());

    let mut diagnostic = Diagnostic::new(DiagnosticId::new(0), kind, SeverityKind::Error)
        .with_primary_span(primary_span)
        .with_label(DiagnosticLabel::primary(
            DiagnosticLabelKind::TraitFulfillment,
            primary_span,
        ))
        .with_arg(member)
        .with_optional_arg(mismatch.map(DiagnosticArg::trait_fulfillment_mismatch));

    for (kind, symbol) in related {
        let Some(anchor) = symbols.declaration_syntax_anchor(symbol) else {
            continue;
        };

        diagnostic = diagnostic.with_related_location(DiagnosticRelatedLocation::new(
            kind,
            SourceSpan::new(anchor.source_id(), anchor.full_range()),
        ));
    }

    Ok(diagnostic)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_diagnostics::{
        DiagnosticArgValue, DiagnosticGenericParameterCategory, DiagnosticKind,
        DiagnosticRelatedLocationKind, DiagnosticTraitFulfillmentMismatch,
    };
    use bray_messages::DiagnosticRenderer;
    use bray_symbols::{ImplementationSymbolId, SymbolOrigin, TraitRequirementResolution};
    use bray_testing::assert_goal_state_diagnostic_kind;

    use crate::compilation::{
        SemanticQueryContext, SemanticQueryFailure, SemanticQueryViolation, SemanticSymbolCategory,
    };
    use crate::fact::{CompilationFactKey, FactQueryError};
    use crate::test_support::compilation;

    #[test]
    fn inherent_implementation_rejection_retains_the_exact_symbol_kind() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "struct Holder\n",
            "{\n",
            "}\n",
            "\n",
            "impl Holder\n",
            "{\n",
            "}\n",
        ));

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("symbol graph must be available: {error:?}"));

        let implementation = symbols
            .inherent_implementations()
            .iter()
            .find(|symbol| symbol.origin() == SymbolOrigin::Source)
            .map(|symbol| ImplementationSymbolId::from(symbol.id()))
            .unwrap_or_else(|| panic!("test source must declare one inherent implementation"));

        let error = compilation
            .trait_implementation_conformance(implementation)
            .expect_err("an inherent implementation cannot have trait conformance");

        let FactQueryError::SemanticQuery(error) = error else {
            panic!("implementation-kind mismatch must retain a semantic-query failure");
        };

        let symbol = implementation.into_any();

        assert_eq!(
            error.cause(),
            &SemanticQueryFailure::contract(
                SemanticQueryContext::Symbol(symbol),
                SemanticQueryViolation::UnexpectedSymbolKind {
                    expected: SemanticSymbolCategory::TraitImplementation,
                    actual: symbol.kind(),
                },
            )
        );
    }

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
    fn missing_trait_fulfillment_points_to_the_required_member() {
        let compilation = compilation(concat!(
            "module app;\n",
            "struct Holder {}\n",
            "trait Provides\n",
            "{\n",
            "    const enabled: bool;\n",
            "}\n",
            "impl Holder(Provides) {}\n",
        ));

        let result = compilation
            .trait_implementation_conformance(source_implementation(&compilation))
            .unwrap_or_else(|error| panic!("conformance must publish: {error:?}"));

        assert_goal_state_diagnostic_kind(
            result.diagnostics(),
            DiagnosticKind::CheckingMissingTraitFulfillment,
        );

        let diagnostics = result.diagnostics().iter().collect::<Vec<_>>();

        let [diagnostic] = diagnostics.as_slice() else {
            panic!("test source must produce one missing-fulfillment diagnostic");
        };

        assert_eq!(diagnostic.related_locations().len(), 1);

        assert_eq!(
            diagnostic.related_locations()[0].kind(),
            DiagnosticRelatedLocationKind::RequirementOrigin
        );
    }

    #[test]
    fn extra_trait_fulfillment_identifies_the_unrequired_member() {
        let compilation = compilation(concat!(
            "module app;\n",
            "struct Holder {}\n",
            "trait Provides {}\n",
            "impl Holder(Provides)\n",
            "{\n",
            "    const extra: bool = true;\n",
            "}\n",
        ));

        let result = compilation
            .trait_implementation_conformance(source_implementation(&compilation))
            .unwrap_or_else(|error| panic!("conformance must publish: {error:?}"));

        assert_goal_state_diagnostic_kind(
            result.diagnostics(),
            DiagnosticKind::CheckingExtraTraitFulfillment,
        );

        assert_eq!(
            result
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.kind())
                .collect::<Vec<_>>(),
            [DiagnosticKind::CheckingExtraTraitFulfillment]
        );
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
    fn inferred_async_cancellation_does_not_change_trait_fulfillment_compatibility() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "struct Holder\n",
            "{\n",
            "}\n",
            "\n",
            "async func operation() -> i32\n",
            "{\n",
            "    return 1;\n",
            "}\n",
            "\n",
            "trait Provides\n",
            "{\n",
            "    async func get() -> i32;\n",
            "}\n",
            "\n",
            "impl Holder(Provides)\n",
            "{\n",
            "    async func get() -> i32\n",
            "    {\n",
            "        return await operation();\n",
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

        assert_goal_state_diagnostic_kind(
            result.diagnostics(),
            DiagnosticKind::CheckingIncompatibleTraitFulfillment,
        );

        let diagnostics = result.diagnostics().iter().collect::<Vec<_>>();

        let [diagnostic] = diagnostics.as_slice() else {
            panic!("test source must produce one conformance diagnostic");
        };

        assert!(diagnostic.args().iter().any(|argument| matches!(
            argument.value(),
            DiagnosticArgValue::TraitFulfillmentMismatch(
                DiagnosticTraitFulfillmentMismatch::GenericParameterCategory {
                    ordinal: 0,
                    required: DiagnosticGenericParameterCategory::Type,
                    provided: DiagnosticGenericParameterCategory::Constant,
                }
            )
        )));

        assert!(diagnostic.related_locations().iter().any(|location| {
            location.kind() == DiagnosticRelatedLocationKind::RequirementOrigin
        }));

        assert_eq!(
            DiagnosticRenderer::english().render(diagnostic).message(),
            "trait fulfillment of 'get' is incompatible: generic parameter 1 is a constant parameter, but the trait requires a type parameter"
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
    fn trait_arguments_are_substituted_inside_nested_callable_types() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "struct Holder<T>\n",
            "{\n",
            "    value: T;\n",
            "}\n",
            "\n",
            "impl Holder<T>(ElementIndex<usize>)\n",
            "{\n",
            "    type Output = T;\n",
            "\n",
            "    func index(pos selector: &usize) -> &T\n",
            "    {\n",
            "        return &self.value;\n",
            "    }\n",
            "}\n",
            "\n",
            "func select(pos holder: Holder<usize>) -> usize\n",
            "{\n",
            "    let selector: usize = 0;\n",
            "    let selected: &usize = holder[selector];\n",
            "\n",
            "    return 0;\n",
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

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );
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

        assert_goal_state_diagnostic_kind(
            result.diagnostics(),
            DiagnosticKind::CheckingDuplicateTraitFulfillment,
        );

        let diagnostics = result.diagnostics().iter().collect::<Vec<_>>();

        let [diagnostic] = diagnostics.as_slice() else {
            panic!("test source must produce one duplicate diagnostic");
        };

        assert_eq!(diagnostic.related_locations().len(), 1);

        assert_eq!(
            diagnostic.related_locations()[0].kind(),
            DiagnosticRelatedLocationKind::FirstDeclaration
        );
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
