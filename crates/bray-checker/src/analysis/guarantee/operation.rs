use bray_bound_tree::{
    BoundCallResult, BoundExpression, BoundExpressionId, BoundReferenceTarget,
    BoundStructuredExpressionKind, CheckedMemoryOperations, CheckedSemanticSelections,
    OperatorTarget, SelectedConversion, SelectedOperation, SemanticSelection, StorageIdentity,
    StoragePlan, StorageProjection,
};
use bray_symbols::AnySymbolId;

use crate::execution_guarantees::{ExecutionDependency, ExecutionProperty};
use crate::{CheckerRequestContext, CheckerUnitView};

pub(super) fn collect_preservation_dependencies(
    occurrences: impl Iterator<Item = bray_bound_tree::BoundExecutionSite>,
    selections: &CheckedSemanticSelections,
    memory: &CheckedMemoryOperations,
    contracts: Option<&crate::TrustedContractInputs>,
    dependencies: &mut Vec<ExecutionDependency>,
) {
    // Retaining entry facts across a pure call depends on that call's proof, even when
    // the enclosing obligation is only a completion predicate or termination promise.
    for occurrence in occurrences {
        let bray_bound_tree::BoundExecutionSite::Node(node) = occurrence else {
            if contracts
                .and_then(|inputs| inputs.calls.get(&occurrence))
                .is_some_and(|contract| contract.preserves_inputs)
                && selections.invoked_callable(occurrence).is_some()
            {
                collect_invocation_dependency(
                    selections,
                    occurrence,
                    ExecutionProperty::Pure,
                    dependencies,
                );
            }

            continue;
        };

        let bray_bound_tree::AnyBoundNodeId::Expression(expression) = node else {
            continue;
        };

        let Some(SemanticSelection::Call(call)) = selections.expression(expression) else {
            continue;
        };

        if call
            .phase_behaviors()
            .invocation()
            .execution_properties()
            .contains(&ExecutionProperty::Pure)
            && !memory.operation(expression).is_some_and(|operation| {
                crate::memory::execution::guarantees(operation.kind(), ExecutionProperty::Pure)
            })
        {
            dependencies.push(ExecutionDependency {
                target: crate::ExecutionDependencyTarget::selected(call),
                obligation: crate::ExecutionObligation::Property(ExecutionProperty::Pure, None),
                occurrence: node.into(),
            });
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum ExecutionGoal {
    Property(ExecutionProperty),
    NoUnwind,
}

pub(super) fn check_expression<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
    goal: ExecutionGoal,
    selections: &CheckedSemanticSelections,
    storage: &StoragePlan,
    flow: Option<&bray_bound_tree::StorageFlow>,
    memory: &CheckedMemoryOperations,
    operation_domain_proven: bool,
    dependencies: &mut Vec<ExecutionDependency>,
) -> bool {
    let property = match goal {
        ExecutionGoal::Property(property) => property,
        ExecutionGoal::NoUnwind => ExecutionProperty::Total,
    };

    let Some(bound) = request.view().expression(expression) else {
        return false;
    };

    if bound.is_recovered() {
        return false;
    }

    if !check_storage_accesses(request, expression.into(), storage, property, dependencies) {
        return false;
    }

    if let Some(operation) = memory.operation(expression) {
        if goal == ExecutionGoal::NoUnwind
            && matches!(
                operation.kind(),
                bray_bound_tree::CheckedMemoryOperationKind::AtomicWait { .. }
            )
        {
            let Some(SemanticSelection::Call(call)) = selections.expression(expression) else {
                panic!("checked atomic wait retains its selected call");
            };

            // Waiting need not terminate, but only its execution requirement can unwind.
            dependencies.push(ExecutionDependency {
                target: crate::ExecutionDependencyTarget::selected(call),
                obligation: crate::ExecutionObligation::Property(property, None),
                occurrence: expression.into(),
            });

            return true;
        }

        return crate::memory::execution::guarantees(operation.kind(), property);
    }

    match selections.expression(expression) {
        Some(SemanticSelection::Call(call)) => {
            if !matches!(call.resolution().result(), BoundCallResult::Immediate(_)) {
                return false;
            }

            dependencies.push(ExecutionDependency {
                target: crate::ExecutionDependencyTarget::selected(call),
                obligation: crate::ExecutionObligation::Property(property, None),
                occurrence: expression.into(),
            });

            return true;
        }
        Some(SemanticSelection::Operation(operation)) => {
            if !check_operation(
                operation,
                property,
                expression,
                operation_domain_proven,
                flow.is_some_and(|flow| {
                    flow.assignment_target(expression).is_some_and(|target| {
                        target.mutation_scope(storage, request.semantic_values())
                            != bray_bound_tree::StorageMutationScope::Unbounded
                    })
                }),
                dependencies,
            ) {
                return false;
            }
        }
        Some(SemanticSelection::Propagation(propagation)) => {
            if let Some(conversion) = propagation.error_conversion()
                && !collect_conversion_dependencies(
                    conversion,
                    property,
                    expression.into(),
                    dependencies,
                )
            {
                return false;
            }
        }
        Some(SemanticSelection::StaticReference(_)) => {
            return request.unit().key().is_static_initializer();
        }
        _ => {}
    }

    match bound {
        BoundExpression::Error(_)
        | BoundExpression::ErrorCall(_)
        | BoundExpression::ErrorConversion(_)
        | BoundExpression::UnresolvedReference(_)
        | BoundExpression::BoxConstruction(_)
        | BoundExpression::Await(_) => false,
        BoundExpression::Assignment(_) => {
            property != ExecutionProperty::Pure
                || flow.is_some_and(|flow| {
                    flow.assignment_target(expression).is_some_and(|target| {
                        target.mutation_scope(storage, request.semantic_values())
                            != bray_bound_tree::StorageMutationScope::Unbounded
                    })
                })
        }
        BoundExpression::Name(name) => {
            request.unit().key().is_static_initializer()
                || !matches!(
                    name.target(),
                    BoundReferenceTarget::Surface(AnySymbolId::Static(_))
                )
        }
        BoundExpression::Structured(expression)
            if expression.kind() == BoundStructuredExpressionKind::Panic =>
        {
            property == ExecutionProperty::Total
        }
        BoundExpression::Structured(_)
        | BoundExpression::For(_)
        | BoundExpression::Generator(_) => true,
        BoundExpression::Call(_)
        | BoundExpression::Unary(_)
        | BoundExpression::Binary(_)
        | BoundExpression::Conversion(_)
        | BoundExpression::StructConstruction(_) => selections.expression(expression).is_some(),
        _ => true,
    }
}

pub(super) fn check_storage_accesses<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    occurrence: bray_bound_tree::BoundExecutionSite,
    storage: &StoragePlan,
    property: ExecutionProperty,
    dependencies: &mut Vec<ExecutionDependency>,
) -> bool {
    for plan in storage.occurrence_plans(occurrence) {
        let Some(identity) = storage.root_identity(plan.access()) else {
            return false;
        };

        if storage.identity(identity).is_none_or(|identity| {
            matches!(identity, StorageIdentity::Error(_))
                || matches!(identity, StorageIdentity::Static(_))
                    && !request.unit().key().is_static_initializer()
        }) {
            return false;
        }

        let Some(path) = storage.resolved_projections(plan.access()) else {
            return false;
        };

        let kind = plan.purpose().projection_borrow_kind();

        for (index, projection) in path.iter().enumerate() {
            if *projection != StorageProjection::OwnedTarget {
                continue;
            }

            let owner = storage
                .access_at(identity, &path[..index])
                .and_then(|access| storage.access(access))
                .map(|access| access.reached_type())
                .map(|ty| request.semantic_values().unborrowed_type(ty));

            let Some(call) = owner.and_then(|owner| storage.owned_borrow(owner, kind)) else {
                return false;
            };

            dependencies.push(ExecutionDependency {
                target: (bray_bound_tree::BoundCallableTarget::Declaration(call.callable())).into(),
                obligation: crate::ExecutionObligation::Property(property, None),
                occurrence,
            });
        }
    }

    true
}

fn check_operation(
    operation: &SelectedOperation,
    property: ExecutionProperty,
    expression: BoundExpressionId,
    operation_domain_proven: bool,
    confined_mutation: bool,
    dependencies: &mut Vec<ExecutionDependency>,
) -> bool {
    let mut calls = Vec::new();
    let mut defaults = Vec::new();

    let resolved =
        crate::behavior::collect_operation_behavior(operation, None, &mut calls, &mut defaults);

    if !resolved {
        return false;
    }

    if !matches!(operation, SelectedOperation::Construction(construction)
        if construction.inputs().iter().any(|input|
            matches!(input, bray_bound_tree::SelectedConstructionInput::Default { .. })))
    {
        dependencies.extend(defaults.into_iter().map(|provider| ExecutionDependency {
            target: crate::ExecutionDependencyTarget::Default(provider),
            obligation: crate::ExecutionObligation::Property(property, None),
            occurrence: expression.into(),
        }));
    }

    dependencies.extend(calls.iter().map(|call| ExecutionDependency {
        target: (call.target()).into(),
        obligation: crate::ExecutionObligation::Property(property, None),
        occurrence: expression.into(),
    }));

    match operation {
        SelectedOperation::Operator {
            target: OperatorTarget::BuiltIn(operator),
            ..
        } => {
            property == ExecutionProperty::Pure
                || !operator.builtin_may_panic()
                || operation_domain_proven
        }
        // Field providers contribute their own selected execution dependencies.
        SelectedOperation::Construction(_) => true,
        SelectedOperation::CompoundAssignment(selection) => {
            (property != ExecutionProperty::Pure || confined_mutation)
                && match selection.target() {
                    OperatorTarget::BuiltIn(operator) => {
                        property == ExecutionProperty::Pure
                            || !operator.builtin_may_panic()
                            || operation_domain_proven
                    }
                    OperatorTarget::Trait { .. } | OperatorTarget::TraitConstraint { .. } => {
                        !calls.is_empty()
                    }
                }
        }
        SelectedOperation::Index {
            target:
                bray_bound_tree::IndexTarget::ArrayElement
                | bray_bound_tree::IndexTarget::SliceElement
                | bray_bound_tree::IndexTarget::ArraySlice
                | bray_bound_tree::IndexTarget::Slice,
            ..
        } => property == ExecutionProperty::Pure || operation_domain_proven,
        SelectedOperation::Index { .. } => !calls.is_empty(),
        _ => true,
    }
}

fn collect_conversion_dependencies(
    conversion: &SelectedConversion,
    property: ExecutionProperty,
    occurrence: bray_bound_tree::BoundExecutionSite,
    dependencies: &mut Vec<ExecutionDependency>,
) -> bool {
    let mut calls = Vec::new();
    let resolved = crate::behavior::collect_conversion_behavior(conversion, None, &mut calls);

    dependencies.extend(calls.iter().map(|call| ExecutionDependency {
        target: call.target().into(),
        obligation: crate::ExecutionObligation::Property(property, None),
        occurrence,
    }));

    resolved
}

pub(super) fn collect_invocation_dependency(
    selections: &CheckedSemanticSelections,
    invocation: bray_bound_tree::BoundExecutionSite,
    property: ExecutionProperty,
    dependencies: &mut Vec<ExecutionDependency>,
) {
    let selection = invocation
        .expression()
        .and_then(|expression| selections.expression(expression));

    let target = match (invocation, selection) {
        (bray_bound_tree::BoundExecutionSite::Node(_), Some(SemanticSelection::Call(call))) => {
            crate::ExecutionDependencyTarget::selected(call)
        }
        _ => {
            let callable = selections
                .invoked_callable(invocation)
                .expect("invocation dependency retains its selected callable");

            let dispatch = match selection {
                Some(SemanticSelection::Operation(operation)) => {
                    match operation.operator_target() {
                        Some(OperatorTarget::TraitConstraint { dispatch, .. }) => Some(dispatch),
                        _ => match operation {
                            SelectedOperation::Index {
                                target:
                                    bray_bound_tree::IndexTarget::TraitConstraint { dispatch, .. },
                                ..
                            } => Some(*dispatch),
                            SelectedOperation::Conversion(conversion) => {
                                match conversion.target() {
                                    bray_bound_tree::ConversionTarget::TraitConstraint {
                                        dispatch,
                                        ..
                                    } => Some(*dispatch),
                                    _ => None,
                                }
                            }
                            _ => None,
                        },
                    }
                }
                _ => None,
            };

            match dispatch {
                Some(dispatch) => {
                    crate::ExecutionDependencyTarget::Requirement { callable, dispatch }
                }
                None => bray_bound_tree::BoundCallableTarget::Declaration(callable).into(),
            }
        }
    };

    dependencies.push(ExecutionDependency {
        target,
        obligation: if property == ExecutionProperty::Total
            && matches!(
                invocation,
                bray_bound_tree::BoundExecutionSite::Iterate(_)
                    | bray_bound_tree::BoundExecutionSite::Next(_)
            ) {
            crate::ExecutionObligation::Termination
        } else {
            crate::ExecutionObligation::Property(property, None)
        },
        occurrence: invocation,
    });
}

pub(super) fn check_construction_admission<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
    property: ExecutionProperty,
    expressions: &bray_bound_tree::CheckedExpressionSemantics,
    storage: &StoragePlan,
    dependencies: &mut Vec<ExecutionDependency>,
    diagnostics: &mut bray_diagnostics::DiagnosticBag,
) -> Result<bool, crate::CheckerQueryError<C::UpstreamError>> {
    // Static product formation admits the materialized owner separately. Actual
    // initializer cleanup still follows its guarded scope operations.
    if request.unit().key().is_static_initializer() {
        return Ok(true);
    }

    let constructed = matches!(expressions.selections().expression(expression), Some(SemanticSelection::Operation(operation)) if matches!(operation, SelectedOperation::Construction(_)) || matches!(operation, SelectedOperation::Member(member) if matches!(member.member(), bray_symbols::AnySymbolId::UnionVariant(_))));

    let republished = bray_bound_tree::storage_expression_republishes_destructor_receiver(
        request.unit(),
        storage,
        expression,
    );

    if !constructed && republished.is_none() {
        return Ok(true);
    }

    let Some(ty) = republished.or_else(|| {
        expressions
            .types()
            .expression(expression)
            .map(|entry| entry.ty())
    }) else {
        return Ok(false);
    };

    let admission = crate::asynchronous::execution_cleanup_dependencies(
        request,
        ty,
        property,
        crate::asynchronous::ExecutionCleanupMode::Admission,
        expression.into(),
    )?;

    let (admission, admission_diagnostics) = admission.into_parts();

    let valid = admission.is_some();

    dependencies.extend(admission.into_iter().flatten());
    diagnostics.add_range(admission_diagnostics);

    Ok(valid)
}

pub(super) fn check_input<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    invocation: bray_bound_tree::BoundExecutionSite,
    selections: &CheckedSemanticSelections,
    storage: &StoragePlan,
    property: ExecutionProperty,
    dependencies: &mut Vec<ExecutionDependency>,
) -> bool {
    let bray_bound_tree::BoundExecutionSite::CallInput {
        expression,
        ordinal,
    } = invocation
    else {
        panic!("input execution checking has its exact occurrence");
    };

    let selection = selections
        .expression(expression)
        .expect("input execution retains its selected invocation");

    let input = selection
        .input(ordinal)
        .expect("input execution retains its exact input");

    let valid = if let Some(provider) = input.default_provider() {
        dependencies.push(ExecutionDependency {
            target: crate::ExecutionDependencyTarget::Default(provider),
            obligation: crate::ExecutionObligation::Property(property, None),
            occurrence: invocation,
        });

        true
    } else if let Some(conversion) = input.conversion() {
        collect_conversion_dependencies(conversion, property, invocation, dependencies)
    } else {
        true
    };

    valid && check_storage_accesses(request, invocation, storage, property, dependencies)
}
