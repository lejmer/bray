use bray_bound_tree::{
    BoundCallResult, BoundExpression, BoundExpressionId, BoundReferenceTarget,
    BoundStructuredExpressionKind, CheckedMemoryOperationKind, CheckedMemoryOperations,
    CheckedSemanticSelections, ConstructionTarget, OperatorTarget, SelectedArgument,
    SelectedConstructionInput, SelectedConversion, SelectedOperation, SemanticSelection,
    StorageIdentity, StoragePlan, StorageProjection,
};
use bray_symbols::AnySymbolId;

use crate::execution_guarantees::{ExecutionDependency, ExecutionProperty};
use crate::{CheckerRequestContext, CheckerUnitView};

pub(super) fn collect_preservation_dependencies(
    occurrences: impl Iterator<Item = bray_bound_tree::BoundExecutionSite>,
    selections: &CheckedSemanticSelections,
    memory: &CheckedMemoryOperations,
    values: &bray_symbols::SemanticValueStore,
    dependencies: &mut Vec<ExecutionDependency>,
) {
    // Retaining entry facts across a pure call depends on that call's proof, even when
    // the enclosing obligation is only a completion predicate or termination promise.
    for occurrence in occurrences {
        let bray_bound_tree::BoundExecutionSite::Node(node) = occurrence else {
            if scoped_invocation_preserves_inputs(selections, values, occurrence) {
                collect_scoped_dependency(
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
            && memory.operation(expression).is_none()
        {
            dependencies.push(ExecutionDependency {
                target: call.target(),
                property: ExecutionProperty::Pure,
                occurrence: node.into(),
            });
        }
    }
}

pub(super) fn scoped_invocation_preserves_inputs(
    selections: &CheckedSemanticSelections,
    values: &bray_symbols::SemanticValueStore,
    occurrence: bray_bound_tree::BoundExecutionSite,
) -> bool {
    let Some(SemanticSelection::ScopedUse(scoped)) = selections.expression(
        occurrence
            .expression()
            .expect("scoped invocation has an expression"),
    ) else {
        panic!("scoped invocation retains its selected lifecycle declarations");
    };

    let data = values.type_data(scoped.invocation(occurrence).1.callable_type());

    let bray_symbols::TypeData::Callable(callable) = data.as_ref() else {
        panic!("scoped invocation signature retains its callable type");
    };

    callable
        .phase_behaviors()
        .invocation()
        .execution_properties()
        .contains(&ExecutionProperty::Pure)
}

pub(super) fn check_expression<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
    property: ExecutionProperty,
    selections: &CheckedSemanticSelections,
    storage: &StoragePlan,
    memory: &CheckedMemoryOperations,
    dependencies: &mut Vec<ExecutionDependency>,
) -> bool {
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
        if property == ExecutionProperty::Total
            && matches!(
                operation.kind(),
                CheckedMemoryOperationKind::Read { .. }
                    | CheckedMemoryOperationKind::Write { .. }
                    | CheckedMemoryOperationKind::Copy { .. }
                    | CheckedMemoryOperationKind::IsNull { .. }
                    | CheckedMemoryOperationKind::Offset { .. }
                    | CheckedMemoryOperationKind::Reinterpret { .. }
                    | CheckedMemoryOperationKind::LayoutQuery { .. }
                    | CheckedMemoryOperationKind::RawBufferCapacity
                    | CheckedMemoryOperationKind::RawBufferInitializedCount
                    | CheckedMemoryOperationKind::RawBufferPointer
                    | CheckedMemoryOperationKind::RawBufferSparePointer { .. }
                    | CheckedMemoryOperationKind::RawBufferSetInitializedCount
            )
        {
            // These checked primitives complete on their required storage domains.
            // Allocation and selected cleanup retain their fallible call dependencies.
            return true;
        }

        return matches!(
            operation.kind(),
            CheckedMemoryOperationKind::SequenceLength
                | CheckedMemoryOperationKind::ByteBufferRead
                | CheckedMemoryOperationKind::Read {
                    kind: bray_bound_tree::MemoryReadKind::Copy,
                    ..
                }
                | CheckedMemoryOperationKind::IsNull { .. }
                | CheckedMemoryOperationKind::Offset { .. }
                | CheckedMemoryOperationKind::Reinterpret { .. }
                | CheckedMemoryOperationKind::LayoutQuery { .. }
                | CheckedMemoryOperationKind::Null { .. }
                | CheckedMemoryOperationKind::Address { .. }
                | CheckedMemoryOperationKind::BorrowFrom { .. }
                | CheckedMemoryOperationKind::UninitPointer { .. }
                | CheckedMemoryOperationKind::RawBufferInitializedSlice
                | CheckedMemoryOperationKind::RawBufferInitializedSliceMut
                | CheckedMemoryOperationKind::RawBufferCapacity
                | CheckedMemoryOperationKind::RawBufferInitializedCount
                | CheckedMemoryOperationKind::RawBufferPointer
                | CheckedMemoryOperationKind::RawBufferSparePointer { .. }
        );
    }

    match selections.expression(expression) {
        Some(SemanticSelection::Call(call)) => {
            // TODO(BRA-500): Certify trait dispatch from its preserved contract and dependencies.
            if call.resolution().trait_dispatch().is_some()
                || !matches!(call.resolution().result(), BoundCallResult::Immediate(_))
                || call
                    .arguments()
                    .iter()
                    .any(|argument| matches!(argument, SelectedArgument::Default { .. }))
            {
                return false;
            }

            for argument in call.arguments() {
                if let SelectedArgument::Explicit { conversion, .. } = argument
                    && !collect_conversion_dependencies(
                        conversion,
                        property,
                        expression,
                        dependencies,
                    )
                {
                    return false;
                }
            }

            if call.implementation_hook()
                == Some(bray_compiler_known::ImplementationHook::NumericTruncate)
                && integer_truncation(request, call)
            {
                return true;
            }

            dependencies.push(ExecutionDependency {
                target: call.target(),
                property,
                occurrence: expression.into(),
            });

            return true;
        }
        Some(SemanticSelection::Operation(operation)) => {
            if !check_operation(operation, property, expression, dependencies) {
                return false;
            }
        }
        Some(SemanticSelection::Propagation(propagation)) => {
            if let Some(conversion) = propagation.error_conversion()
                && !collect_conversion_dependencies(conversion, property, expression, dependencies)
            {
                return false;
            }
        }
        Some(SemanticSelection::Iteration(_) | SemanticSelection::StaticReference(_)) => {
            return false;
        }
        _ => {}
    }

    match bound {
        BoundExpression::Error(_)
        | BoundExpression::ErrorCall(_)
        | BoundExpression::ErrorConversion(_)
        | BoundExpression::UnresolvedReference(_)
        | BoundExpression::BoxConstruction(_)
        | BoundExpression::Await(_)
        | BoundExpression::Generator(_)
        | BoundExpression::For(_) => false,
        BoundExpression::Assignment(_) => property != ExecutionProperty::Pure,
        BoundExpression::Name(name) => !matches!(
            name.target(),
            BoundReferenceTarget::Surface(AnySymbolId::Static(_))
        ),
        BoundExpression::Structured(expression)
            if expression.kind() == BoundStructuredExpressionKind::Panic =>
        {
            property == ExecutionProperty::Total
        }
        BoundExpression::Structured(expression) => !matches!(
            expression.kind(),
            BoundStructuredExpressionKind::GeneralGenerator
                | BoundStructuredExpressionKind::ArrayGenerator
                | BoundStructuredExpressionKind::BooleanAllFold
                | BoundStructuredExpressionKind::BooleanAnyFold
        ),
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
            matches!(
                identity,
                StorageIdentity::Static(_) | StorageIdentity::Error(_)
            )
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
                target: bray_bound_tree::BoundCallableTarget::Declaration(call.callable()),
                property,
                occurrence,
            });
        }
    }

    true
}

