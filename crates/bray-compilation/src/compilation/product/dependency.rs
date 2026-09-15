use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    BoundCallableTarget, BoundExpression, BoundReferenceTarget, BoundUnitKind, ConstructionInputId,
    ConstructionTarget, ConversionTarget, DefaultValueProvider, IndexTarget, OperatorTarget,
    SelectedImplementationWitness, SelectedIterationSource, SelectedOperation, SemanticSelection,
};
use bray_declarations::DeclarationTable;
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{AnySymbolId, SemanticValueStore, SymbolGraph};

use super::super::Compilation;
use super::visibility::{
    add_internal_dependency_diagnostic, callable_instance_internal_dependency,
    implementation_instance_internal_dependency, resolved_type_internal_dependency,
    source_symbol_is_not_publicly_reachable, substitution_internal_dependency,
};
use crate::fact::{BatchWork, CancellationToken, FactQueryError};

pub(super) fn validate_public_expression_dependencies(
    compilation: &Compilation,
    cancellation: &CancellationToken,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
    public_symbols: &BTreeSet<AnySymbolId>,
    diagnostics: &mut DiagnosticBag,
) -> Result<bool, FactQueryError> {
    let mut roots = Vec::new();

    for key in compilation.declared_unit_keys()? {
        cancellation.check()?;

        if !matches!(
            key.kind(),
            BoundUnitKind::Constraint | BoundUnitKind::ContractClause
        ) {
            continue;
        }

        let Some(owner) = symbols.symbol_for_key(key.declared_owner()) else {
            continue;
        };

        if !public_symbols.contains(&owner) {
            continue;
        }

        // Scheduled work retains the shared unit identity independently of the inventory.
        roots.push((owner, key.clone()));
    }

    let completed = compilation
        .state
        .fact_runtime
        .complete_batch(roots, cancellation, |(_, key)| {
            // Both lazy query results own the same Arc-backed unit key independently.
            let bound = compilation.bound_unit_with_cancellation(key.clone(), cancellation)?;

            let expressions =
                compilation.expression_semantics_with_cancellation(key.clone(), cancellation)?;

            let internal = bound_expression_internal_dependency(
                bound.result().value(),
                expressions.result().value().selections(),
                semantic_values,
                symbols,
                declarations,
            )?;

            Ok::<_, FactQueryError>(BatchWork::leaf((expressions, internal)))
        })
        .map_err(|error| error.into_fact_query_error())?;

    let mut recovered_owners = BTreeMap::new();

    for ((owner, _), (expressions, internal)) in completed {
        diagnostics.add_range(expressions.result().diagnostics().iter().cloned());

        if let Some(internal) = internal {
            recovered_owners.entry(owner).or_insert(internal);
        }
    }

    for (owner, internal) in recovered_owners.iter() {
        add_internal_dependency_diagnostic(*owner, *internal, symbols, diagnostics)?;
    }

    Ok(!recovered_owners.is_empty())
}

fn bound_expression_internal_dependency(
    unit: &bray_bound_tree::BoundUnit,
    selections: &bray_bound_tree::CheckedSemanticSelections,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
) -> Result<Option<AnySymbolId>, FactQueryError> {
    for (_, expression) in unit.tree().expressions() {
        if let BoundExpression::Name(name) = expression
            && let BoundReferenceTarget::Surface(symbol) = name.target()
            && source_symbol_is_not_publicly_reachable(symbol, declarations, symbols)
        {
            return Ok(Some(symbol));
        }
    }

    for entry in selections.entries() {
        if let Some(internal) = selection_internal_dependency(
            entry.selection(),
            semantic_values,
            symbols,
            declarations,
        )? {
            return Ok(Some(internal));
        }
    }

    Ok(None)
}

