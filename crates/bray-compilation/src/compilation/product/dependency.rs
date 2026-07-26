use std::collections::BTreeSet;

use bray_bound_tree::{
    BoundCallableTarget, BoundExpression, BoundReferenceTarget, BoundUnitKind,
    ConstructionDefaultProvider, ConstructionInputId, ConstructionTarget, ConversionTarget,
    IndexTarget, OperatorTarget, SelectedImplementationWitness, SelectedIterationSource,
    SelectedOperation, SemanticSelection,
};
use bray_declarations::DeclarationTable;
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{AnySymbolId, SemanticValueStore, SymbolGraph};

use super::super::Compilation;
use super::visibility::{
    add_internal_dependency_diagnostic, callable_instance_exposes_internal,
    implementation_instance_exposes_internal, resolved_type_exposes_internal,
    source_symbol_is_not_publicly_reachable, substitution_exposes_internal,
};
use crate::fact::{CancellationToken, FactQueryError};

pub(super) fn validate_public_expression_dependencies(
    compilation: &Compilation,
    cancellation: &CancellationToken,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
    public_symbols: &BTreeSet<AnySymbolId>,
    diagnostics: &mut DiagnosticBag,
) -> Result<bool, FactQueryError> {
    let mut recovered_owners = BTreeSet::new();

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

        // Both lazy facts own the same Arc-backed unit key independently.
        let bound = compilation.bound_unit_with_cancellation(key.clone(), cancellation)?;

        let selections = compilation.semantic_selections_with_cancellation(key, cancellation)?;

        diagnostics.add_range(selections.result().diagnostics().iter().cloned());

        if bound_expression_dependencies_expose_internal(
            bound.result().value(),
            selections.result().value(),
            semantic_values,
            symbols,
            declarations,
        )? {
            recovered_owners.insert(owner);
        }
    }

    for owner in recovered_owners.iter().copied() {
        add_internal_dependency_diagnostic(owner, symbols, diagnostics);
    }

    Ok(!recovered_owners.is_empty())
}

fn bound_expression_dependencies_expose_internal(
    unit: &bray_bound_tree::BoundUnit,
    selections: &bray_bound_tree::CheckedSemanticSelections,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
) -> Result<bool, FactQueryError> {
    for (_, expression) in unit.tree().expressions() {
        if let BoundExpression::Name(name) = expression
            && let BoundReferenceTarget::Surface(symbol) = name.target()
            && source_symbol_is_not_publicly_reachable(symbol, declarations, symbols)
        {
            return Ok(true);
        }
    }

    for entry in selections.entries() {
        if selection_exposes_internal(entry.selection(), semantic_values, symbols, declarations)? {
            return Ok(true);
        }
    }

    Ok(false)
}

fn selection_exposes_internal(
    selection: &SemanticSelection,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
) -> Result<bool, FactQueryError> {
    let mut dependencies = Vec::new();
    let mut semantic_values_expose_internal = false;

    match selection {
        SemanticSelection::Reference(BoundReferenceTarget::Surface(symbol)) => {
            dependencies.push(*symbol);
        }
        SemanticSelection::Reference(BoundReferenceTarget::Local(_)) => {}
        SemanticSelection::Call(call) => {
            if let BoundCallableTarget::Declaration(target) = call.target() {
                dependencies.push(target.definition().symbol());

                semantic_values_expose_internal |= callable_instance_exposes_internal(
                    target,
                    semantic_values,
                    symbols,
                    declarations,
                );
            } else if let BoundCallableTarget::Indirect(ty) = call.target() {
                semantic_values_expose_internal |=
                    resolved_type_exposes_internal(ty, semantic_values, symbols, declarations);
            }

            if let Some(receiver) = call.receiver() {
                semantic_values_expose_internal |= push_conversion_dependencies(
                    semantic_values,
                    symbols,
                    declarations,
                    receiver.conversion(),
                    &mut dependencies,
                )?;
            }

            for argument in call.arguments() {
                if let bray_bound_tree::SelectedArgument::Explicit { conversion, .. } = argument {
                    semantic_values_expose_internal |= push_conversion_dependencies(
                        semantic_values,
                        symbols,
                        declarations,
                        conversion,
                        &mut dependencies,
                    )?;
                }
            }

            for witness in call.witnesses() {
                semantic_values_expose_internal |= push_witness_dependencies(
                    semantic_values,
                    symbols,
                    declarations,
                    *witness,
                    &mut dependencies,
                )?;
            }
        }
        SemanticSelection::Operation(operation) => {
            semantic_values_expose_internal |= push_operation_dependencies(
                semantic_values,
                symbols,
                declarations,
                operation,
                &mut dependencies,
            )?;
        }
        SemanticSelection::Iteration(iteration) => {
            semantic_values_expose_internal |= push_iteration_dependencies(
                semantic_values,
                symbols,
                declarations,
                iteration,
                &mut dependencies,
            )?;
        }
    }

    Ok(semantic_values_expose_internal
        || dependencies
            .into_iter()
            .any(|symbol| source_symbol_is_not_publicly_reachable(symbol, declarations, symbols)))
}

