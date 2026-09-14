use std::sync::Arc;

use bray_binder::BindingQueryContext;
use bray_bound_tree::{
    AnyBoundNodeId, BoundExpression, BoundReferenceTarget, BoundSourceAnchor, BoundUnit,
    BoundUnitKey, BoundUnitKind, BoundUnitRoot, CheckedExpressionTypes, StorageAccessId,
    StorageIdentity, StorageIdentityId, StoragePlan,
};
use bray_checker::ConstantReferenceResolution;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_ir::MirTargetContract;
use bray_lowering::{
    CompileTimeUnit, LoweredUnit, LoweringError, LoweringInput, LoweringInputError,
    VerifiedLoweringPlans, executable_unit_kind, lower_unit,
};
use bray_source::SourceSpan;
use bray_symbols::{
    AnySymbolId, ConstantValueId, GenericOwnerId, StaticInstanceTemplateId,
    StaticReferenceSelection, TypeId,
};

use super::Compilation;
use super::binder::generic_parameter_ids;
use super::substitution::identity_substitution;
use crate::fact::{
    CancellationToken, CompilationFactKey, FactQueryError, LocatedLoweringFailure,
    PublishedUnitResult, QueryPriority,
};

type LoweredUnitComputation = (
    DiagnosticResult<Option<LoweredUnit>>,
    Box<[bray_binder::BinderDependency]>,
);