fn selection_internal_dependency(
    selection: &SemanticSelection,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
) -> Result<Option<AnySymbolId>, FactQueryError> {
    let mut dependencies = Vec::new();
    let mut semantic_internal = None;

    match selection {
        SemanticSelection::Reference(BoundReferenceTarget::Surface(symbol)) => {
            dependencies.push(*symbol);
        }
        SemanticSelection::Reference(BoundReferenceTarget::Local(_)) => {}
        SemanticSelection::CallableReference(callable) => {
            dependencies.push(callable.definition().symbol());

            semantic_internal = callable_instance_internal_dependency(
                *callable,
                semantic_values,
                symbols,
                declarations,
            )?;
        }
        SemanticSelection::StaticReference(instance) => {
            dependencies.push(instance.template().declaration().into());

            semantic_internal = substitution_internal_dependency(
                instance.substitution(),
                semantic_values,
                symbols,
                declarations,
            )?;
        }
        SemanticSelection::Call(call) => {
            if let BoundCallableTarget::Declaration(target) = call.target() {
                dependencies.push(target.definition().symbol());

                if let Some(internal) = callable_instance_internal_dependency(
                    target,
                    semantic_values,
                    symbols,
                    declarations,
                )? {
                    return Ok(Some(internal));
                }
            } else if let BoundCallableTarget::Indirect(ty) = call.target() {
                semantic_internal =
                    resolved_type_internal_dependency(ty, semantic_values, symbols, declarations)?;
            }

            if let Some(receiver) = call.receiver() {
                if semantic_internal.is_none() {
                    semantic_internal = resolved_type_internal_dependency(
                        receiver.source_type(),
                        semantic_values,
                        symbols,
                        declarations,
                    )?;
                }

                if semantic_internal.is_none() {
                    semantic_internal = resolved_type_internal_dependency(
                        receiver.target_type(),
                        semantic_values,
                        symbols,
                        declarations,
                    )?;
                }
            }

            for argument in call.arguments() {
                if let bray_bound_tree::SelectedArgument::Explicit { conversion, .. } = argument {
                    semantic_internal = semantic_internal.or(push_conversion_dependencies(
                        semantic_values,
                        symbols,
                        declarations,
                        conversion,
                        &mut dependencies,
                    )?);
                }
            }

            for witness in call.witnesses() {
                semantic_internal = semantic_internal.or(push_witness_dependencies(
                    semantic_values,
                    symbols,
                    declarations,
                    *witness,
                    &mut dependencies,
                )?);
            }
        }
        SemanticSelection::Predicate(predicate) => {
            dependencies.push(predicate.predicate().into_any());

            if let Some(internal) = substitution_internal_dependency(
                predicate.substitution(),
                semantic_values,
                symbols,
                declarations,
            )? {
                return Ok(Some(internal));
            }
        }
        SemanticSelection::Operation(operation) => {
            semantic_internal = push_operation_dependencies(
                semantic_values,
                symbols,
                declarations,
                operation,
                &mut dependencies,
            )?;
        }
        SemanticSelection::Iteration(iteration) => {
            semantic_internal = push_iteration_dependencies(
                semantic_values,
                symbols,
                declarations,
                iteration,
                &mut dependencies,
            )?;
        }
        SemanticSelection::Propagation(propagation) => {
            if let Some(conversion) = propagation.error_conversion() {
                semantic_internal = push_conversion_dependencies(
                    semantic_values,
                    symbols,
                    declarations,
                    conversion,
                    &mut dependencies,
                )?;
            }
        }
    }

    let internal = dependencies
        .into_iter()
        .find(|symbol| source_symbol_is_not_publicly_reachable(*symbol, declarations, symbols));

    Ok(semantic_internal.or(internal))
}

fn push_iteration_dependencies(
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
    iteration: &SelectedIterationSource,
    dependencies: &mut Vec<AnySymbolId>,
) -> Result<Option<AnySymbolId>, FactQueryError> {
    let mut internal = None;

    for ty in [
        iteration.source_type(),
        iteration.cursor_type(),
        iteration.element_type(),
    ] {
        if internal.is_none() {
            internal =
                resolved_type_internal_dependency(ty, semantic_values, symbols, declarations)?;
        }
    }

    for target in [
        iteration.iterate_member(),
        iteration.iterate(),
        iteration.next_member(),
        iteration.next(),
    ] {
        dependencies.push(target.definition().symbol());

        if internal.is_none() {
            internal = callable_instance_internal_dependency(
                target,
                semantic_values,
                symbols,
                declarations,
            )?;
        }
    }

    for witness in [
        SelectedImplementationWitness::new(
            iteration.iterable_requirement(),
            iteration.iterable_witness(),
        ),
        SelectedImplementationWitness::new(
            iteration.iterator_requirement(),
            iteration.iterator_witness(),
        ),
    ] {
        internal = internal.or(push_witness_dependencies(
            semantic_values,
            symbols,
            declarations,
            witness,
            dependencies,
        )?);
    }

    Ok(internal)
}

