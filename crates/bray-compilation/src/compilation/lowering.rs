use std::sync::Arc;

use bray_bound_tree::{BoundExpression, BoundReferenceTarget, BoundUnit, BoundUnitKey};
use bray_checker::ConstantReferenceResolution;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_ir::MirTargetContract;
use bray_lowering::{
    CompileTimeUnit, LoweredUnit, LoweringInput, executable_unit_kind, lower_unit,
};
use bray_symbols::ConstantValueId;

use super::Compilation;
use crate::fact::{
    CancellationToken, CompilationFactKey, FactQueryError, PublishedUnitFact, QueryPriority,
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
    ) -> Result<Arc<PublishedUnitFact<Option<LoweredUnit>>>, FactQueryError> {
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
    ) -> Result<Arc<PublishedUnitFact<Option<LoweredUnit>>>, FactQueryError> {
        self.unit_fact_with_priority(
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

        let expression_types =
            self.expression_types_with_cancellation(key.clone(), cancellation)?;

        let patterns = self.patterns_with_cancellation(key.clone(), cancellation)?;

        let selections = self.semantic_selections_with_cancellation(key.clone(), cancellation)?;

        let literals = self.literal_values_with_cancellation(key.clone(), cancellation)?;

        let storage = self.storage_plan_with_cancellation(key.clone(), cancellation)?;

        let liveness = self.liveness_with_cancellation(key.clone(), cancellation)?;

        let refinements = self.refinements_with_cancellation(key.clone(), cancellation)?;

        let storage_flow = self.storage_flow_with_cancellation(key.clone(), cancellation)?;

        let dependencies =
            self.dependency_contracts_with_cancellation(key.clone(), cancellation)?;

        let async_analysis = self.async_analysis_with_cancellation(key.clone(), cancellation)?;

        let behavior = self.body_behavior_with_cancellation(key.clone(), cancellation)?;

        let (constant_reference_values, constant_reference_diagnostics) =
            self.constant_reference_values(unit.result().value(), cancellation)?;

        let diagnostics = DiagnosticBag::merged_all([
            unit.result().diagnostics(),
            control_flow.result().diagnostics(),
            expression_types.result().diagnostics(),
            patterns.result().diagnostics(),
            selections.result().diagnostics(),
            literals.result().diagnostics(),
            storage.result().diagnostics(),
            liveness.result().diagnostics(),
            refinements.result().diagnostics(),
            storage_flow.result().diagnostics(),
            dependencies.result().diagnostics(),
            async_analysis.result().diagnostics(),
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

        let input = LoweringInput::try_new(
            unit.result().value(),
            control_flow.result().value(),
            expression_types.result().value(),
            patterns.result().value(),
            selections.result().value(),
            literals.result().value(),
            storage.result().value(),
            liveness.result().value(),
            refinements.result().value(),
            storage_flow.result().value(),
            dependencies.result().value(),
            async_analysis.result().value(),
            behavior.result().value(),
            self.semantic_value_store()?,
            self.available_compiler_known_symbols(),
            unit_kind,
            target,
        )
        .and_then(|input| input.with_constant_reference_values(&constant_reference_values))
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let span = self
            .state
            .fact_runtime
            .profile()
            .map(|profile| profile.start(crate::profile::ProfileOperation::Lowering, None));

        let result = lower_unit(input);

        if let Some(span) = span {
            span.finish(crate::profile::result_outcome(&result));
        }

        let mir = result.map_err(|_| FactQueryError::InfrastructureFailure)?;

        if let Some(profile) = self.state.fact_runtime.profile() {
            profile.add_metric(crate::profile::ProfileMetricKind::MirUnits, 1);

            profile.add_metric(
                crate::profile::ProfileMetricKind::MirBlocks,
                u64::try_from(mir.blocks().len()).unwrap_or(u64::MAX),
            );

            profile.add_metric(
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
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use bray_bound_tree::{
        BoundUnitKey, BoundUnitKind, CheckedMemoryOperationKind, SemanticSelection,
    };
    use bray_compiler_known::ImplementationHook;
    use bray_diagnostics::DiagnosticResult;
    use bray_ir::{
        MirAggregateKind, MirCallTarget, MirImmediateValue, MirOperand, MirOperationKind,
        MirPanicCause, MirProjectionKind, MirStoreKind, MirTerminatorKind, MirTextOperationKind,
        MirUnit, MirValueOrigin,
    };
    use bray_lowering::LoweredUnit;
    use bray_runtime_interface::RuntimeAbiVersion;
    use bray_symbols::{BorrowKind, ConstantValueKind, PackageIdentity, ProductKind, TypeData};
    use bray_testing::assert_goal_state_diagnostic_kind;

    use super::Compilation;
    use crate::test_support::{
        compilation, compilation_with_sources_and_worker_budget,
        compilation_with_target_operations, package_identity, source_callable_body_key,
        source_function_body_key, source_input, source_trait_callable_fulfillment_body_key,
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

        assert!(lowered.diagnostics().is_empty(), "{:#?}", lowered.diagnostics());

        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must be available: {error:?}"));

        let constants = lowered_mir(&lowered)
            .blocks()
            .iter()
            .filter_map(|block| {
                let MirTerminatorKind::BeginCleanup(cleanup) = block.terminator().kind() else {
                    return None;
                };

                let [MirOperand::Constant { value, ty }] = cleanup.edge().arguments() else {
                    return None;
                };

                Some((*value, *ty))
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
            compilation
                .state
                .checked_expression_types
                .is_published(&key),
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
            compilation
                .state
                .checked_expression_types
                .is_published(&key),
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
            .declared_unit_keys()
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
                bray_ir::MirAggregateKind::Tuple,
                bray_ir::MirAggregateKind::Array,
                bray_ir::MirAggregateKind::RepeatedArray,
                bray_ir::MirAggregateKind::Tuple,
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

        assert!(matches!(
            mir.blocks().last().map(|block| block.terminator().kind()),
            Some(MirTerminatorKind::Return(Some(MirOperand::Value(_))))
        ));
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

        assert!(mir.operations().iter().any(|operation| matches!(
            operation.kind(),
            MirOperationKind::PatternProjection {
                operation: bray_bound_tree::PatternOperation::Consume,
                ..
            }
        )));

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
    fn cleanup_does_not_materialize_unreached_temporary_storage() {
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

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "main"))
            .unwrap_or_else(|error| panic!("resource cleanup must lower: {error:?}"));

        assert!(lowered.diagnostics().is_empty(), "{:#?}", lowered.diagnostics());

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
            .map(|(storage, _)| *storage)
            .collect::<BTreeSet<_>>();

        assert_eq!(cleanup_storages.len(), 2, "{cleanup_places:?}");
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
                kind: bray_symbols::BorrowKind::Shared,
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

        let primitive_hooks = ["exercise_text_operations", "exercise_character_operations"]
            .into_iter()
            .flat_map(|name| implementation_hooks(&compilation, name))
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

        assert_eq!(
            implementation_hooks(&compilation, "scalars"),
            [ImplementationHook::StringScalarCount]
        );

        let next_key = source_trait_callable_fulfillment_body_key(&compilation, "next");

        assert_eq!(
            implementation_hooks_for_key(&compilation, "next", next_key.clone()),
            [ImplementationHook::StringScalarAt]
        );

        for (name, key, expected) in [
            (
                "scalars",
                source_function_body_key(&compilation, "scalars"),
                MirTextOperationKind::ScalarCount,
            ),
            ("next", next_key, MirTextOperationKind::ScalarAt),
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
                MirOperationKind::Call(call) => Some(call.target()),
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
                        kind: bray_symbols::BorrowKind::Mutable,
                        ..
                    }
                )
            })
            .count();

        assert_eq!(mutable_receiver_borrows, 4);
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

        assert!(mir.blocks().iter().any(|block| matches!(
            block.terminator().kind(),
            MirTerminatorKind::Return(Some(MirOperand::Copy(place)))
                if matches!(
                    place.projections().first().map(bray_ir::MirProjection::kind),
                    Some(MirProjectionKind::Dereference)
                )
        )), "{mir:#?}");
    }

    fn declared_unit_key(compilation: &Compilation, kind: BoundUnitKind) -> BoundUnitKey {
        compilation
            .declared_unit_keys()
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