fn push_iteration_dependencies(
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
    iteration: &SelectedIterationSource,
    dependencies: &mut Vec<AnySymbolId>,
) -> Result<bool, FactQueryError> {
    let mut exposes_internal = false;

    for ty in [
        iteration.source_type(),
        iteration.cursor_type(),
        iteration.element_type(),
    ] {
        exposes_internal |=
            resolved_type_exposes_internal(ty, semantic_values, symbols, declarations);
    }

    for target in [
        iteration.iterate_member(),
        iteration.iterate(),
        iteration.next_member(),
        iteration.next(),
    ] {
        dependencies.push(target.definition().symbol());

        exposes_internal |=
            callable_instance_exposes_internal(target, semantic_values, symbols, declarations);
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
        exposes_internal |= push_witness_dependencies(
            semantic_values,
            symbols,
            declarations,
            witness,
            dependencies,
        )?;
    }

    Ok(exposes_internal)
}

fn push_operation_dependencies(
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
    operation: &SelectedOperation,
    dependencies: &mut Vec<AnySymbolId>,
) -> Result<bool, FactQueryError> {
    let mut exposes_internal = false;

    match operation {
        SelectedOperation::Member(target) => {
            dependencies.push(target.member());

            exposes_internal |= resolved_type_exposes_internal(
                target.result_type(),
                semantic_values,
                symbols,
                declarations,
            );

            for witness in target.witnesses() {
                exposes_internal |= push_witness_dependencies(
                    semantic_values,
                    symbols,
                    declarations,
                    *witness,
                    dependencies,
                )?;
            }
        }
        SelectedOperation::Operator {
            target:
                OperatorTarget::Trait {
                    member,
                    fulfillment,
                    requirement,
                    witness,
                    ..
                },
            ..
        }
        | SelectedOperation::Index {
            target:
                IndexTarget::Custom {
                    member,
                    fulfillment,
                    requirement,
                    witness,
                },
            ..
        } => {
            dependencies.push(member.definition().symbol());
            dependencies.push(fulfillment.definition().symbol());

            exposes_internal |=
                callable_instance_exposes_internal(*member, semantic_values, symbols, declarations);

            exposes_internal |= callable_instance_exposes_internal(
                *fulfillment,
                semantic_values,
                symbols,
                declarations,
            );

            exposes_internal |= push_witness_dependencies(
                semantic_values,
                symbols,
                declarations,
                SelectedImplementationWitness::new(*requirement, *witness),
                dependencies,
            )?;
        }
        SelectedOperation::Operator { .. } | SelectedOperation::Index { .. } => {}
        SelectedOperation::Construction(construction) => {
            exposes_internal |= push_construction_dependencies(
                semantic_values,
                symbols,
                declarations,
                construction,
                dependencies,
            );
        }
        SelectedOperation::Conversion(conversion) => {
            exposes_internal |= push_conversion_dependencies(
                semantic_values,
                symbols,
                declarations,
                conversion,
                dependencies,
            )?;
        }
        SelectedOperation::Implementation(witness) => {
            exposes_internal |= push_witness_dependencies(
                semantic_values,
                symbols,
                declarations,
                *witness,
                dependencies,
            )?;
        }
    }

    Ok(exposes_internal)
}