fn integer_truncation<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    call: &bray_bound_tree::SelectedCall,
) -> bool {
    let [SelectedArgument::Explicit { conversion, .. }] = call.arguments() else {
        panic!("selected numeric truncation retains its single explicit argument");
    };

    let BoundCallResult::Immediate(target) = call.resolution().result() else {
        panic!("selected numeric truncation returns its result immediately");
    };

    [conversion.target_type(), target].into_iter().all(|ty| {
        crate::representation::type_representation(request, ty)
            .and_then(bray_compiler_known::RepresentationRole::numeric_kind)
            == Some(bray_compiler_known::NumericRepresentationKind::Integer)
    })
}

fn check_operation(
    operation: &SelectedOperation,
    property: ExecutionProperty,
    expression: BoundExpressionId,
    dependencies: &mut Vec<ExecutionDependency>,
) -> bool {
    let mut calls = Vec::new();
    let mut defaults = Vec::new();

    let resolved =
        crate::behavior::collect_operation_behavior(operation, None, &mut calls, &mut defaults);

    if !resolved || !defaults.is_empty() {
        return false;
    }

    dependencies.extend(calls.iter().map(|call| ExecutionDependency {
        target: call.target(),
        property,
        occurrence: expression.into(),
    }));

    match operation {
        SelectedOperation::Operator {
            target: OperatorTarget::BuiltIn(operator),
            ..
        } => property == ExecutionProperty::Pure || !operator.builtin_may_panic(),
        SelectedOperation::Construction(construction) => {
            matches!(construction.target(), ConstructionTarget::TypeForm { .. })
                || construction
                    .inputs()
                    .iter()
                    .all(|input| matches!(input, SelectedConstructionInput::Explicit { .. }))
        }
        SelectedOperation::CompoundAssignment(selection) => {
            property != ExecutionProperty::Pure
                && match selection.target() {
                    OperatorTarget::BuiltIn(operator) => !operator.builtin_may_panic(),
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
        } => property == ExecutionProperty::Pure,
        SelectedOperation::Index { .. } => !calls.is_empty(),
        _ => true,
    }
}

fn collect_conversion_dependencies(
    conversion: &SelectedConversion,
    property: ExecutionProperty,
    expression: BoundExpressionId,
    dependencies: &mut Vec<ExecutionDependency>,
) -> bool {
    let mut calls = Vec::new();
    let resolved = crate::behavior::collect_conversion_behavior(conversion, None, &mut calls);

    dependencies.extend(calls.iter().map(|call| ExecutionDependency {
        target: call.target(),
        property,
        occurrence: expression.into(),
    }));

    resolved
}

pub(super) fn collect_scoped_dependency(
    selections: &CheckedSemanticSelections,
    invocation: bray_bound_tree::BoundExecutionSite,
    property: ExecutionProperty,
    dependencies: &mut Vec<ExecutionDependency>,
) {
    let Some(SemanticSelection::ScopedUse(scoped)) = selections.expression(
        invocation
            .expression()
            .expect("scoped invocation retains its with occurrence"),
    ) else {
        panic!("implicit scoped invocation retains its checked selection");
    };

    let callable = scoped.invocation(invocation).0;

    dependencies.push(ExecutionDependency {
        target: bray_bound_tree::BoundCallableTarget::Declaration(callable),
        property,
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