impl Compilation {
    /// Returns the lowering result and dependency diagnostics for one checked semantic unit.
    pub fn lowered_unit(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<Option<LoweredUnit>>>, FactQueryError> {
        let published = self.lowered_unit_with_cancellation(key, &self.state.cancellation)?;

        Ok(Arc::clone(published.result()))
    }

    /// Returns one lowering result for a cancellable prioritized request.
    pub fn lowered_unit_with_priority(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
        priority: QueryPriority,
    ) -> Result<Arc<DiagnosticResult<Option<LoweredUnit>>>, FactQueryError> {
        let published =
            self.lowered_unit_with_cancellation_and_priority(key, cancellation, priority)?;

        Ok(Arc::clone(published.result()))
    }

    fn lowered_unit_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitResult<Option<LoweredUnit>>>, FactQueryError> {
        let priority = self
            .state
            .fact_runtime
            .current_priority()?
            .unwrap_or(QueryPriority::Normal);

        self.lowered_unit_with_cancellation_and_priority(key, cancellation, priority)
    }

    fn lowered_unit_with_cancellation_and_priority(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
        priority: QueryPriority,
    ) -> Result<Arc<PublishedUnitResult<Option<LoweredUnit>>>, FactQueryError> {
        self.unit_query_with_priority(
            &self.state.lowered_units,
            CompilationFactKey::LoweredUnit(key.clone()),
            key.clone(),
            cancellation,
            priority,
            |cancellation| self.compute_lowered_unit(&key, cancellation),
        )
    }

    fn compute_lowered_unit(
        &self,
        key: &BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<LoweredUnitComputation, FactQueryError> {
        let unit = self.bound_unit_with_cancellation(key.clone(), cancellation)?;
        let unit_diagnostics = unit.result().diagnostics().clone();

        if let Some(unit) = CompileTimeUnit::try_new(
            // The lowering result owns the same immutable unit identity independently of binding.
            key.clone(),
        ) {
            return Ok((
                DiagnosticResult::new(Some(LoweredUnit::CompileTime(unit)), unit_diagnostics),
                Box::new([]),
            ));
        }

        let control_flow = self.control_flow_with_cancellation(key.clone(), cancellation)?;

        let expressions = self.expression_semantics_with_cancellation(key.clone(), cancellation)?;
        let patterns = self.patterns_with_cancellation(key.clone(), cancellation)?;
        let storage = self.storage_plan_with_cancellation(key.clone(), cancellation)?;
        let body = self.body_semantics_with_cancellation(key.clone(), cancellation)?;
        let behavior = self.body_behavior_with_cancellation(key.clone(), cancellation)?;

        let (constant_reference_values, constant_reference_diagnostics) =
            self.constant_reference_values(unit.result().value(), cancellation)?;

        let diagnostics = DiagnosticBag::merged_all([
            unit.result().diagnostics(),
            control_flow.result().diagnostics(),
            expressions.result().diagnostics(),
            patterns.result().diagnostics(),
            storage.result().diagnostics(),
            body.result().diagnostics(),
            behavior.result().diagnostics(),
            &constant_reference_diagnostics,
        ]);

        if diagnostics.has_errors() {
            return Ok((DiagnosticResult::new(None, diagnostics), Box::new([])));
        }

        cancellation.check()?;

        let selected_target = self.selected_target().target();

        let target = MirTargetContract::new(
            // MIR owns the immutable target profile independently of compilation state.
            selected_target.profile().clone(),
            selected_target.runtime_abi(),
        );

        let unit_kind = executable_unit_kind(unit.result().value(), &target);

        let static_owner = self.static_lowering_owner(
            key,
            unit.result().value(),
            expressions.result().value().types(),
            cancellation,
        )?;

        let native_static_templates =
            self.native_static_templates(expressions.result().value().selections(), cancellation)?;

        let semantic_values = self.semantic_value_store()?;

        let completed =
            self.completed_unit_cleanup(key, body.result().value().asynchronous(), cancellation)?;

        let lowering_plans = VerifiedLoweringPlans::try_new(
            unit.result().value(),
            storage.result().value(),
            body.result().value().liveness(),
            body.result().value().storage_flow(),
            body.result().value().dependencies(),
            expressions.result().value().selections(),
            self.available_compiler_known_symbols(),
            body.result().value().asynchronous(),
        )
        .and_then(|plans| plans.with_completed_finalizers(completed))
        .map_err(LoweringInputError::from);

        let input = lowering_plans
            .and_then(|lowering_plans| {
                LoweringInput::try_new(
                    unit.result().value(),
                    control_flow.result().value(),
                    expressions.result().value().types(),
                    patterns.result().value(),
                    expressions.result().value().literals(),
                    body.result().value().refinements(),
                    lowering_plans,
                    behavior.result().value(),
                    semantic_values,
                    unit_kind,
                    target,
                )
            })
            .and_then(|input| input.with_constant_reference_values(&constant_reference_values))
            .map_err(|error| {
                let source = lowering_input_failure_source(&error, unit.result().value());

                FactQueryError::LoweringInput(LocatedLoweringFailure::new(error, source))
            })?;

        let input = input.with_native_static_templates(&native_static_templates);

        let input = match static_owner {
            Some((reference, ty)) => input.with_static_owner(reference, ty),
            None => input,
        };

        let span = self
            .state
            .fact_runtime
            .profile()
            .map(|profile| profile.start(crate::profile::ProfileOperation::Lowering, None));

        let result = lower_unit(input);

        if let Some(span) = span {
            span.finish(crate::profile::result_outcome(&result));
        }

        let mir = result.map_err(|error| {
            let source =
                lowering_failure_source(&error, unit.result().value(), storage.result().value());

            FactQueryError::Lowering(LocatedLoweringFailure::new(error, source))
        })?;

        if let Some(profile) = self.state.fact_runtime.profile() {
            profile.record_metric(crate::profile::ProfileMetricKind::MirUnits, 1);

            profile.record_metric(
                crate::profile::ProfileMetricKind::MirBlocks,
                u64::try_from(mir.blocks().len()).unwrap_or(u64::MAX),
            );

            profile.record_metric(
                crate::profile::ProfileMetricKind::MirOperations,
                u64::try_from(mir.operations().len()).unwrap_or(u64::MAX),
            );
        }

        cancellation.check()?;

        Ok((
            DiagnosticResult::new(Some(LoweredUnit::Mir(Box::new(mir))), diagnostics),
            Box::new([]),
        ))
    }

    fn native_static_templates(
        &self,
        selections: &bray_bound_tree::CheckedSemanticSelections,
        cancellation: &CancellationToken,
    ) -> Result<Vec<StaticInstanceTemplateId>, FactQueryError> {
        let mut templates = Vec::new();

        for entry in selections.entries() {
            let bray_bound_tree::SemanticSelection::StaticReference(reference) = entry.selection()
            else {
                continue;
            };

            let declaration = reference.template().declaration();

            let native =
                self.foreign_static_contract_with_cancellation(declaration, cancellation)?;

            let source_address = native.value().as_ref().is_some_and(|contract| {
                contract.direction() == bray_symbols::ForeignCallableDirection::Import
                    || contract.is_mutable()
            });

            let imported_address = self
                .imported_native_boundary_with_cancellation(declaration.into(), cancellation)?
                .is_some_and(|boundary| {
                    matches!(
                        boundary.kind(),
                        bray_package_interface::InterfaceNativeBoundaryKind::Static {
                            mutable: true,
                            ..
                        }
                    ) || boundary.direction() == bray_symbols::ForeignCallableDirection::Import
                });

            if source_address || imported_address {
                templates.push(reference.template());
            }
        }

        templates.sort_unstable();
        templates.dedup();

        Ok(templates)
    }

    fn constant_reference_values(
        &self,
        unit: &BoundUnit,
        cancellation: &CancellationToken,
    ) -> Result<
        (
            Vec<(bray_bound_tree::BoundExpressionId, ConstantValueId)>,
            DiagnosticBag,
        ),
        FactQueryError,
    > {
        let mut values = Vec::new();
        let mut diagnostics = DiagnosticBag::new();

        for (expression, node) in unit.tree().expressions() {
            let BoundExpression::Name(name) = node else {
                continue;
            };

            let BoundReferenceTarget::Surface(symbol) = name.target() else {
                continue;
            };

            let resolved = self.resolve_surface_constant(symbol, cancellation, &mut diagnostics);

            let Some((_, resolution)) = resolved? else {
                continue;
            };

            let value = match resolution {
                ConstantReferenceResolution::Value(value) => value,
                ConstantReferenceResolution::Evaluated(result) => result.value(),
                ConstantReferenceResolution::Term(_)
                | ConstantReferenceResolution::Cycle { .. }
                | ConstantReferenceResolution::Invalid => continue,
            };

            values.push((expression, value));
        }

        values.sort_unstable_by_key(|(expression, _)| *expression);

        Ok((values, diagnostics))
    }

    fn static_lowering_owner(
        &self,
        key: &BoundUnitKey,
        unit: &BoundUnit,
        expression_types: &CheckedExpressionTypes,
        cancellation: &CancellationToken,
    ) -> Result<Option<(StaticReferenceSelection, TypeId)>, FactQueryError> {
        if key.kind() != BoundUnitKind::ConstantTemplate {
            return Ok(None);
        }

        let binding_context = self.binding_context_for(key, cancellation)?;

        let Some(AnySymbolId::Static(declaration)) = binding_context
            .symbols()
            .symbol_for_key(key.declared_owner())
        else {
            return Ok(None);
        };

        if self.static_initializer_key(declaration)?.as_ref() != Some(key) {
            return Ok(None);
        }

        let parameters = generic_parameter_ids(binding_context.symbols(), declaration.into())
            .map_err(super::binder::binding_query_error)?;

        let owner = GenericOwnerId::try_new(declaration.into()).ok_or_else(|| {
            FactQueryError::from(super::product::ProductQueryFailure::missing(
                super::product::ProductQueryContext::Symbol(declaration.into()),
                super::product::ProductDataKind::GenericOwner,
            ))
        })?;

        let substitution =
            identity_substitution(binding_context.semantic_values(), owner, &parameters)?;

        let BoundUnitRoot::Expression(initializer) = unit.root() else {
            return Err(super::semantic_error::SemanticQueryFailure::BoundUnit {
                unit: key.clone(),
                cause: bray_bound_tree::BoundUnitBuildError::RootKindMismatch,
            }
            .into());
        };

        let ty = expression_types
            .expression(initializer)
            .ok_or_else(|| {
                FactQueryError::from(super::product::ProductQueryFailure::missing(
                    super::product::ProductQueryContext::MirUnit(bray_ir::MirUnitKey::Bound(
                        key.clone(),
                    )),
                    super::product::ProductDataKind::ResolvedType,
                ))
            })?
            .ty();

        Ok(Some((
            StaticReferenceSelection::open(
                StaticInstanceTemplateId::new(declaration),
                substitution,
                [],
                self.requested_target().profile().identity().clone(),
            ),
            ty,
        )))
    }
}

fn lowering_input_failure_source(error: &LoweringInputError, unit: &BoundUnit) -> SourceSpan {
    match error {
        LoweringInputError::MissingSemanticSelection(expression)
        | LoweringInputError::MissingExpressionType(expression)
        | LoweringInputError::InvalidStorageOperation(expression) => {
            expression_source(unit, *expression)
        }
        LoweringInputError::InvalidStorageExit(block) => {
            node_source(unit, (*block).into()).unwrap_or_else(|| unit_source(unit))
        }
        LoweringInputError::InvalidPlan(failure) => failure
            .expression()
            .map(|expression| expression_source(unit, expression))
            .or_else(|| failure.exit().and_then(|exit| node_source(unit, exit)))
            .or_else(|| {
                failure
                    .scope()
                    .and_then(|scope| node_source(unit, scope.into()))
            })
            .unwrap_or_else(|| unit_source(unit)),
        LoweringInputError::ForeignInput { .. }
        | LoweringInputError::InputKindMismatch { .. }
        | LoweringInputError::InvalidPatternInput
        | LoweringInputError::InvalidInputContents(_)
        | LoweringInputError::SemanticValue(_)
        | LoweringInputError::StorageOperationCountMismatch { .. }
        | LoweringInputError::LiteralTargetWidthMismatch { .. }
        | LoweringInputError::ExecutableHostRequiresSyntheticInput
        | LoweringInputError::CompileTimeUnitRequiresClassification => unit_source(unit),
    }
}

fn lowering_failure_source(
    error: &LoweringError,
    unit: &BoundUnit,
    storage: &StoragePlan,
) -> SourceSpan {
    match error {
        LoweringError::UnsupportedRoot(root) => {
            node_source(unit, (*root).into()).unwrap_or_else(|| unit_source(unit))
        }
        LoweringError::MissingBoundNode(node) | LoweringError::RecoveredBoundNode(node) => {
            node_source(unit, *node).unwrap_or_else(|| unit_source(unit))
        }
        LoweringError::MissingExpressionType(expression)
        | LoweringError::AwaitOutsideProtectedFrame(expression)
        | LoweringError::MissingSuspensionPoint(expression)
        | LoweringError::InvalidTaskOperation(expression)
        | LoweringError::MissingLiteralValue(expression)
        | LoweringError::MissingSemanticSelection(expression)
        | LoweringError::UnsupportedExpression(expression)
        | LoweringError::UnsupportedOperator { expression, .. }
        | LoweringError::MissingStorageAccess(expression)
        | LoweringError::MissingIterationStorage(expression)
        | LoweringError::MissingOperationResult(expression)
        | LoweringError::MemoryArgumentOrdinalUnrepresentable { expression, .. }
        | LoweringError::MatchArmOrdinalUnrepresentable { expression, .. } => {
            expression_source(unit, *expression)
        }
        LoweringError::UnsupportedPattern(pattern) => {
            node_source(unit, (*pattern).into()).unwrap_or_else(|| unit_source(unit))
        }
        LoweringError::MissingStorageAccessRecord(access)
        | LoweringError::MissingStorageIdentity(access)
        | LoweringError::UnsupportedStorageAccess(access) => {
            access_failure_source(unit, storage, *access)
        }
        LoweringError::MissingStorageIdentityRecord(identity) => {
            identity_source(unit, storage, *identity).unwrap_or_else(|| unit_source(unit))
        }
        LoweringError::MissingCallableResultType
        | LoweringError::MissingRepresentation(_)
        | LoweringError::SemanticValueUnavailable
        | LoweringError::GenericSubstitution(_)
        | LoweringError::SemanticValue(_)
        | LoweringError::InvalidFrameDescriptor(_)
        | LoweringError::Mir(_) => unit_source(unit),
        LoweringError::InvalidCleanupScopeDepth { exit, .. } => {
            node_source(unit, *exit).unwrap_or_else(|| unit_source(unit))
        }
        LoweringError::MissingScopeExitPlan { scope, exit } => node_source(unit, *exit)
            .or_else(|| node_source(unit, (*scope).into()))
            .unwrap_or_else(|| unit_source(unit)),
    }
}

fn expression_source(
    unit: &BoundUnit,
    expression: bray_bound_tree::BoundExpressionId,
) -> SourceSpan {
    node_source(unit, expression.into()).unwrap_or_else(|| unit_source(unit))
}

fn node_source(unit: &BoundUnit, node: AnyBoundNodeId) -> Option<SourceSpan> {
    let view = unit.view();

    let origin = match node {
        AnyBoundNodeId::Expression(expression) => {
            view.expression(expression).map(BoundExpression::origin)
        }
        AnyBoundNodeId::Pattern(pattern) => view.pattern(pattern).map(|pattern| pattern.origin()),
        AnyBoundNodeId::Block(block) => view.block(block).map(|block| block.origin()),
        AnyBoundNodeId::CallableBody(body) => view.callable_body(body).map(|body| body.origin()),
    }?;

    Some(source_span(origin.source_anchor()))
}

fn access_failure_source(
    unit: &BoundUnit,
    storage: &StoragePlan,
    access: StorageAccessId,
) -> SourceSpan {
    if let Some(access) = storage.access(access) {
        return source_span(access.source());
    }

    storage
        .access_plans()
        .iter()
        .find(|plan| plan.access() == access)
        .map(|plan| expression_source(unit, plan.expression()))
        .unwrap_or_else(|| unit_source(unit))
}

fn identity_source(
    unit: &BoundUnit,
    storage: &StoragePlan,
    identity: StorageIdentityId,
) -> Option<SourceSpan> {
    match storage.identity(identity)? {
        StorageIdentity::CompilerCreated(origin) => Some(source_span(origin.source_anchor())),
        StorageIdentity::Error(source) => Some(source_span(source)),
        identity => identity
            .definition_node()
            .and_then(|node| node_source(unit, node)),
    }
}

fn unit_source(unit: &BoundUnit) -> SourceSpan {
    source_span(unit.key().source())
}

fn source_span(source: BoundSourceAnchor) -> SourceSpan {
    let syntax = source.syntax();

    SourceSpan::new(syntax.source_id(), syntax.full_range())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use bray_bound_tree::{
        BoundUnitKey, BoundUnitKind, CheckedMemoryOperationKind, OperatorTarget, SelectedOperation,
        SemanticSelection,
    };
    use bray_compiler_known::ImplementationHook;
    use bray_diagnostics::DiagnosticResult;
    use bray_ir::{
        MirAggregateKind, MirBinaryOperator, MirCallIntrinsic, MirCallTarget, MirImmediateValue,
        MirOperand, MirOperationKind, MirPanicCause, MirProjectionKind, MirStoreKind,
        MirTerminatorKind, MirTextOperationKind, MirUnit, MirValueOrigin,
    };
    use bray_lowering::LoweredUnit;
    use bray_runtime_interface::{ExecutionLaneRequirement, RuntimeAbiVersion};
    use bray_symbols::{BorrowKind, ConstantValueKind, PackageIdentity, ProductKind, TypeData};
    use bray_testing::assert_goal_state_diagnostic_kind;

    use super::Compilation;
    use crate::test_support::{
        compilation, compilation_with_sources_and_worker_budget,
        compilation_with_target_operations, package_identity, source_callable_body_key,
        source_function_body_key, source_input, source_trait_callable_fulfillment_body_key,
        source_type_callable_member_body_key,
    };
    use crate::{
        CancellationToken, CompilationOptions, CompilationRequest, FactQueryError, QueryPriority,
        SelectedTarget, WorkerBudget,
    };

    const LOWERING_SOURCE: &str = concat!(
        "module app;\n",
        "func main() -> i32\n",
        "{\n",
        "    return 1;\n",
        "}\n",
    );

    const UPDATED_LOWERING_SOURCE: &str = concat!(
        "module app;\n",
        "func main() -> i32\n",
        "{\n",
        "    return 2;\n",
        "}\n",
    );

    const STRUCTURED_LOWERING_SOURCE: &str = concat!(
        "module app;\n",
        "\n",
        "struct Pair\n",
        "{\n",
        "    first: i32;\n",
        "    second: i32 = 9;\n",
        "}\n",
        "\n",
        "union Choice\n",
        "{\n",
        "    Value(value: i32);\n",
        "    Empty;\n",
        "}\n",
        "\n",
        "func main() -> i64\n",
        "{\n",
        "    let tuple: (i32, i32) = (1, 2);\n",
        "    let array: [i32; 2] = [3, 4];\n",
        "    let repeated: [i32; 2] = [5; 2];\n",
        "    let pair: Pair = Pair { first = tuple.0, second = array[0] };\n",
        "    let defaulted: Pair = Pair { first = 9 };\n",
        "    let widened: (i64, i64) = tuple as (i64, i64);\n",
        "    let choice: Choice = Value(value = pair.first);\n",
        "    let empty: Choice = Empty;\n",
        "    let owned: box i32 = box(8);\n",
        "    let owned_tuple: (box i32,) = (owned,);\n",
        "    let moved: box i32 = owned_tuple.0;\n",
        "    return pair.first as i64;\n",
        "}\n",
    );

    const CONTROL_LOWERING_SOURCE: &str = concat!(
        "module app;\n",
        "\n",
        "struct Point\n",
        "{\n",
        "    x: i32;\n",
        "    y: i32;\n",
        "}\n",
        "\n",
        "struct Items\n",
        "{\n",
        "}\n",
        "\n",
        "struct ItemsCursor\n",
        "{\n",
        "}\n",
        "\n",
        "impl &Items(Iterable)\n",
        "{\n",
        "    type Element = bool;\n",
        "    type Cursor = ItemsCursor;\n",
        "\n",
        "    consume func iterate() -> ItemsCursor\n",
        "    {\n",
        "        return ItemsCursor {};\n",
        "    }\n",
        "}\n",
        "\n",
        "impl ItemsCursor(Iterator)\n",
        "{\n",
        "    type Element = bool;\n",
        "\n",
        "    mut func next() -> bool?\n",
        "    {\n",
        "        panic();\n",
        "    }\n",
        "}\n",
        "\n",
        "func main()\n",
        "{\n",
        "    let point: Point = Point { x = 1, y = 2 };\n",
        "    let { x, y }: Point = point;\n",
        "    x;\n",
        "    y;\n",
        "\n",
        "    let logical: bool = true && false;\n",
        "    logical;\n",
        "\n",
        "    while false\n",
        "    {\n",
        "        continue;\n",
        "    }\n",
        "\n",
        "    loop\n",
        "    {\n",
        "        break;\n",
        "    }\n",
        "\n",
        "    match make_boolean()\n",
        "    {\n",
        "        case true\n",
        "        {\n",
        "        }\n",
        "        case false\n",
        "        {\n",
        "        }\n",
        "    }\n",
        "\n",
        "    let items: Items = Items {};\n",
        "    let every: bool = all(items);\n",
        "    let some: bool = any(items);\n",
        "\n",
        "    for item in items\n",
        "    {\n",
        "        item;\n",
        "    }\n",
        "\n",
        "    let branch: bool = if true\n",
        "    {\n",
        "        yield true;\n",
        "    }\n",
        "    else\n",
        "    {\n",
        "        yield false;\n",
        "    };\n",
        "}\n",
        "\n",
        "func make_boolean() -> bool\n",
        "{\n",
        "    return true;\n",
        "}\n",
    );

    const GENERATOR_LOWERING_SOURCE: &str = concat!(
        "module app;\n",
        "\n",
        "struct Items\n",
        "{\n",
        "}\n",
        "\n",
        "struct ItemsCursor\n",
        "{\n",
        "}\n",
        "\n",
        "impl &Items(Iterable)\n",
        "{\n",
        "    type Element = bool;\n",
        "    type Cursor = ItemsCursor;\n",
        "\n",
        "    consume func iterate() -> ItemsCursor\n",
        "    {\n",
        "        return ItemsCursor {};\n",
        "    }\n",
        "}\n",
        "\n",
        "impl ItemsCursor(Iterator)\n",
        "{\n",
        "    type Element = bool;\n",
        "\n",
        "    mut func next() -> bool?\n",
        "    {\n",
        "        panic();\n",
        "    }\n",
        "}\n",
        "\n",
        "func main()\n",
        "{\n",
        "    let items: Items = Items {};\n",
        "    let lazy =\n",
        "    {\n",
        "        each item in items\n",
        "        {\n",
        "            if item\n",
        "            {\n",
        "                break;\n",
        "            }\n",
        "\n",
        "            yield item;\n",
        "            yield false;\n",
        "\n",
        "            let nested_items: Items = Items {};\n",
        "\n",
        "            each nested in nested_items\n",
        "            {\n",
        "                false;\n",
        "            }\n",
        "        }\n",
        "    };\n",
        "\n",
        "}\n",
    );

    const FAILURE_LOWERING_SOURCE: &str = concat!(
        "module app;\n",
        "\n",
        "func main()\n",
        "{\n",
        "    assert(true);\n",
        "}\n",
    );

    const DIVERGING_ASSERTION_MESSAGE_SOURCE: &str = concat!(
        "module app;\n",
        "\n",
        "func main()\n",
        "{\n",
        "    assert(false, panic(\"message\"));\n",
        "}\n",
    );

    const TERMINATING_ASSERTION_SOURCE: &str = concat!(
        "module app;\n",
        "\n",
        "func value() -> i32\n",
        "{\n",
        "    assert(false);\n",
        "}\n",
    );

    const RESULT_PROPAGATION_LOWERING_SOURCE: &str = concat!(
        "module app;\n",
        "\n",
        "func main(pos value: Result<i32, i32>) -> Result<i32, i32>\n",
        "{\n",
        "    let unwrapped: i32 = try value;\n",
        "\n",
        "    return value;\n",
        "}\n",
    );

    const CONVERTED_RESULT_PROPAGATION_SOURCE: &str = concat!(
        "module app;\n",
        "\n",
        "func main(\n",
        "    pos value: Result<i32, i32>,\n",
        "    pos fallback: Result<i32, i64>,\n",
        ") -> Result<i32, i64>\n",
        "{\n",
        "    let unwrapped: i32 = try value;\n",
        "\n",
        "    return fallback;\n",
        "}\n",
    );

    const INCOMPATIBLE_RESULT_PROPAGATION_SOURCE: &str = concat!(
        "module app;\n",
        "\n",
        "func main(pos value: Result<i32, i32>) -> i32\n",
        "{\n",
        "    return try value;\n",
        "}\n",
    );

    const INCOMPATIBLE_NULLABLE_PROPAGATION_SOURCE: &str = concat!(
        "module app;\n",
        "\n",
        "func main(pos value: i32?) -> i32\n",
        "{\n",
        "    return value?;\n",
        "}\n",
    );

    const BORROW_LOWERING_SOURCE: &str = concat!(
        "module app;\n",
        "\n",
        "func main()\n",
        "{\n",
        "    let number: i32 = 1;\n",
        "    let reference: &i32 = &number;\n",
        "}\n",
    );

    const UNIT_ROOT_LOWERING_SOURCE: &str = concat!(
        "module app;\n",
        "\n",
        "const value: i32 = 1;\n",
        "\n",
        "func defaults(value: i32 = 1)\n",
        "{\n",
        "}\n",
        "\n",
        "func main()\n",
        "{\n",
        "    lambda() -> i32\n",
        "    {\n",
        "        return 1;\n",
        "    };\n",
        "}\n",
    );

    const CONSTANT_REFERENCE_LOWERING_SOURCE: &str = concat!(
        "module app;\n",
        "\n",
        "const value: i32 = 7;\n",
        "\n",
        "func main() -> i32\n",
        "{\n",
        "    return value;\n",
        "}\n",
    );

    const RANGE_LOWERING_SOURCE: &str = concat!(
        "module app;\n",
        "\n",
        "struct RangeSource\n",
        "{\n",
        "}\n",
        "\n",
        "impl &RangeSource(Iterable)\n",
        "{\n",
        "    type Element = i32;\n",
        "    type Cursor = Range<i32>;\n",
        "\n",
        "    consume func iterate() -> Range<i32>\n",
        "    {\n",
        "        return 1..3;\n",
        "    }\n",
        "}\n",
        "\n",
        "func main() -> i32\n",
        "{\n",
        "    let mut total: i32 = 0;\n",
        "\n",
        "    for value in (0..4)\n",
        "    {\n",
        "        total += value;\n",
        "    }\n",
        "\n",
        "    let source: RangeSource = RangeSource {};\n",
        "\n",
        "    for value in source\n",
        "    {\n",
        "        total += value;\n",
        "    }\n",
        "\n",
        "    let mut cursor: Range<i32> = 4..4;\n",
        "    let exhausted: i32? = cursor(Iterator).next();\n",
        "    let range: Range<i32> = 0..4;\n",
        "    let shared_cursor: Range<i32> = range(Iterable).iterate();\n",
        "    let moved_cursor: Range<i32> = (0..4)(Iterable).iterate();\n",
        "\n",
        "    return total;\n",
        "}\n",
    );

    const ASYNC_LOWERING_SOURCE: &str = concat!(
        "module app;\n",
        "\n",
        "async func main() -> i32\n",
        "{\n",
        "    let retained: i32 = 1;\n",
        "    let pending = child();\n",
        "    let ignored: i32 = await pending;\n",
        "\n",
        "    return retained;\n",
        "}\n",
        "\n",
        "async func child() -> i32\n",
        "{\n",
        "    return 1;\n",
        "}\n",
    );

    #[test]
    fn lowering_results_are_computed_lazily_and_published_once() {
        let compilation = lowering_compilation();
        let key = source_callable_body_key(&compilation);

        assert_eq!(
            compilation.state.lowered_units.is_published(&key),
            Ok(false)
        );

        let first = compilation
            .lowered_unit(key.clone())
            .unwrap_or_else(|error| panic!("MIR must be available: {error:?}"));

        let second = compilation
            .lowered_unit(key.clone())
            .unwrap_or_else(|error| panic!("MIR must remain available: {error:?}"));

        assert!(first.diagnostics().is_empty());
        assert!(first.value().is_some());
        assert!(Arc::ptr_eq(&first, &second));

        assert_eq!(compilation.state.lowered_units.is_published(&key), Ok(true));
    }

    #[test]
    fn half_open_ranges_lower_to_aggregate_cursors_and_direct_advancement() {
        let compilation = compilation(RANGE_LOWERING_SOURCE);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        let lowered = compilation
            .lowered_unit(source_callable_body_key(&compilation))
            .unwrap_or_else(|error| panic!("range MIR must be available: {error:?}"));

        let mir = lowered_mir(&lowered);

        assert!(mir.operations().iter().any(|operation| matches!(
            operation.kind(),
            MirOperationKind::Aggregate(aggregate)
                if aggregate.kind() == MirAggregateKind::Range
        )));

        assert!(
            mir.operations()
                .iter()
                .any(|operation| matches!(operation.kind(), MirOperationKind::Call(_)))
        );

        assert!(mir.blocks().iter().any(|block| matches!(
            block.terminator().kind(),
            MirTerminatorKind::RangeIterate { .. }
        )));

        assert!(
            !mir.blocks().iter().any(|block| matches!(
                block.terminator().kind(),
                MirTerminatorKind::Iterate { .. }
            ))
        );
    }

    #[test]
    fn borrowed_atomic_lock_loop_lowers() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "trusted func wait(pos lock: &core.atomic.Atomic<u32>, pos expected: u32)\n",
            "{\n",
            "}\n",
            "\n",
            "trusted func acquire(pos lock: &core.atomic.Atomic<u32>) -> bool\n",
            "{\n",
            "    let (_, acquired) = core.atomic.compare_exchange<u32, 1, 0>(lock, expected = 0, desired = 1);\n",
            "\n",
            "    if acquired\n",
            "    {\n",
            "        return true;\n",
            "    }\n",
            "\n",
            "    let mut observed: u32 = core.atomic.exchange<u32, 1>(lock, value = 2);\n",
            "\n",
            "    while observed != 0\n",
            "    {\n",
            "        trusted wait(lock, expected = 2);\n",
            "        observed = core.atomic.exchange<u32, 1>(lock, value = 2);\n",
            "    }\n",
            "\n",
            "    return true;\n",
            "}\n",
        ));

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        let key = source_function_body_key(&compilation, "acquire");