fn push_operation_dependencies(
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
    operation: &SelectedOperation,
    dependencies: &mut Vec<AnySymbolId>,
) -> Result<Option<AnySymbolId>, FactQueryError> {
    let mut internal = None;

    if let Some(target) = operation.operator_target() {
        if let OperatorTarget::Trait {
            member,
            fulfillment,
            requirement,
            witness,
            ..
        } = target
        {
            internal = push_trait_operation_dependencies(
                semantic_values,
                symbols,
                declarations,
                member,
                fulfillment,
                requirement,
                witness,
                dependencies,
            )?;
        }

        return Ok(internal);
    }

    match operation {
        SelectedOperation::Member(target) => {
            dependencies.push(target.member());

            internal = resolved_type_internal_dependency(
                target.result_type(),
                semantic_values,
                symbols,
                declarations,
            )?;

            for witness in target.witnesses() {
                internal = internal.or(push_witness_dependencies(
                    semantic_values,
                    symbols,
                    declarations,
                    *witness,
                    dependencies,
                )?);
            }
        }
        SelectedOperation::Index {
            target:
                IndexTarget::Custom {
                    member,
                    fulfillment,
                    requirement,
                    witness,
                    ..
                },
            ..
        } => {
            internal = push_trait_operation_dependencies(
                semantic_values,
                symbols,
                declarations,
                *member,
                *fulfillment,
                *requirement,
                *witness,
                dependencies,
            )?;
        }
        SelectedOperation::Operator { .. }
        | SelectedOperation::CompoundAssignment(_)
        | SelectedOperation::Index { .. } => {}
        SelectedOperation::Construction(construction) => {
            internal = push_construction_dependencies(
                semantic_values,
                symbols,
                declarations,
                construction,
                dependencies,
            )?;
        }
        SelectedOperation::Conversion(conversion) => {
            internal = push_conversion_dependencies(
                semantic_values,
                symbols,
                declarations,
                conversion,
                dependencies,
            )?;
        }
        SelectedOperation::Implementation(witness) => {
            internal = push_witness_dependencies(
                semantic_values,
                symbols,
                declarations,
                *witness,
                dependencies,
            )?;
        }
    }

    Ok(internal)
}

#[expect(
    clippy::too_many_arguments,
    reason = "trait operation dependencies retain both callable instances and their witness"
)]
fn push_trait_operation_dependencies(
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
    member: bray_symbols::CallableInstanceData,
    fulfillment: bray_symbols::CallableInstanceData,
    requirement: bray_symbols::ImplementationRequirementKey,
    witness: bray_symbols::ImplementationInstanceId,
    dependencies: &mut Vec<AnySymbolId>,
) -> Result<Option<AnySymbolId>, FactQueryError> {
    dependencies.push(member.definition().symbol());
    dependencies.push(fulfillment.definition().symbol());

    let mut internal =
        callable_instance_internal_dependency(member, semantic_values, symbols, declarations)?;

    if internal.is_none() {
        internal = callable_instance_internal_dependency(
            fulfillment,
            semantic_values,
            symbols,
            declarations,
        )?;
    }

    internal = internal.or(push_witness_dependencies(
        semantic_values,
        symbols,
        declarations,
        SelectedImplementationWitness::new(requirement, witness),
        dependencies,
    )?);

    Ok(internal)
}