fn push_construction_dependencies(
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
    construction: &bray_bound_tree::SelectedConstruction,
    dependencies: &mut Vec<AnySymbolId>,
) -> bool {
    let mut exposes_internal = resolved_type_exposes_internal(
        construction.result_type(),
        semantic_values,
        symbols,
        declarations,
    );

    match construction.target() {
        ConstructionTarget::Struct(target) => dependencies.push(target.into()),
        ConstructionTarget::UnionVariant(target) => dependencies.push(target.into()),
        ConstructionTarget::TypeForm { callable, .. } => {
            dependencies.push(callable.definition().symbol());

            exposes_internal |= callable_instance_exposes_internal(
                callable,
                semantic_values,
                symbols,
                declarations,
            );
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

    exposes_internal
}

fn push_construction_input(input: ConstructionInputId, dependencies: &mut Vec<AnySymbolId>) {
    let symbol = match input {
        ConstructionInputId::StructField(symbol) => symbol.into(),
        ConstructionInputId::UnionPayloadField(symbol) => symbol.into(),
        ConstructionInputId::CallableParameter(symbol) => symbol.into(),
    };

    dependencies.push(symbol);
}

fn push_construction_default(
    provider: ConstructionDefaultProvider,
    dependencies: &mut Vec<AnySymbolId>,
) {
    let symbol = match provider {
        ConstructionDefaultProvider::StructField(symbol) => symbol.into(),
        ConstructionDefaultProvider::UnionPayload(symbol) => symbol.into(),
        ConstructionDefaultProvider::CallableParameter(symbol) => symbol.into(),
    };

    dependencies.push(symbol);
}

fn push_conversion_dependencies(
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
    root: &bray_bound_tree::SelectedConversion,
    dependencies: &mut Vec<AnySymbolId>,
) -> Result<bool, FactQueryError> {
    let mut pending = vec![root];
    let mut exposes_internal = false;

    while let Some(conversion) = pending.pop() {
        exposes_internal |= resolved_type_exposes_internal(
            conversion.source_type(),
            semantic_values,
            symbols,
            declarations,
        );

        exposes_internal |= resolved_type_exposes_internal(
            conversion.target_type(),
            semantic_values,
            symbols,
            declarations,
        );

        match conversion.target() {
            ConversionTarget::Trait {
                member,
                fulfillment,
                requirement,
                witness,
            } => {
                dependencies.push(member.definition().symbol());
                dependencies.push(fulfillment.definition().symbol());

                exposes_internal |= callable_instance_exposes_internal(
                    *member,
                    semantic_values,
                    symbols,
                    declarations,
                );

                exposes_internal |= callable_instance_exposes_internal(
                    *fulfillment,
                    semantic_values,
                    symbols,
                    declarations,
                );

                exposes_internal |= push_witness_dependencies(
                    semantic_values,
                    symbols,
                    declarations,
                    SelectedImplementationWitness::new(*requirement, *witness),
                    dependencies,
                )?;
            }
            ConversionTarget::Composite(children) => pending.extend(children.iter()),
            ConversionTarget::Identity | ConversionTarget::BuiltInScalar => {}
        }
    }

    Ok(exposes_internal)
}

fn push_witness_dependencies(
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
    witness: SelectedImplementationWitness,
    dependencies: &mut Vec<AnySymbolId>,
) -> Result<bool, FactQueryError> {
    let application = semantic_values
        .trait_application_data(witness.requirement().trait_application())
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    dependencies.push(application.definition().into());

    let application_exposes_internal = source_symbol_is_not_publicly_reachable(
        application.definition().into(),
        declarations,
        symbols,
    ) || substitution_exposes_internal(
        application.substitution(),
        semantic_values,
        symbols,
        declarations,
    );

    let implementation_exposes_internal = implementation_instance_exposes_internal(
        witness.witness(),
        semantic_values,
        symbols,
        declarations,
    );

    Ok(application_exposes_internal || implementation_exposes_internal)
}