        let lowered = compilation
            .lowered_unit(key)
            .unwrap_or_else(|error| panic!("borrowed atomic lock loop must lower: {error:?}"));

        assert!(
            lowered.diagnostics().is_empty(),
            "{:#?}",
            lowered.diagnostics()
        );

        assert!(lowered.value().is_some());
    }

    #[test]
    fn discard_pattern_reads_parameter_initializer() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "func discard(pos value: usize)\n",
            "{\n",
            "    let _: usize = value;\n",
            "}\n",
        ));

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "discard"))
            .unwrap_or_else(|error| panic!("discard-pattern MIR must be available: {error:?}"));

        assert!(lowered.value().is_some(), "{lowered:#?}");

        assert!(
            lowered.diagnostics().is_empty(),
            "{:#?}",
            lowered.diagnostics()
        );
    }

    #[test]
    fn value_producing_blocks_lower_through_a_result_join() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main() -> bool\n",
            "{\n",
            "    return\n",
            "    {\n",
            "        yield true;\n",
            "    };\n",
            "}\n",
        ));

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "main"))
            .unwrap_or_else(|error| panic!("value-producing block must lower: {error:?}"));

        assert!(
            lowered.diagnostics().is_empty(),
            "{:#?}",
            lowered.diagnostics()
        );

        let mir = lowered_mir(&lowered);

        assert_eq!(mir.blocks().len(), 2);

        assert!(matches!(
            mir.blocks()[0].terminator().kind(),
            MirTerminatorKind::Goto(_)
        ));

        assert!(matches!(
            mir.blocks()[1].terminator().kind(),
            MirTerminatorKind::Return(Some(MirOperand::Value(_)))
        ));
    }

    #[test]
    fn shared_string_literal_borrows_lower_as_static_constants() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func text(pos selected: bool) -> &string\n",
            "{\n",
            "    if selected\n",
            "    {\n",
            "        return &\"static text\";\n",
            "    }\n",
            "\n",
            "    return &\"other text\";\n",
            "}\n",
        ));

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "text"))
            .unwrap_or_else(|error| panic!("borrowed literal must lower: {error:?}"));

        assert!(
            lowered.diagnostics().is_empty(),
            "{:#?}",
            lowered.diagnostics()
        );

        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must be available: {error:?}"));

        let constants = lowered_mir(&lowered)
            .operations()
            .iter()
            .filter_map(|operation| {
                let MirOperationKind::Store {
                    value: MirOperand::Constant { value, ty },
                    destination,
                    ..
                } = operation.kind()
                else {
                    return None;
                };

                matches!(
                    lowered_mir(&lowered)
                        .storage(destination.storage())
                        .unwrap()
                        .kind(),
                    bray_ir::MirStorageKind::Return
                )
                .then_some((*value, *ty))
            })
            .collect::<Vec<_>>();

        assert_eq!(constants.len(), 2);

        for (value, ty) in constants {
            let ty = values
                .type_data(ty)
                .unwrap_or_else(|error| panic!("borrow type must be available: {error:?}"));

            let value = values
                .constant_value_data(value)
                .unwrap_or_else(|error| panic!("borrowed literal must be available: {error:?}"));

            let TypeData::Borrow {
                kind: BorrowKind::Shared,
                target,
            } = ty.as_ref()
            else {
                panic!("literal must use a shared borrow representation");
            };

            assert_eq!(value.ty(), *target);
            assert!(matches!(value.kind(), ConstantValueKind::String(_)));
        }
    }

    #[test]
    fn shared_nonliteral_borrows_continue_through_storage() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func accept(pos text: &string)\n",
            "{\n",
            "}\n",
            "\n",
            "func forward(pos text: string)\n",
            "{\n",
            "    accept(&text);\n",
            "}\n",
        ));

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "forward"))
            .unwrap_or_else(|error| panic!("nonliteral borrow must lower: {error:?}"));

        assert!(
            lowered.diagnostics().is_empty(),
            "{:#?}",
            lowered.diagnostics()
        );

        assert!(lowered_mir(&lowered).operations().iter().any(|operation| {
            matches!(
                operation.kind(),
                MirOperationKind::Borrow {
                    kind: BorrowKind::Shared,
                    ..
                }
            )
        }));
    }

    #[test]
    fn lowering_is_deterministic_across_worker_widths() {
        let parallel_budget = WorkerBudget::new(4)
            .unwrap_or_else(|error| panic!("parallel worker budget must build: {error:?}"));

        let serial =
            compilation_with_sources_and_worker_budget(&[LOWERING_SOURCE], WorkerBudget::serial());

        let parallel =
            compilation_with_sources_and_worker_budget(&[LOWERING_SOURCE], parallel_budget);

        let serial_key = source_callable_body_key(&serial);
        let parallel_key = source_callable_body_key(&parallel);

        let serial_unit = serial
            .lowered_unit(serial_key)
            .unwrap_or_else(|error| panic!("serial MIR must publish: {error:?}"));

        let parallel_unit = parallel
            .lowered_unit(parallel_key)
            .unwrap_or_else(|error| panic!("parallel MIR must publish: {error:?}"));

        assert_eq!(serial_unit.diagnostics(), parallel_unit.diagnostics());

        let serial_mir = lowered_mir(&serial_unit);
        let parallel_mir = lowered_mir(&parallel_unit);

        assert_eq!(serial_mir.key(), parallel_mir.key());
        assert_eq!(serial_mir.unit(), parallel_mir.unit());
        assert_eq!(serial_mir.source(), parallel_mir.source());
        assert_eq!(serial_mir.target(), parallel_mir.target());
        assert_eq!(serial_mir.kind(), parallel_mir.kind());
        assert_eq!(serial_mir.entry(), parallel_mir.entry());

        assert_eq!(
            serial_mir.frame_descriptor(),
            parallel_mir.frame_descriptor()
        );

        assert_eq!(serial_mir.storages(), parallel_mir.storages());
        assert_eq!(serial_mir.values(), parallel_mir.values());
        assert_eq!(serial_mir.operations(), parallel_mir.operations());

        let [serial_block] = serial_mir.blocks() else {
            panic!("simple lowering source must produce one MIR block");
        };

        let [parallel_block] = parallel_mir.blocks() else {
            panic!("simple lowering source must produce one MIR block");
        };

        assert_eq!(serial_block.source(), parallel_block.source());
        assert_eq!(serial_block.kind(), parallel_block.kind());
        assert_eq!(serial_block.parameters(), parallel_block.parameters());
        assert_eq!(serial_block.operations(), parallel_block.operations());

        assert_eq!(
            serial_block.terminator().source(),
            parallel_block.terminator().source()
        );

        let (
            MirTerminatorKind::Return(Some(MirOperand::Constant {
                value: serial_value,
                ty: serial_type,
            })),
            MirTerminatorKind::Return(Some(MirOperand::Constant {
                value: parallel_value,
                ty: parallel_type,
            })),
        ) = (
            serial_block.terminator().kind(),
            parallel_block.terminator().kind(),
        )
        else {
            panic!("simple lowering source must return one constant");
        };

        let serial_values = serial
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("serial values must be available: {error:?}"));

        let parallel_values = parallel
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("parallel values must be available: {error:?}"));

        let serial_type = serial_values
            .type_data(*serial_type)
            .unwrap_or_else(|error| panic!("serial type must be available: {error:?}"));

        let parallel_type = parallel_values
            .type_data(*parallel_type)
            .unwrap_or_else(|error| panic!("parallel type must be available: {error:?}"));

        let (
            TypeData::Named {
                definition: serial_definition,
                substitution: serial_substitution,
            },
            TypeData::Named {
                definition: parallel_definition,
                substitution: parallel_substitution,
            },
        ) = (serial_type.as_ref(), parallel_type.as_ref())
        else {
            panic!("simple lowering source must return one named integer type");
        };

        assert_eq!(serial_definition, parallel_definition);

        assert_eq!(
            serial_values.generic_substitution_data(*serial_substitution),
            parallel_values.generic_substitution_data(*parallel_substitution)
        );

        let serial_constant = serial_values
            .constant_value_data(*serial_value)
            .unwrap_or_else(|error| panic!("serial constant must be available: {error:?}"));

        let parallel_constant = parallel_values
            .constant_value_data(*parallel_value)
            .unwrap_or_else(|error| panic!("parallel constant must be available: {error:?}"));

        assert_eq!(serial_constant.kind(), parallel_constant.kind());
    }

    #[test]
    fn cancelled_mir_requests_publish_no_partial_unit() {
        let compilation = lowering_compilation();
        let key = source_callable_body_key(&compilation);
        let cancellation = CancellationToken::new();

        cancellation.cancel();

        let result = compilation.lowered_unit_with_priority(
            key.clone(),
            &cancellation,
            QueryPriority::Normal,
        );

        assert_eq!(result, Err(FactQueryError::Cancelled));

        assert_eq!(
            compilation.state.lowered_units.is_published(&key),
            Ok(false)
        );

        assert!(compilation.lowered_unit(key).is_ok());
    }

    #[test]
    fn compile_time_units_are_classified_without_demanding_runtime_queries() {
        let compilation = compilation(UNIT_ROOT_LOWERING_SOURCE);
        let key = declared_unit_key(&compilation, BoundUnitKind::ConstantTemplate);

        assert_eq!(
            compilation.state.checked_control_flow.is_published(&key),
            Ok(false)
        );

        assert_eq!(
            compilation.state.expression_semantics.is_published(&key),
            Ok(false)
        );

        let result = compilation
            .lowered_unit(key.clone())
            .unwrap_or_else(|error| panic!("compile-time classification must publish: {error:?}"));

        let Some(LoweredUnit::CompileTime(unit)) = result.value() else {
            panic!("constant templates must be classified as compile-time-only");
        };

        assert_eq!(unit.key(), &key);

        assert_eq!(
            compilation.state.checked_control_flow.is_published(&key),
            Ok(false)
        );

        assert_eq!(
            compilation.state.expression_semantics.is_published(&key),
            Ok(false)
        );
    }

    #[test]
    fn runtime_default_expression_roots_lower_to_mir() {
        let compilation = compilation(UNIT_ROOT_LOWERING_SOURCE);
        let key = declared_unit_key(&compilation, BoundUnitKind::RuntimeDefault);

        let result = compilation
            .lowered_unit(key)
            .unwrap_or_else(|error| panic!("runtime default MIR must publish: {error:?}"));

        assert!(matches!(
            lowered_mir(&result)
                .blocks()
                .last()
                .map(|block| block.terminator().kind()),
            Some(MirTerminatorKind::Return(Some(MirOperand::Constant { .. })))
        ));
    }

    #[test]
    fn runtime_default_borrows_caller_owned_input_storage() {
        let compilation = compilation(
            r#"
            module app;

            func observe(pos first: bool, second: &bool = &first)
            {
            }
        "#,
        );

        let key = declared_unit_key(&compilation, BoundUnitKind::RuntimeDefault);

        let result = compilation
            .lowered_unit(key)
            .unwrap_or_else(|error| panic!("runtime default MIR must publish: {error:?}"));

        let mir = lowered_mir(&result);

        assert!(mir.storages_with_ids().any(|(_, storage)| matches!(
            storage.kind(),
            bray_ir::MirStorageKind::BorrowedParameter(0)
        )));

        assert!(
            !mir.storages_with_ids().any(|(_, storage)| matches!(
                storage.kind(),
                bray_ir::MirStorageKind::Parameter(_)
            ))
        );
    }

    #[test]
    fn constant_references_lower_to_closed_mir_operands() {
        let compilation = compilation(CONSTANT_REFERENCE_LOWERING_SOURCE);
        let key = source_callable_body_key(&compilation);

        let result = compilation
            .lowered_unit(key)
            .unwrap_or_else(|error| panic!("constant reference MIR must publish: {error:?}"));

        assert!(matches!(
            lowered_mir(&result)
                .blocks()
                .last()
                .map(|block| block.terminator().kind()),
            Some(MirTerminatorKind::Return(Some(MirOperand::Constant { .. })))
        ));
    }

    #[test]
    fn anonymous_callable_values_and_nested_bodies_lower_independently() {
        let compilation = compilation(UNIT_ROOT_LOWERING_SOURCE);

        let (outer, nested) = compilation
            .declared_unit_keys_for_test()
            .unwrap_or_else(|error| panic!("declared units must be available: {error:?}"))
            .into_iter()
            .filter(|key| key.kind() == BoundUnitKind::CallableBody)
            .find_map(|key| {
                let unit = compilation.bound_unit(key.clone()).ok()?;
                let nested = unit.value().nested_units().first()?.clone();

                Some((key, nested))
            })
            .unwrap_or_else(|| panic!("test source must contain one nested anonymous callable"));

        let outer_result = compilation
            .lowered_unit(outer)
            .unwrap_or_else(|error| panic!("outer callable MIR must publish: {error:?}"));

        assert!(
            lowered_mir(&outer_result)
                .operations()
                .iter()
                .any(|operation| matches!(
                    operation.kind(),
                    MirOperationKind::AnonymousCallable(
                        bray_ir::MirAnonymousCallableReference::Bound(key)
                    ) if key == &nested
                ))
        );

        let nested_result = compilation
            .lowered_unit(nested.clone())
            .unwrap_or_else(|error| panic!("nested callable MIR must publish: {error:?}"));

        assert!(matches!(
            lowered_mir(&nested_result).key(),
            bray_ir::MirUnitKey::Bound(key) if key == &nested
        ));
    }

    #[test]
    fn async_callables_lower_to_protected_frames_and_explicit_suspension() {
        let compilation = compilation(ASYNC_LOWERING_SOURCE);
        let key = source_callable_body_key(&compilation);

        let result = compilation
            .lowered_unit(key)
            .unwrap_or_else(|error| panic!("async MIR must publish: {error:?}"));

        let mir = lowered_mir(&result);

        assert!(matches!(
            mir.kind(),
            bray_ir::MirUnitKind::ProtectedAsyncFrame(_)
        ));

        let Some(frame) = mir.frame_descriptor() else {
            panic!("async callable MIR must carry a protected-frame descriptor");
        };

        assert_eq!(frame.states().len(), 2);
        assert!(!frame.states()[1].initialized_storages().is_empty());

        assert!(mir.operations().iter().any(|operation| matches!(
            operation.kind(),
            MirOperationKind::Async(bray_ir::MirAsyncOperation::CreateFrame { .. })
        )));

        assert!(
            mir.blocks().iter().any(|block| matches!(
                block.terminator().kind(),
                MirTerminatorKind::Suspend { .. }
            ))
        );

        assert!(mir.operations().iter().any(|operation| matches!(
            operation.kind(),
            MirOperationKind::Async(bray_ir::MirAsyncOperation::PublishTerminalState {
                state: bray_ir::MirTaskTerminalState::Cancelled,
                ..
            })
        )));
    }

    #[test]
    fn partial_array_cleanup_flags_survive_suspension() {
        let compilation = compilation(
            r#"module app;
struct Guard { destruct() {} }
func take(pos value: Guard) {}
async func partial(pos values: [[Guard; 2]; 2], pos index: usize, pos pending: Future<i32>)
{
    take(values[index][0]);
    let _ = await pending;
}
"#,
        );

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );

        let result = compilation
            .lowered_unit(source_function_body_key(&compilation, "partial"))
            .unwrap();

        let mir = lowered_mir(&result);
        let frame = mir.frame_descriptor().unwrap();
        let values = compilation.semantic_value_store().unwrap();

        let boolean = compilation
            .available_compiler_known_symbols()
            .representation_symbol::<bray_symbols::StructSymbolId>(
                bray_compiler_known::RepresentationRole::ScalarBool,
            )
            .unwrap();

        let guards = mir
            .storages_with_ids()
            .filter_map(|(id, storage)| {
                if storage.kind() != &bray_ir::MirStorageKind::Local {
                    return None;
                }

                let mut ty = storage.ty();
                let mut dimensions = 0;

                loop {
                    let data = values.type_data(ty).unwrap();

                    match data.as_ref() {
                        TypeData::Array { element, .. } => {
                            ty = *element;
                            dimensions += 1;
                        }
                        TypeData::Named {
                            definition: bray_symbols::NamedTypeSymbolId::Struct(definition),
                            ..
                        } if *definition == boolean => return Some((id, dimensions)),
                        _ => return None,
                    }
                }
            })
            .collect::<Vec<_>>();

        assert!(
            guards.iter().any(|(_, dimensions)| *dimensions == 2),
            "nested array flags must remain represented: {mir:?}"
        );

        assert!(frame.states().len() > 1);

        for state in frame.states().iter().skip(1) {
            assert!(
                guards
                    .iter()
                    .all(|(guard, _)| state.initialized_storages().contains(guard)),
                "all entry-initialized flags must survive suspension: {frame:?}"
            );
        }

        assert!(
            mir.blocks()
                .iter()
                .filter(|block| block.kind() != bray_ir::MirBlockKind::Ordinary)
                .all(|block| !matches!(
                    block.terminator().kind(),
                    MirTerminatorKind::Suspend { .. }
                )),
            "cleanup array loops must not suspend with an unretained counter: {mir:?}"
        );
    }

    #[test]
    fn async_callable_frames_retain_declared_execution_lanes() {
        let compilation = compilation(concat!(
            "module app;\n",
            "async func wait()\n",
            "    requires(blocking_execution())\n",
            "{\n",
            "}\n",
        ));

        let result = compilation
            .lowered_unit(source_callable_body_key(&compilation))
            .unwrap_or_else(|error| panic!("blocking async MIR must publish: {error:?}"));

        let frame = lowered_mir(&result)
            .frame_descriptor()
            .unwrap_or_else(|| panic!("async callable must publish a frame descriptor"));

        assert!(
            frame
                .states()
                .iter()
                .all(|state| { state.lane_requirements() == [ExecutionLaneRequirement::Blocking] })
        );
    }

    #[test]
    fn generic_union_returns_preserve_the_callable_result_through_cleanup() {
        let compilation = compilation(
            r#"module app;

union Choice<T>
{
    Value(pos value: T);
    Empty;
}

struct Guard
{
    destruct() {}
}

struct Receiver<T>
{
    async func receive(pos value: T) -> Choice<T>
    {
        let guard: Guard = Guard {};

        return Choice.Empty;
    }
}
"#,
        );

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        let lowered = compilation
            .lowered_unit(source_type_callable_member_body_key(
                &compilation,
                "receive",
            ))
            .unwrap_or_else(|error| panic!("generic union return must lower: {error:?}"));

        assert!(lowered.value().is_some(), "{lowered:#?}");
    }

    #[test]
    fn updated_snapshots_reuse_only_target_and_source_compatible_mir() {
        let previous = lowering_compilation();
        let key = source_callable_body_key(&previous);

        let previous_mir = previous
            .lowered_unit(key.clone())
            .unwrap_or_else(|error| panic!("initial MIR must be available: {error:?}"));

        let parallel = WorkerBudget::new(2)
            .unwrap_or_else(|error| panic!("parallel worker budget must build: {error:?}"));

        let worker_update = previous
            .updated(lowering_request(
                LOWERING_SOURCE,
                0,
                CompilationOptions::new(parallel, ProductKind::Library, SelectedTarget::baseline()),
            ))
            .unwrap_or_else(|error| panic!("worker-budget update must load: {error:?}"));

        let worker_mir = worker_update
            .lowered_unit(key.clone())
            .unwrap_or_else(|error| panic!("reused MIR must be available: {error:?}"));

        assert!(Arc::ptr_eq(&previous_mir, &worker_mir));

        let baseline = SelectedTarget::baseline();

        let revised_target =
            SelectedTarget::new(baseline.profile().clone(), RuntimeAbiVersion::new(1, 1));

        let target_update = previous
            .updated(lowering_request(
                LOWERING_SOURCE,
                0,
                CompilationOptions::new(
                    WorkerBudget::serial(),
                    ProductKind::Library,
                    revised_target,
                ),
            ))
            .unwrap_or_else(|error| panic!("target update must load: {error:?}"));

        let target_mir = target_update
            .lowered_unit(key)
            .unwrap_or_else(|error| panic!("target-specific MIR must be available: {error:?}"));

        assert!(!Arc::ptr_eq(&previous_mir, &target_mir));

        let source_update = previous
            .updated(lowering_request(
                UPDATED_LOWERING_SOURCE,
                1,
                CompilationOptions::default(),
            ))
            .unwrap_or_else(|error| panic!("source update must load: {error:?}"));

        let source_key = source_callable_body_key(&source_update);

        let source_mir = source_update
            .lowered_unit(source_key)
            .unwrap_or_else(|error| panic!("revised source MIR must be available: {error:?}"));

        assert!(!Arc::ptr_eq(&previous_mir, &source_mir));
    }

    #[test]
    fn structured_values_lower_to_aggregates_construction_and_projections() {
        let compilation = compilation(STRUCTURED_LOWERING_SOURCE);
        let key = source_callable_body_key(&compilation);

        let result = compilation
            .lowered_unit(key)
            .unwrap_or_else(|error| panic!("structured MIR must be available: {error:?}"));

        let mir = lowered_mir(&result);

        let aggregate_kinds = mir.operations().iter().filter_map(|operation| {
            let MirOperationKind::Aggregate(aggregate) = operation.kind() else {
                return None;
            };

            Some(aggregate.kind())
        });

        assert_eq!(
            aggregate_kinds.collect::<Vec<_>>(),
            [
                MirAggregateKind::Tuple,
                MirAggregateKind::Array,
                MirAggregateKind::RepeatedArray,
                MirAggregateKind::Tuple,
            ]
        );

        assert!(
            mir.operations()
                .iter()
                .any(|operation| matches!(operation.kind(), MirOperationKind::Construct(_)))
        );

        let construction_targets = mir.operations().iter().filter_map(|operation| {
            let MirOperationKind::Construct(construction) = operation.kind() else {
                return None;
            };

            Some(construction.target())
        });

        assert_eq!(
            construction_targets
                .filter(|target| matches!(
                    target,
                    bray_bound_tree::ConstructionTarget::UnionVariant(_)
                ))
                .count(),
            2
        );

        assert!(mir.operations().iter().any(|operation| matches!(
            operation.kind(),
            MirOperationKind::Construct(construction)
                if matches!(
                    construction.target(),
                    bray_bound_tree::ConstructionTarget::TypeForm { .. }
                )
        )));

        assert!(mir.operations().iter().any(|operation| {
            let MirOperationKind::Construct(construction) = operation.kind() else {
                return false;
            };

            construction
                .inputs()
                .iter()
                .any(|input| matches!(input, bray_ir::MirConstructionInput::Default { .. }))
        }));

        assert!(mir.operations().iter().any(|operation| {
            let MirOperationKind::Construct(construction) = operation.kind() else {
                return false;
            };

            construction.inputs().iter().all(|input| {
                matches!(
                    input,
                    bray_ir::MirConstructionInput::Explicit {
                        value: MirOperand::Copy(place),
                        ..
                    } if !place.projections().is_empty()
                )
            })
        }));

        assert!(mir.operations().iter().any(|operation| matches!(
            operation.kind(),
            MirOperationKind::Store {
                value: MirOperand::Move(place),
                ..
            } if !place.projections().is_empty()
        )));

        assert!(mir.operations().iter().any(|operation| matches!(
            operation.kind(),
            MirOperationKind::Convert { conversion, .. }
                if conversion.source_type() != conversion.target_type()
        )));

        assert!(mir.operations().iter().any(|operation| matches!(
            operation.kind(),
            MirOperationKind::Convert { conversion, .. }
                if matches!(
                    conversion.target(),
                    bray_bound_tree::ConversionTarget::Composite(_)
                )
        )));

        let returns = mir
            .blocks()
            .iter()
            .filter_map(|block| match block.terminator().kind() {
                MirTerminatorKind::Return(value) => Some(value),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert!(
            matches!(returns.as_slice(), [Some(MirOperand::Move(place))] if mir.storage(place.storage()).unwrap().kind() == &bray_ir::MirStorageKind::Return),
            "cleanup continuations must preserve the single value return: {mir:?}"
        );
    }

    #[test]
    fn checked_control_patterns_and_iteration_lower_to_explicit_mir() {
        let compilation = compilation(CONTROL_LOWERING_SOURCE);
        let key = source_callable_body_key(&compilation);

        let result = compilation
            .lowered_unit(key)
            .unwrap_or_else(|error| panic!("control MIR must be available: {error:?}"));

        let mir = lowered_mir(&result);

        assert!(mir.blocks().iter().any(|block| matches!(
            block.terminator().kind(),
            MirTerminatorKind::PatternBranch { .. }
        )));

        assert!(
            mir.blocks().iter().any(|block| matches!(
                block.terminator().kind(),
                MirTerminatorKind::Iterate { .. }
            ))
        );

        assert!(
            mir.blocks()
                .iter()
                .any(|block| matches!(block.terminator().kind(), MirTerminatorKind::Branch { .. }))
        );

        assert!(mir.operations().iter().any(|operation| {
            let mut projected_move = false;

            operation.kind().for_each_operand(|operand| {
                projected_move |= matches!(operand, MirOperand::Move(place) if !place.projections().is_empty());
            });

            projected_move
        }));

        assert!(mir.operations().iter().any(|operation| {
            let MirOperationKind::Call(call) = operation.kind() else {
                return false;
            };

            !call.witnesses().is_empty()
        }));
    }

    #[test]
    fn custom_indexing_lowers_shared_and_mutable_access_as_places() {
        let compilation = compilation(
            r#"module app;

struct Item
{
    mut value: i32;
}

struct Values
{
    mut first: Item;
    mut second: Item;
}

impl Values(ElementIndex<i32>)
{
    type Output = Item;

    func index(pos selector: &i32) -> &Item
    {
        return &self.first;
    }
}

impl Values(MutableElementIndex<i32>)
{
    type Output = Item;

    mut func index(pos selector: &i32) -> &mut Item
    {
        return &mut self.second;
    }
}

impl Values(SliceIndex<i32>)
{
    type Output = Item;

    func slice(pos start: i32?, pos end: i32?) -> &Item
    {
        return &self.first;
    }
}

func observe(pos values: Values) -> i32
{
    return values[0].value;
}

func borrow_shared(pos values: Values)
{
    let selected: &Item = &values[0];
}

func borrow_mutable(pos input: Values)
{
    let mut values: Values = input;
    let selected: &mut Item = &mut values[0];
}

func assign(pos input: Values)
{
    let mut values: Values = input;
    values[0] = Item { value = 3 };
}

func nested_assign(pos input: Values)
{
    let mut values: Values = input;
    values[0].value = 4;
}

func compound_assign(pos input: Values)
{
    let mut values: Values = input;
    values[0].value += 1;
}

func lower_only(pos values: Values) -> i32
{
    return values[1..].value;
}

func upper_only(pos values: Values) -> i32
{
    return values[..2].value;
}

func both_bounds(pos values: Values) -> i32
{
    return values[1..2].value;
}

"#,
        );

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        for function in [
            "observe",
            "borrow_shared",
            "borrow_mutable",
            "assign",
            "nested_assign",
            "compound_assign",
            "lower_only",
            "upper_only",
            "both_bounds",
        ] {
            let result = compilation
                .lowered_unit(source_function_body_key(&compilation, function))
                .unwrap_or_else(|error| panic!("{function} MIR must be available: {error:?}"));

            let mir = lowered_mir(&result);

            let protocol_calls = mir
                .operations()
                .iter()
                .filter(|operation| {
                    matches!(operation.kind(), MirOperationKind::Call(call) if !call.witnesses().is_empty())
                })
                .count();

            assert_eq!(protocol_calls, 1, "{function}: {mir:#?}");
        }

        for (function, kind) in [
            ("borrow_shared", BorrowKind::Shared),
            ("borrow_mutable", BorrowKind::Mutable),
        ] {
            let result = compilation
                .lowered_unit(source_function_body_key(&compilation, function))
                .unwrap_or_else(|error| panic!("{function} MIR must be available: {error:?}"));

            let mir = lowered_mir(&result);

            assert!(
                mir.operations().iter().any(|operation| matches!(
                    operation.kind(),
                    MirOperationKind::Borrow { kind: actual, place }
                        if *actual == kind
                            && matches!(
                                place.projections().first().map(bray_ir::MirProjection::kind),
                                Some(MirProjectionKind::Dereference)
                            )
                )),
                "{function}: {mir:#?}"
            );
        }

        for function in ["assign", "nested_assign", "compound_assign"] {
            let assignment = compilation
                .lowered_unit(source_function_body_key(&compilation, function))
                .unwrap_or_else(|error| panic!("{function} MIR must be available: {error:?}"));

            let assignment = lowered_mir(&assignment);

            assert!(
                assignment.operations().iter().any(|operation| matches!(
                    operation.kind(),
                    MirOperationKind::Store {
                        kind: MirStoreKind::Assign,
                        destination,
                        ..
                    } if matches!(
                        destination.projections().first().map(bray_ir::MirProjection::kind),
                        Some(MirProjectionKind::Dereference)
                    )
                )),
                "{function}: {assignment:#?}"
            );
        }

        let lower_only = compilation
            .lowered_unit(source_function_body_key(&compilation, "lower_only"))
            .unwrap_or_else(|error| panic!("lower-only MIR must be available: {error:?}"));

        let upper_only = compilation
            .lowered_unit(source_function_body_key(&compilation, "upper_only"))
            .unwrap_or_else(|error| panic!("upper-only MIR must be available: {error:?}"));

        let both_bounds = compilation
            .lowered_unit(source_function_body_key(&compilation, "both_bounds"))
            .unwrap_or_else(|error| panic!("both-bound MIR must be available: {error:?}"));

        let lower_only = lowered_mir(&lower_only);
        let upper_only = lowered_mir(&upper_only);
        let both_bounds = lowered_mir(&both_bounds);

        let [_, lower_start, lower_end] = custom_index_call_arguments(lower_only) else {
            panic!("lower-only call must retain receiver, start, and end arguments");
        };

        let [_, upper_start, upper_end] = custom_index_call_arguments(upper_only) else {
            panic!("upper-only call must retain receiver, start, and end arguments");
        };

        let [_, both_start, both_end] = custom_index_call_arguments(both_bounds) else {
            panic!("both-bound call must retain receiver, start, and end arguments");
        };

        let lower_start = explicit_call_operand(lower_start);
        let lower_end = explicit_call_operand(lower_end);
        let upper_start = explicit_call_operand(upper_start);
        let upper_end = explicit_call_operand(upper_end);
        let both_start = explicit_call_operand(both_start);
        let both_end = explicit_call_operand(both_end);

        let lower_payload = nullable_payload(lower_only, lower_start)
            .unwrap_or_else(|| panic!("lower-only start must be present"));

        let upper_payload = nullable_payload(upper_only, upper_end)
            .unwrap_or_else(|| panic!("upper-only end must be present"));

        assert_eq!(nullable_payload(lower_only, lower_end), None);
        assert_eq!(nullable_payload(upper_only, upper_start), None);

        assert_eq!(
            nullable_payload(both_bounds, both_start),
            Some(lower_payload)
        );

        assert_eq!(nullable_payload(both_bounds, both_end), Some(upper_payload));

        assert_ne!(lower_payload, upper_payload);
    }

    #[test]
    fn else_if_conditions_lower_to_short_circuit_branches() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main(pos first: bool, pos second: bool)\n",
            "{\n",
            "    if first\n",
            "    {\n",
            "    }\n",
            "    else if second\n",
            "    {\n",
            "    }\n",
            "    else\n",
            "    {\n",
            "    }\n",
            "}\n",
        ));

        let result = compilation
            .lowered_unit(source_callable_body_key(&compilation))
            .unwrap_or_else(|error| panic!("else-if MIR must be available: {error:?}"));

        let branch_count = lowered_mir(&result)
            .blocks()
            .iter()
            .filter(|block| matches!(block.terminator().kind(), MirTerminatorKind::Branch { .. }))
            .count();

        assert_eq!(branch_count, 2);
        assert!(result.diagnostics().is_empty(), "{result:?}");
    }

    #[test]
    fn contextual_variant_patterns_do_not_require_superseded_binding_candidates() {
        let compilation = compilation(concat!(
            "module app;\n",
            "union Choice\n",
            "{\n",
            "    First;\n",
            "}\n",
            "func main()\n",
            "{\n",
            "    match make_choice()\n",
            "    {\n",
            "        case First\n",
            "        {\n",
            "        }\n",
            "    }\n",
            "}\n",
            "func make_choice() -> Choice\n",
            "{\n",
            "    return First;\n",
            "}\n",
        ));

        let result = compilation
            .lowered_unit(source_function_body_key(&compilation, "main"))
            .unwrap_or_else(|error| panic!("contextual variant match must lower: {error:?}"));

        assert!(result.value().is_some(), "{:#?}", result.diagnostics());

        assert!(
            result.diagnostics().is_empty(),
            "{:#?}",
            result.diagnostics()
        );
    }

    #[test]
    fn consumed_match_subject_uses_the_unprojected_union_storage() {
        let compilation = compilation(concat!(
            "module app;\n",
            "union Outcome\n",
            "{\n",
            "    Value(pos value: usize);\n",
            "    Error;\n",
            "}\n",
            "func main(input: Outcome) -> usize\n",
            "{\n",
            "    match consume input\n",
            "    {\n",
            "        case Outcome.Value(value)\n",
            "        {\n",
            "            return value;\n",
            "        }\n",
            "\n",
            "        case Outcome.Error\n",
            "        {\n",
            "            return 0;\n",
            "        }\n",
            "    }\n",
            "}\n",
        ));

        let result = compilation
            .lowered_unit(source_callable_body_key(&compilation))
            .unwrap_or_else(|error| panic!("consumed union match must lower: {error:?}"));

        assert!(
            result.diagnostics().is_empty(),
            "{:?}",
            result.diagnostics()
        );

        lowered_mir(&result);
    }

    #[test]
    fn checked_general_generators_lower_to_accumulation_operations() {
        let compilation = compilation(GENERATOR_LOWERING_SOURCE);
        let key = source_callable_body_key(&compilation);

        let result = compilation
            .lowered_unit(key)
            .unwrap_or_else(|error| panic!("generator MIR must be available: {error:?}"));

        let mir = lowered_mir(&result);

        let operations = mir
            .operations()
            .iter()
            .filter_map(|operation| match operation.kind() {
                MirOperationKind::Generator(operation) => Some(operation),
                _ => None,
            })
            .collect::<Vec<_>>();

        let [
            bray_ir::MirGeneratorOperation::Begin {
                kind,
                destination: begin,
                exact_count: None,
                ..
            },
            bray_ir::MirGeneratorOperation::Push {
                destination: push, ..
            },
            bray_ir::MirGeneratorOperation::Push {
                destination: second_push,
                ..
            },
            bray_ir::MirGeneratorOperation::Finish {
                destination: finish,
            },
        ] = operations.as_slice()
        else {
            panic!("general generators must initialize, push, and finish in order");
        };

        assert_eq!(*kind, bray_ir::MirGeneratorKind::General);
        assert_eq!(begin, push);
        assert_eq!(push, second_push);
        assert_eq!(second_push, finish);
    }

    #[test]
    fn checked_assertion_lowers_to_explicit_failure_control() {
        let compilation = compilation(FAILURE_LOWERING_SOURCE);
        let key = source_callable_body_key(&compilation);

        let result = compilation
            .lowered_unit(key)
            .unwrap_or_else(|error| panic!("failure MIR must be available: {error:?}"));

        let mir = lowered_mir(&result);

        assert!(mir.operations().iter().any(|operation| {
            matches!(
                operation.kind(),
                MirOperationKind::PanicReport(MirPanicCause::Assertion(None))
            )
        }));

        assert!(mir.blocks().iter().any(|block| matches!(
            block.terminator().kind(),
            MirTerminatorKind::BeginCleanup(_) | MirTerminatorKind::Panic { .. }
        )));
    }

    #[test]
    fn diverging_assertion_messages_leave_the_success_path_available() {
        let compilation = compilation(DIVERGING_ASSERTION_MESSAGE_SOURCE);
        let key = source_callable_body_key(&compilation);

        let result = compilation
            .lowered_unit(key)
            .unwrap_or_else(|error| panic!("assertion MIR must be available: {error:?}"));

        assert!(
            result.value().is_some(),
            "a diverging failure message must not terminate the assertion success path: {:?}",
            result.diagnostics()
        );
    }

    #[test]
    fn cleanup_materializes_every_verified_lifecycle_storage() {
        let compilation = compilation(concat!(
            "module app;\n",
            "struct Resource\n",
            "{\n",
            "    destruct()\n",
            "    {\n",
            "    }\n",
            "}\n",
            "union OpenError\n",
            "{\n",
            "    Failed;\n",
            "}\n",
            "func open() -> Result<Resource, OpenError>\n",
            "{\n",
            "    return Ok(Resource {});\n",
            "}\n",
            "func read(pos text: &string) -> bool\n",
            "{\n",
            "    return true;\n",
            "}\n",
            "func main() -> Result<unit, OpenError>\n",
            "{\n",
            "    let resource: Resource = try open();\n",
            "    let observed: bool = read(&\"borrowed\");\n",
            "    return Ok(unit);\n",
            "}\n",
        ));

        let key = source_function_body_key(&compilation, "main");
        let storage = compilation.storage_plan(key.clone()).unwrap();
        let analysis = compilation.async_analysis(key.clone()).unwrap();

        let required = analysis
            .value()
            .scope_exits()
            .iter()
            .flat_map(|exit| {
                exit.cancellation_broadcast()
                    .iter()
                    .chain(exit.lifecycle_resolution())
            })
            .filter_map(|access| storage.value().root_identity(*access))
            .collect::<BTreeSet<_>>();

        let lowered = compilation
            .lowered_unit(key)
            .unwrap_or_else(|error| panic!("resource cleanup must lower: {error:?}"));

        assert!(
            lowered.diagnostics().is_empty(),
            "{:#?}",
            lowered.diagnostics()
        );

        let cleanup_places = lowered_mir(&lowered)
            .operations()
            .iter()
            .filter_map(|operation| match operation.kind() {
                MirOperationKind::Cleanup { place, .. } => Some((place.storage(), place.ty())),
                _ => None,
            })
            .collect::<Vec<_>>();

        let cleanup_storages = cleanup_places
            .iter()
            .filter(|(storage, _)| {
                lowered_mir(&lowered).storage(*storage).unwrap().kind()
                    != &bray_ir::MirStorageKind::Return
            })
            .map(|(storage, _)| *storage)
            .collect::<BTreeSet<_>>();

        assert_eq!(cleanup_storages.len(), required.len(), "{cleanup_places:?}");
    }

    #[test]
    fn propagated_nested_scope_exit_publishes_its_cleanup_plan() {
        let compilation = standard_text_compilation(&[
            include_str!("../../../../standard-library/std/src/memory.bray"),
            include_str!("../../../../standard-library/std/src/bytes/buffer.bray"),
            include_str!("../../../../standard-library/std/src/collection/list.bray"),
            include_str!("../../../../standard-library/std/src/collection/deque.bray"),
            r#"module app;

using std.collection;
using std.bytes;
using std.memory;

struct Probe
{
    bytes: std.bytes.Buffer;
}

trusted func main() -> Result<unit, std.memory.MemoryLayoutError>
{
    {
        let bytes: [u8; 1] = [7];
        let mut values: std.collection.Deque<Probe> = try trusted std.collection.Deque<Probe>();

        try trusted values.push_back(
            {
                bytes = try std.bytes.Buffer.from_slice(&bytes[..]),
            }
        );
    }

    return Ok(unit);
}
"#,
        ]);

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "main"))
            .unwrap_or_else(|error| panic!("nested propagated cleanup must lower: {error:?}"));

        assert!(
            lowered.diagnostics().is_empty(),
            "{:#?}",
            lowered.diagnostics()
        );

        assert!(lowered.value().is_some(), "{lowered:#?}");
    }

    #[test]
    fn nullable_nested_scope_exit_publishes_its_cleanup_plan() {
        let compilation = compilation(
            r#"module app;

struct Probe
{
    destruct() {}
}

func main(pos value: i32?) -> i32?
{
    {
        let probe: Probe = Probe {};
        let unwrapped: i32 = value?;
        let mut result: i32? = none;

        result = unwrapped;

        return result;
    }
}
"#,
        );

        let key = source_function_body_key(&compilation, "main");

        let bound = compilation
            .bound_unit(key.clone())
            .unwrap_or_else(|error| panic!("nullable unit must bind: {error:?}"));

        assert!(bound.diagnostics().is_empty(), "{:#?}", bound.diagnostics());

        let types = compilation
            .expression_types(key.clone())
            .unwrap_or_else(|error| panic!("nullable expressions must type: {error:?}"));

        assert!(types.diagnostics().is_empty(), "{:#?}", types.diagnostics());

        let patterns = compilation
            .patterns(key.clone())
            .unwrap_or_else(|error| panic!("nullable patterns must check: {error:?}"));

        assert!(
            patterns.diagnostics().is_empty(),
            "{:#?}",
            patterns.diagnostics()
        );

        let storage = compilation
            .storage_plan(key.clone())
            .unwrap_or_else(|error| panic!("nullable storage must plan: {error:?}"));

        assert!(
            storage.diagnostics().is_empty(),
            "{:#?}",
            storage.diagnostics()
        );

        compilation
            .storage_flow(key.clone())
            .unwrap_or_else(|error| panic!("nullable storage flow must publish: {error:?}"));

        compilation
            .async_analysis(key.clone())
            .unwrap_or_else(|error| panic!("nullable cleanup plans must publish: {error:?}"));

        let lowered = compilation
            .lowered_unit(key)
            .unwrap_or_else(|error| panic!("nested nullable cleanup must lower: {error:?}"));

        assert!(
            lowered.diagnostics().is_empty(),
            "{:#?}",
            lowered.diagnostics()
        );

        assert!(lowered.value().is_some(), "{lowered:#?}");
    }

    #[test]
    fn checked_result_propagation_lowers_success_and_error_paths() {
        let compilation = compilation(RESULT_PROPAGATION_LOWERING_SOURCE);
        let key = source_callable_body_key(&compilation);

        let result = compilation
            .lowered_unit(key)
            .unwrap_or_else(|error| panic!("result propagation MIR must be available: {error:?}"));

        let mir = lowered_mir(&result);

        assert!(mir.blocks().iter().any(|block| matches!(
            block.terminator().kind(),
            MirTerminatorKind::PatternBranch {
                predicate: bray_ir::MirPatternPredicate::ActiveUnionVariant(_),
                ..
            }
        )));

        assert!(mir.operations().iter().any(|operation| matches!(
            operation.kind(),
            MirOperationKind::PatternProjection {
                projection: bray_bound_tree::PatternProjection::ActiveUnionPayloadField { .. },
                ..
            }
        )));
    }

    #[test]
    fn result_propagation_applies_the_checked_error_conversion() {
        let compilation = compilation(CONVERTED_RESULT_PROPAGATION_SOURCE);
        let key = source_callable_body_key(&compilation);

        let result = compilation
            .lowered_unit(key)
            .unwrap_or_else(|error| panic!("result propagation MIR must be available: {error:?}"));

        let mir = lowered_mir(&result);

        assert!(
            mir.operations()
                .iter()
                .any(|operation| matches!(operation.kind(), MirOperationKind::Convert { .. }))
        );
    }

    #[test]
    fn result_propagation_requires_a_compatible_lexical_boundary() {
        let compilation = compilation(INCOMPATIBLE_RESULT_PROPAGATION_SOURCE);
        let key = source_callable_body_key(&compilation);

        let selections = compilation
            .semantic_selections(key)
            .unwrap_or_else(|error| panic!("semantic selections must be available: {error:?}"));

        assert_goal_state_diagnostic_kind(
            selections.diagnostics(),
            bray_diagnostics::DiagnosticKind::CheckingNoCompatiblePropagationBoundary,
        );
    }

    #[test]
    fn nullable_propagation_requires_a_nullable_lexical_boundary() {
        let compilation = compilation(INCOMPATIBLE_NULLABLE_PROPAGATION_SOURCE);
        let key = source_callable_body_key(&compilation);

        let selections = compilation
            .semantic_selections(key)
            .unwrap_or_else(|error| panic!("semantic selections must be available: {error:?}"));

        assert_goal_state_diagnostic_kind(
            selections.diagnostics(),
            bray_diagnostics::DiagnosticKind::CheckingNoCompatiblePropagationBoundary,
        );
    }

    #[test]
    fn checked_borrows_lower_to_explicit_borrow_operations() {
        let compilation = compilation(BORROW_LOWERING_SOURCE);
        let key = source_callable_body_key(&compilation);

        let result = compilation
            .lowered_unit(key)
            .unwrap_or_else(|error| panic!("borrow MIR must be available: {error:?}"));

        let mir = lowered_mir(&result);

        assert!(mir.operations().iter().any(|operation| matches!(
            operation.kind(),
            MirOperationKind::Borrow {
                kind: BorrowKind::Shared,
                ..
            }
        )));
    }

    #[test]
    fn qualified_compiler_provided_memory_calls_lower_to_explicit_mir() {
        let compilation = compilation_with_target_operations(
            concat!(
                "trusted module app;\n",
                "trusted func main() uses(manual_alloc)\n",
                "{\n",
                "    let pointer = trusted core.memory.allocate(bytes = 0, align = 1);\n",
                "\n",
                "    trusted core.memory.deallocate(pointer = pointer, bytes = 0, align = 1);\n",
                "}\n",
            ),
            true,
            true,
        );

        let result = compilation
            .lowered_unit(source_callable_body_key(&compilation))
            .unwrap_or_else(|error| panic!("memory operation MIR must be available: {error:?}"));

        let kinds = lowered_mir(&result)
            .operations()
            .iter()
            .filter_map(|operation| match operation.kind() {
                MirOperationKind::Memory(memory) => Some(memory.kind()),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            kinds,
            [
                CheckedMemoryOperationKind::RawAllocate,
                CheckedMemoryOperationKind::RawDeallocate,
            ]
        );
    }

    #[test]
    fn standard_text_sources_bind_primitive_hooks_and_cursor_bodies() {
        let compilation = standard_text_compilation(&[
            include_str!("../../../../xtask/fixtures/native-execution/standard-string.bray"),
            include_str!("../../../../xtask/fixtures/native-execution/standard-character.bray"),
            include_str!("../../../../xtask/fixtures/native-execution/standard-text-cursor.bray"),
        ]);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        for name in ["exercise_text_operations", "exercise_character_operations"] {
            assert!(
                implementation_hooks(&compilation, name).is_empty(),
                "{name}"
            );
        }

        let text_selections = compilation
            .semantic_selections(source_function_body_key(
                &compilation,
                "exercise_text_operations",
            ))
            .unwrap_or_else(|error| panic!("text selections must be available: {error:?}"));

        let string_operator_targets = text_selections
            .value()
            .entries()
            .iter()
            .filter_map(|entry| match entry.selection() {
                SemanticSelection::Operation(SelectedOperation::Operator { target, .. }) => {
                    Some(target)
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(string_operator_targets.len(), 2);

        assert!(
            string_operator_targets
                .iter()
                .all(|target| matches!(target, OperatorTarget::BuiltIn(_)))
        );

        let text_operations = compilation
            .lowered_unit(source_function_body_key(
                &compilation,
                "exercise_text_operations",
            ))
            .unwrap_or_else(|error| panic!("text operations must lower: {error:?}"));

        assert_eq!(
            lowered_mir(&text_operations)
                .operations()
                .iter()
                .filter(|operation| {
                    matches!(
                        operation.kind(),
                        MirOperationKind::Text(text)
                            if text.kind() == MirTextOperationKind::Equals
                    )
                })
                .count(),
            2,
        );

        let string_hooks = [
            ("length", ImplementationHook::StringScalarCount),
            ("is_empty", ImplementationHook::StringIsEmpty),
            ("get", ImplementationHook::StringScalarAt),
            ("slice", ImplementationHook::StringScalarSlice),
            ("as_bytes", ImplementationHook::StringUtf8),
            ("from_utf8", ImplementationHook::StringFromUtf8),
        ];

        let character_hooks = [
            ("code_point", ImplementationHook::CharacterScalarValue),
            (
                "from_code_point",
                ImplementationHook::CharacterFromScalarValue,
            ),
            ("is_alphabetic", ImplementationHook::CharacterIsAlphabetic),
            ("is_numeric", ImplementationHook::CharacterIsNumeric),
            ("is_whitespace", ImplementationHook::CharacterIsWhitespace),
        ];

        let encode_utf8_hooks = implementation_hooks_for_key(
            &compilation,
            "encode_utf8",
            source_type_callable_member_body_key(&compilation, "encode_utf8"),
        );

        assert_eq!(
            encode_utf8_hooks,
            [
                ImplementationHook::CharacterUtf8Length,
                ImplementationHook::CharacterUtf8Byte,
            ]
        );

        let equals_hooks = implementation_hooks_for_key(
            &compilation,
            "equals",
            source_trait_callable_fulfillment_body_key(&compilation, "equals"),
        );

        assert_eq!(equals_hooks, [ImplementationHook::StringEquals]);

        let primitive_hooks = string_hooks
            .into_iter()
            .chain(character_hooks)
            .flat_map(|(name, expected)| {
                let key = source_type_callable_member_body_key(&compilation, name);
                let hooks = implementation_hooks_for_key(&compilation, name, key);

                assert_eq!(hooks, [expected], "{name}");

                hooks
            })
            .chain(encode_utf8_hooks)
            .chain(equals_hooks)
            .collect::<BTreeSet<_>>();

        assert_eq!(
            primitive_hooks,
            BTreeSet::from([
                ImplementationHook::StringScalarCount,
                ImplementationHook::StringIsEmpty,
                ImplementationHook::StringEquals,
                ImplementationHook::StringScalarAt,
                ImplementationHook::StringScalarSlice,
                ImplementationHook::StringUtf8,
                ImplementationHook::StringFromUtf8,
                ImplementationHook::CharacterScalarValue,
                ImplementationHook::CharacterFromScalarValue,
                ImplementationHook::CharacterUtf8Length,
                ImplementationHook::CharacterUtf8Byte,
                ImplementationHook::CharacterIsAlphabetic,
                ImplementationHook::CharacterIsNumeric,
                ImplementationHook::CharacterIsWhitespace,
            ])
        );

        assert!(
            implementation_hooks_for_key(
                &compilation,
                "characters",
                source_type_callable_member_body_key(&compilation, "characters"),
            )
            .is_empty()
        );

        let next_key = source_trait_callable_fulfillment_body_key(&compilation, "next");

        assert!(implementation_hooks_for_key(&compilation, "next", next_key).is_empty());

        for (name, key, expected) in [
            (
                "length",
                source_type_callable_member_body_key(&compilation, "length"),
                MirTextOperationKind::ScalarCount,
            ),
            (
                "get",
                source_type_callable_member_body_key(&compilation, "get"),
                MirTextOperationKind::ScalarAt,
            ),
        ] {
            let lowered = compilation.lowered_unit(key).unwrap_or_else(|error| {
                panic!("{name} must lower through its Bray body: {error:?}")
            });

            assert!(
                lowered.diagnostics().is_empty(),
                "{:#?}",
                lowered.diagnostics()
            );

            let operations = lowered_mir(&lowered)
                .operations()
                .iter()
                .filter_map(|operation| match operation.kind() {
                    MirOperationKind::Text(text) => Some(text.kind()),
                    _ => None,
                })
                .collect::<Vec<_>>();

            assert_eq!(operations, [expected], "{name}");
        }
    }

    #[test]
    fn standard_numeric_truncation_lowers_explicitly_with_an_inferred_source_type() {
        let compilation = standard_text_compilation(&[concat!(
            "module std.numeric;\n",
            "\n",
            "func truncate(pos value: u128) -> u8\n",
            "{\n",
            "    return std.truncate_to<u8>(value);\n",
            "}\n",
        )]);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        let key = source_function_body_key(&compilation, "truncate");

        let lowered = compilation
            .lowered_unit(key)
            .unwrap_or_else(|error| panic!("numeric truncation must lower: {error:?}"));

        assert!(lowered_mir(&lowered).operations().iter().any(|operation| {
            matches!(
                operation.kind(),
                MirOperationKind::NumericConversion {
                    kind: bray_ir::MirNumericConversionKind::Truncate,
                    ..
                }
            )
        }));
    }

    #[test]
    fn trait_qualified_standard_text_calls_lower_to_direct_calls() {
        let compilation = standard_text_compilation(&[include_str!(
            "../../../../xtask/fixtures/native-execution/standard-text-cursor.bray"
        )]);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "main"))
            .unwrap_or_else(|error| panic!("standard text cursor must lower: {error:?}"));

        assert!(
            lowered.diagnostics().is_empty(),
            "{:#?}",
            lowered.diagnostics()
        );

        let call_targets = lowered_mir(&lowered)
            .operations()
            .iter()
            .filter_map(|operation| match operation.kind() {
                MirOperationKind::Call(call)
                    if !matches!(call.target(), MirCallTarget::Runtime(_)) =>
                {
                    Some(call.target())
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(call_targets.len(), 5);

        assert!(
            call_targets
                .iter()
                .all(|target| matches!(target, MirCallTarget::Direct(_)))
        );

        let mutable_receiver_borrows = lowered_mir(&lowered)
            .operations()
            .iter()
            .filter(|operation| {
                matches!(
                    operation.kind(),
                    MirOperationKind::Borrow {
                        kind: BorrowKind::Mutable,
                        ..
                    }
                )
            })
            .count();

        assert_eq!(mutable_receiver_borrows, 4);
    }

    #[test]
    fn direct_compiler_known_trait_calls_retain_builtin_intrinsics() {
        let compilation = standard_text_compilation(&[concat!(
            "module std.direct_trait_call;\n",
            "func generic_equal<T>(pos left: &T, pos right: &T) -> bool\n",
            "    with(T: Equatable<T>)\n",
            "{\n",
            "    return left.equals(right);\n",
            "}\n",
        )]);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "generic_equal"))
            .unwrap_or_else(|error| panic!("generic equality call must lower: {error:?}"));

        let calls = lowered_mir(&lowered)
            .operations()
            .iter()
            .filter_map(|operation| match operation.kind() {
                MirOperationKind::Call(call) => Some(call),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(calls.len(), 1);
        assert!(calls[0].trait_dispatch().is_some());

        assert_eq!(
            calls[0].intrinsic(),
            Some(MirCallIntrinsic::Binary(MirBinaryOperator::Equal))
        );
    }

    #[test]
    fn standard_run_utilities_lower_to_current_run_operations() {
        let compilation = standard_text_compilation(&[include_str!(
            "../../../../standard-library/std/src/run.bray"
        )]);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        assert_eq!(
            implementation_hooks(&compilation, "cancellation_requested"),
            [ImplementationHook::CurrentRunCancellationObservation]
        );

        assert_eq!(
            implementation_hooks(&compilation, "checkpoint"),
            [
                ImplementationHook::CurrentRunCancellationObservation,
                ImplementationHook::CurrentRunCancellationPropagation,
            ]
        );

        let observation = compilation
            .lowered_unit(source_function_body_key(
                &compilation,
                "cancellation_requested",
            ))
            .unwrap_or_else(|error| panic!("cancellation observation must lower: {error:?}"));

        assert!(observation.diagnostics().is_empty());

        assert!(
            lowered_mir(&observation)
                .operations()
                .iter()
                .any(|operation| {
                    matches!(
                        operation.kind(),
                        MirOperationKind::Async(
                            bray_ir::MirAsyncOperation::ObserveCurrentRunCancellation { .. }
                        )
                    )
                })
        );

        let checkpoint = compilation
            .lowered_unit(source_function_body_key(&compilation, "checkpoint"))
            .unwrap_or_else(|error| panic!("cancellation checkpoint must lower: {error:?}"));

        assert!(checkpoint.diagnostics().is_empty());

        assert!(lowered_mir(&checkpoint).blocks().iter().any(|block| {
            matches!(
                block.terminator().kind(),
                MirTerminatorKind::PropagateCancellation { .. }
            )
        }));
    }

    #[test]
    fn standard_testing_failure_lowers_to_structured_failure_control() {
        let compilation = standard_text_compilation(&[
            include_str!("../../../../standard-library/std/src/testing.bray"),
            concat!(
                "module std.testing;\n",
                "\n",
                "func exercise(pos message: &string) -> never\n",
                "{\n",
                "    fail(message);\n",
                "}\n",
                "\n",
                "func catch_failure(pos message: &string) -> Result<never, PanicReport>\n",
                "{\n",
                "    return catch fail(message);\n",
                "}\n",
            ),
        ]);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        assert_eq!(
            implementation_hooks(&compilation, "exercise"),
            [ImplementationHook::TestingFail]
        );

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "exercise"))
            .unwrap_or_else(|error| panic!("testing failure must lower: {error:?}"));

        assert!(lowered.diagnostics().is_empty());

        assert!(lowered_mir(&lowered).operations().iter().any(|operation| {
            matches!(
                operation.kind(),
                MirOperationKind::PanicReport(MirPanicCause::ExplicitTestFailure(_))
            )
        }));

        assert!(lowered_mir(&lowered).blocks().iter().any(|block| {
            matches!(
                block.terminator().kind(),
                MirTerminatorKind::BeginCleanup(_) | MirTerminatorKind::Panic { .. }
            )
        }));

        let caught = compilation
            .lowered_unit(source_function_body_key(&compilation, "catch_failure"))
            .unwrap_or_else(|error| panic!("caught testing failure must lower: {error:?}"));

        assert!(caught.diagnostics().is_empty());

        assert!(
            lowered_mir(&caught).blocks().iter().any(|block| {
                matches!(block.terminator().kind(), MirTerminatorKind::Panic { .. })
            })
        );
    }

    #[test]
    fn standard_task_yield_is_an_asynchronous_computation() {
        let compilation = standard_text_compilation(&[
            include_str!("../../../../standard-library/std/src/run.bray"),
            include_str!("../../../../standard-library/std/src/task.bray"),
        ]);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "yield_now"))
            .unwrap_or_else(|error| panic!("task yield must lower: {error:?}"));

        assert!(lowered.diagnostics().is_empty());

        assert!(matches!(
            lowered_mir(&lowered).kind(),
            bray_ir::MirUnitKind::ProtectedAsyncFrame(_)
        ));

        assert_eq!(
            implementation_hooks(&compilation, "yield_now"),
            [ImplementationHook::TaskYield]
        );

        assert!(lowered_mir(&lowered).blocks().iter().any(|block| {
            matches!(
                block.terminator().kind(),
                MirTerminatorKind::Suspend {
                    kind: bray_ir::MirSuspensionKind::Yield,
                    ..
                }
            )
        }));
    }

    #[test]
    fn standard_task_events_lower_creation_and_waiting() {
        let compilation = standard_text_compilation(&[
            include_str!("../../../../standard-library/std/src/run.bray"),
            include_str!("../../../../standard-library/std/src/task.bray"),
        ]);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        let creation_key = source_function_body_key(&compilation, "event");

        let creation = compilation
            .lowered_unit(creation_key)
            .unwrap_or_else(|error| panic!("task event creation must lower: {error:?}"));

        assert!(creation.diagnostics().is_empty());

        let wait = compilation
            .lowered_unit(source_function_body_key(&compilation, "wait"))
            .unwrap_or_else(|error| panic!("task event wait must lower: {error:?}"));

        assert!(wait.diagnostics().is_empty());

        assert!(matches!(
            lowered_mir(&wait).kind(),
            bray_ir::MirUnitKind::ProtectedAsyncFrame(_)
        ));

        assert!(lowered_mir(&wait).blocks().iter().any(|block| {
            matches!(
                block.terminator().kind(),
                MirTerminatorKind::Suspend {
                    kind: bray_ir::MirSuspensionKind::TaskEvent,
                    payload: Some(_),
                    ..
                }
            )
        }));
    }

    fn standard_text_compilation(additional_sources: &[&str]) -> Compilation {
        let package = PackageIdentity::try_new("std")
            .unwrap_or_else(|| panic!("standard library identity must be valid"));

        let sources = [
            include_str!("../../../../standard-library/std/src/std.bray"),
            include_str!("../../../../standard-library/std/src/string.bray"),
            include_str!("../../../../standard-library/std/src/character.bray"),
        ]
        .into_iter()
        .chain(additional_sources.iter().copied())
        .enumerate()
        .map(|(index, source)| {
            let version = u32::try_from(index)
                .unwrap_or_else(|_| panic!("standard text source index must fit in u32"));

            source_input(source, version)
        })
        .collect();

        let request = CompilationRequest::with_options(
            package,
            sources,
            CompilationOptions::new(
                WorkerBudget::serial(),
                ProductKind::Library,
                SelectedTarget::baseline(),
            ),
        )
        .with_standard_library_source_authority();

        Compilation::load(request)
            .unwrap_or_else(|error| panic!("standard text compilation must load: {error:?}"))
    }

    fn implementation_hooks(compilation: &Compilation, name: &str) -> Vec<ImplementationHook> {
        let key = source_function_body_key(compilation, name);

        implementation_hooks_for_key(compilation, name, key)
    }

    fn implementation_hooks_for_key(
        compilation: &Compilation,
        name: &str,
        key: BoundUnitKey,
    ) -> Vec<ImplementationHook> {
        let selections = compilation
            .semantic_selections(key)
            .unwrap_or_else(|error| {
                panic!("standard text selections for {name} must be available: {error:?}")
            });

        assert!(
            selections.diagnostics().is_empty(),
            "{name}: {:#?}",
            selections.diagnostics()
        );

        selections
            .value()
            .entries()
            .iter()
            .filter_map(|entry| match entry.selection() {
                SemanticSelection::Call(call) => call.implementation_hook(),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn annotated_nullable_local_initializers_materialize_presence() {
        let compilation = compilation(
            r#"module app;

struct OwnedValue
{
    value: i32;
}

func from_literal() -> i32?
{
    let value: i32? = 1;

    return value;
}

func from_name(pos input: i32) -> i32?
{
    let value: i32? = input;

    return value;
}

func from_unit() -> unit?
{
    let value: unit? = unit;

    return value;
}

func from_owned() -> OwnedValue?
{
    let value: OwnedValue? = OwnedValue
    {
        value = 1
    };

    return value;
}

func return_literal() -> i32?
{
    return 1;
}
"#,
        );

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        for name in [
            "from_literal",
            "from_name",
            "from_unit",
            "from_owned",
            "return_literal",
        ] {
            let key = source_function_body_key(&compilation, name);

            let lowered = compilation.lowered_unit(key).unwrap_or_else(|error| {
                panic!("nullable initializer MIR for {name} must be available: {error:?}")
            });

            let mir = lowered_mir(&lowered);

            assert!(
                mir.operations().iter().any(|operation| matches!(
                    operation.kind(),
                    MirOperationKind::Aggregate(aggregate)
                        if aggregate.kind() == MirAggregateKind::NullablePresent
                )),
                "{mir:#?}"
            );
        }
    }

    #[test]
    fn annotated_nullable_local_constant_initializers_retain_nullable_storage() {
        let compilation = compilation(
            r#"module app;

func main() -> i32?
{
    const value: i32? = 1;

    return value;
}
"#,
        );

        let key = source_function_body_key(&compilation, "main");

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        let lowered = compilation.lowered_unit(key).unwrap_or_else(|error| {
            panic!("nullable local constant MIR must be available: {error:?}")
        });

        let mir = lowered_mir(&lowered);

        let Some(MirTerminatorKind::Return(Some(MirOperand::Copy(place)))) =
            mir.blocks().last().map(|block| block.terminator().kind())
        else {
            panic!("nullable local constant must lower through typed storage: {mir:#?}");
        };

        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must be available: {error:?}"));

        let ty = values
            .type_data(place.ty())
            .unwrap_or_else(|error| panic!("nullable storage type must be available: {error:?}"));

        assert!(matches!(ty.as_ref(), TypeData::Nullable(_)));
    }

    fn lowering_compilation() -> Compilation {
        compilation(LOWERING_SOURCE)
    }

    fn lowered_mir(result: &DiagnosticResult<Option<LoweredUnit>>) -> &MirUnit {
        result
            .value()
            .as_ref()
            .and_then(LoweredUnit::mir)
            .unwrap_or_else(|| panic!("checked executable unit must produce MIR: {result:#?}"))
    }

    fn custom_index_call_arguments(mir: &MirUnit) -> &[bray_ir::MirCallArgument] {
        mir.operations()
            .iter()
            .find_map(|operation| match operation.kind() {
                MirOperationKind::Call(call) if !call.witnesses().is_empty() => {
                    Some(call.arguments())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("custom index protocol call must be present: {mir:#?}"))
    }

    fn explicit_call_operand(argument: &bray_ir::MirCallArgument) -> &MirOperand {
        argument
            .value()
            .unwrap_or_else(|| panic!("custom index protocol arguments must be explicit"))
    }

    fn nullable_payload<'mir>(
        mir: &'mir MirUnit,
        operand: &'mir MirOperand,
    ) -> Option<&'mir MirOperand> {
        match operand {
            MirOperand::Immediate {
                value: MirImmediateValue::NullableAbsent,
                ..
            } => None,
            MirOperand::Value(value) => {
                let value = mir
                    .value(*value)
                    .unwrap_or_else(|| panic!("nullable value must exist: {value:?}"));

                let MirValueOrigin::Operation(operation) = value.origin() else {
                    panic!("present nullable must be produced by an operation");
                };

                let operation = mir
                    .operation(operation)
                    .unwrap_or_else(|| panic!("nullable operation must exist: {operation:?}"));

                let MirOperationKind::Aggregate(aggregate) = operation.kind() else {
                    panic!("present nullable must be an aggregate: {operation:#?}");
                };

                assert_eq!(aggregate.kind(), MirAggregateKind::NullablePresent);

                let [payload] = aggregate.operands() else {
                    panic!("present nullable must retain exactly one payload");
                };

                Some(payload)
            }
            _ => panic!("slice bound must be a present or absent nullable: {operand:#?}"),
        }
    }

    #[test]
    fn shared_receiver_storage_is_read_through_its_borrowed_representation() {
        let compilation = compilation(
            r#"module app;

trait Read
{
    func read() -> i32;
}

impl I32Read = i32(Read)
{
    func read() -> i32
    {
        return self;
    }
}
"#,
        );

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        let lowered = compilation
            .lowered_unit(source_trait_callable_fulfillment_body_key(
                &compilation,
                "read",
            ))
            .unwrap_or_else(|error| panic!("shared receiver body must lower: {error:?}"));

        let mir = lowered_mir(&lowered);

        assert!(
            mir.blocks().iter().any(|block| matches!(
                block.terminator().kind(),
                MirTerminatorKind::Return(Some(MirOperand::Copy(place)))
                    if matches!(
                        place.projections().first().map(bray_ir::MirProjection::kind),
                        Some(MirProjectionKind::Dereference)
                    )
            )),
            "{mir:#?}"
        );
    }

    #[test]
    fn constant_false_assertions_terminate_non_unit_callables() {
        let compilation = compilation(TERMINATING_ASSERTION_SOURCE);
        let key = source_callable_body_key(&compilation);

        let result = compilation
            .lowered_unit(key)
            .unwrap_or_else(|error| panic!("assertion MIR must be available: {error:?}"));

        let mir = lowered_mir(&result);

        assert!(
            mir.blocks()
                .iter()
                .all(|block| !matches!(block.terminator().kind(), MirTerminatorKind::Return(None)))
        );
    }

    #[test]
    fn nested_borrowed_fields_insert_each_implicit_dereference() {
        let compilation = compilation(
            r#"module app;

struct Value
{
    number: i32;
}

struct Owner
{
    value: &Value;
}

func read(pos owner: &Owner) -> i32
{
    return owner.value.number;
}
"#,
        );

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "read"))
            .unwrap_or_else(|error| panic!("nested borrowed fields must lower: {error:?}"));

        let mir = lowered_mir(&lowered);

        let projections = mir
            .blocks()
            .iter()
            .find_map(|block| match block.terminator().kind() {
                MirTerminatorKind::BeginCleanup(cleanup)
                | MirTerminatorKind::ContinueCleanup(cleanup) => cleanup
                    .edge()
                    .arguments()
                    .iter()
                    .find_map(|argument| match argument {
                        MirOperand::Copy(place) => Some(place.projections()),
                        _ => None,
                    }),
                MirTerminatorKind::Return(Some(MirOperand::Copy(place))) => {
                    Some(place.projections())
                }
                _ => None,
            });

        let Some(projections) = projections else {
            panic!("nested field read must return from a place: {mir:#?}");
        };

        assert_eq!(
            projections
                .iter()
                .filter(|projection| projection.kind() == &MirProjectionKind::Dereference)
                .count(),
            2,
            "{mir:#?}"
        );
    }

    fn declared_unit_key(compilation: &Compilation, kind: BoundUnitKind) -> BoundUnitKey {
        compilation
            .declared_unit_keys_for_test()
            .unwrap_or_else(|error| panic!("declared units must be available: {error:?}"))
            .into_iter()
            .find(|key| key.kind() == kind)
            .unwrap_or_else(|| panic!("test source must contain a {kind:?} unit"))
    }

    fn lowering_request(
        source: &str,
        version: u32,
        options: CompilationOptions,
    ) -> CompilationRequest {
        CompilationRequest::with_options(
            package_identity(),
            vec![source_input(source, version)],
            options,
        )
    }
}