fn push_construction_dependencies(
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
    construction: &bray_bound_tree::SelectedConstruction,
    dependencies: &mut Vec<AnySymbolId>,
) -> Result<Option<AnySymbolId>, FactQueryError> {
    let mut internal = resolved_type_internal_dependency(
        construction.result_type(),
        semantic_values,
        symbols,
        declarations,
    )?;

    match construction.target() {
        ConstructionTarget::Struct(target) => dependencies.push(target.into()),
        ConstructionTarget::UnionVariant(target) => dependencies.push(target.into()),
        ConstructionTarget::TypeForm { callable, .. } => {
            dependencies.push(callable.definition().symbol());

            if internal.is_none() {
                internal = callable_instance_internal_dependency(
                    callable,
                    semantic_values,
                    symbols,
                    declarations,
                )?;
            }
        }
    }

    for input in construction.inputs() {
        match input {
            bray_bound_tree::SelectedConstructionInput::Explicit { input, .. } => {
                push_construction_input(*input, dependencies);
            }
            bray_bound_tree::SelectedConstructionInput::Default {
                input, provider, ..
            } => {
                push_construction_input(*input, dependencies);
                push_construction_default(*provider, dependencies);
            }
        }
    }

    Ok(internal)
}

fn push_construction_input(input: ConstructionInputId, dependencies: &mut Vec<AnySymbolId>) {
    let symbol = match input {
        ConstructionInputId::StructField(symbol) => symbol.into(),
        ConstructionInputId::UnionPayloadField(symbol) => symbol.into(),
        ConstructionInputId::CallableParameter(symbol) => symbol.into(),
    };

    dependencies.push(symbol);
}

fn push_construction_default(provider: DefaultValueProvider, dependencies: &mut Vec<AnySymbolId>) {
    dependencies.push(provider.symbol());
}

fn push_conversion_dependencies(
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
    root: &bray_bound_tree::SelectedConversion,
    dependencies: &mut Vec<AnySymbolId>,
) -> Result<Option<AnySymbolId>, FactQueryError> {
    let mut pending = vec![root];
    let mut internal = None;

    while let Some(conversion) = pending.pop() {
        if internal.is_none() {
            internal = resolved_type_internal_dependency(
                conversion.source_type(),
                semantic_values,
                symbols,
                declarations,
            )?;
        }

        if internal.is_none() {
            internal = resolved_type_internal_dependency(
                conversion.target_type(),
                semantic_values,
                symbols,
                declarations,
            )?;
        }

        match conversion.target() {
            ConversionTarget::Trait {
                member,
                fulfillment,
                requirement,
                witness,
            } => {
                dependencies.push(member.definition().symbol());
                dependencies.push(fulfillment.definition().symbol());

                if internal.is_none() {
                    internal = callable_instance_internal_dependency(
                        *member,
                        semantic_values,
                        symbols,
                        declarations,
                    )?;
                }

                if internal.is_none() {
                    internal = callable_instance_internal_dependency(
                        *fulfillment,
                        semantic_values,
                        symbols,
                        declarations,
                    )?;
                }

                internal = internal.or(push_witness_dependencies(
                    semantic_values,
                    symbols,
                    declarations,
                    SelectedImplementationWitness::new(*requirement, *witness),
                    dependencies,
                )?);
            }
            ConversionTarget::TraitConstraint { member, .. } => {
                dependencies.push(member.definition().symbol());

                if internal.is_none() {
                    internal = callable_instance_internal_dependency(
                        *member,
                        semantic_values,
                        symbols,
                        declarations,
                    )?;
                }
            }
            ConversionTarget::Composite(children) => pending.extend(children.iter()),
            ConversionTarget::Identity
            | ConversionTarget::CallableContract
            | ConversionTarget::NullablePresent
            | ConversionTarget::BuiltInScalar
            | ConversionTarget::CVariadicPromotion => {}
        }
    }

    Ok(internal)
}

fn push_witness_dependencies(
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
    witness: SelectedImplementationWitness,
    dependencies: &mut Vec<AnySymbolId>,
) -> Result<Option<AnySymbolId>, FactQueryError> {
    let application =
        semantic_values.trait_application_data(witness.requirement().trait_application());

    dependencies.push(application.definition().into());

    let mut application_internal = source_symbol_is_not_publicly_reachable(
        application.definition().into(),
        declarations,
        symbols,
    )
    .then_some(application.definition().into());

    if application_internal.is_none() {
        application_internal = substitution_internal_dependency(
            application.substitution(),
            semantic_values,
            symbols,
            declarations,
        )?;
    }

    let implementation_internal = implementation_instance_internal_dependency(
        witness.witness(),
        semantic_values,
        symbols,
        declarations,
    )?;

    Ok(application_internal.or(implementation_internal))
}
