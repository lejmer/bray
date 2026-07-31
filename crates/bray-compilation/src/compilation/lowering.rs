use std::sync::Arc;

use bray_bound_tree::BoundUnitKey;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_ir::MirTargetFacts;
use bray_lowering::{
    CompileTimeUnit, LoweredUnit, LoweringInput, executable_unit_kind, lower_unit,
};

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

        let patterns = self.pattern_facts_with_cancellation(key.clone(), cancellation)?;

        let selections = self.semantic_selections_with_cancellation(key.clone(), cancellation)?;

        let literals = self.literal_values_with_cancellation(key.clone(), cancellation)?;
        let storage = self.storage_plan_with_cancellation(key.clone(), cancellation)?;
        let liveness = self.liveness_with_cancellation(key.clone(), cancellation)?;
        let refinements = self.refinement_facts_with_cancellation(key.clone(), cancellation)?;

        let storage_flow = self.storage_flow_facts_with_cancellation(key.clone(), cancellation)?;

        let dependencies =
            self.dependency_contracts_with_cancellation(key.clone(), cancellation)?;

        let async_facts = self.async_facts_with_cancellation(key.clone(), cancellation)?;
        let behavior = self.body_behavior_with_cancellation(key.clone(), cancellation)?;

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
            async_facts.result().diagnostics(),
            behavior.result().diagnostics(),
        ]);

        if diagnostics.has_errors() {
            return Ok((DiagnosticResult::new(None, diagnostics), Box::new([])));
        }

        cancellation.check()?;

        let selected_target = self.selected_target().target();

        let target = MirTargetFacts::new(
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
            async_facts.result().value(),
            behavior.result().value(),
            self.semantic_value_store()?,
            self.available_compiler_known_symbols(),
            unit_kind,
            target,
        )
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let mir = lower_unit(input).map_err(|_| FactQueryError::InfrastructureFailure)?;

        cancellation.check()?;

        Ok((
            DiagnosticResult::new(Some(LoweredUnit::Mir(Box::new(mir))), diagnostics),
            Box::new([]),
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_bound_tree::{BoundUnitKey, BoundUnitKind};
    use bray_diagnostics::DiagnosticResult;
    use bray_ir::{MirOperand, MirTerminatorKind, MirUnit};
    use bray_lowering::LoweredUnit;
    use bray_runtime_interface::RuntimeAbiVersion;
    use bray_symbols::{ProductKind, TypeData};

    use super::Compilation;
    use crate::test_support::{
        compilation, compilation_with_sources_and_worker_budget, package_identity,
        source_callable_body_key, source_input,
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
        "    let choice: Choice = .Value(value = pair.first);\n",
        "    let empty: Choice = .Empty;\n",
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
        "    };\n",
        "\n",
        "    loop\n",
        "    {\n",
        "        break;\n",
        "    };\n",
        "\n",
        "    match make_boolean()\n",
        "    {\n",
        "        case true\n",
        "        {\n",
        "        }\n",
        "        case false\n",
        "        {\n",
        "        }\n",
        "    };\n",
        "\n",
        "    let items: Items = Items {};\n",
        "    let every: bool = all(items);\n",
        "    let some: bool = any(items);\n",
        "\n",
        "    for item in items\n",
        "    {\n",
        "        item;\n",
        "    };\n",
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
        "            };\n",
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
    fn compile_time_units_are_classified_without_demanding_runtime_facts() {
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
                    bray_ir::MirOperationKind::AnonymousCallable(key) if key == &nested
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
        assert!(!frame.states()[1].deferred_calls().is_empty());
        assert!(!frame.states()[1].initialized_storages().is_empty());

        assert!(mir.operations().iter().any(|operation| matches!(
            operation.kind(),
            bray_ir::MirOperationKind::Async(bray_ir::MirAsyncOperation::CreateFrame { .. })
        )));

        assert!(
            mir.blocks().iter().any(|block| matches!(
                block.terminator().kind(),
                MirTerminatorKind::Suspend { .. }
            ))
        );

        assert!(mir.operations().iter().any(|operation| matches!(
            operation.kind(),
            bray_ir::MirOperationKind::Async(bray_ir::MirAsyncOperation::PublishTerminalState {
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
            let bray_ir::MirOperationKind::Aggregate(aggregate) = operation.kind() else {
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
            mir.operations().iter().any(|operation| matches!(
                operation.kind(),
                bray_ir::MirOperationKind::Construct(_)
            ))
        );

        let construction_targets = mir.operations().iter().filter_map(|operation| {
            let bray_ir::MirOperationKind::Construct(construction) = operation.kind() else {
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
            bray_ir::MirOperationKind::Construct(construction)
                if matches!(
                    construction.target(),
                    bray_bound_tree::ConstructionTarget::TypeForm { .. }
                )
        )));

        assert!(mir.operations().iter().any(|operation| {
            let bray_ir::MirOperationKind::Construct(construction) = operation.kind() else {
                return false;
            };

            construction
                .inputs()
                .iter()
                .any(|input| matches!(input, bray_ir::MirConstructionInput::Default { .. }))
        }));

        assert!(mir.operations().iter().any(|operation| {
            let bray_ir::MirOperationKind::Construct(construction) = operation.kind() else {
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
            bray_ir::MirOperationKind::Store {
                value: MirOperand::Move(place),
                ..
            } if !place.projections().is_empty()
        )));

        assert!(mir.operations().iter().any(|operation| matches!(
            operation.kind(),
            bray_ir::MirOperationKind::Convert { conversion, .. }
                if conversion.source_type() != conversion.target_type()
        )));

        assert!(mir.operations().iter().any(|operation| matches!(
            operation.kind(),
            bray_ir::MirOperationKind::Convert { conversion, .. }
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
            bray_ir::MirOperationKind::PatternProjection {
                operation: bray_bound_tree::PatternOperation::Consume,
                ..
            }
        )));

        assert!(mir.operations().iter().any(|operation| {
            let bray_ir::MirOperationKind::Call(call) = operation.kind() else {
                return false;
            };

            !call.witnesses().is_empty()
        }));
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
                bray_ir::MirOperationKind::Generator(operation) => Some(operation),
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

        assert!(mir.operations().iter().any(|operation| matches!(
            operation.kind(),
            bray_ir::MirOperationKind::PanicReport(_)
        )));

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
                predicate: bray_bound_tree::PatternPredicate::ActiveUnionVariant(_),
                ..
            }
        )));

        assert!(mir.operations().iter().any(|operation| matches!(
            operation.kind(),
            bray_ir::MirOperationKind::PatternProjection {
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

        assert!(mir.operations().iter().any(|operation| matches!(
            operation.kind(),
            bray_ir::MirOperationKind::Convert { .. }
        )));
    }

    #[test]
    fn result_propagation_requires_a_compatible_lexical_boundary() {
        let compilation = compilation(INCOMPATIBLE_RESULT_PROPAGATION_SOURCE);
        let key = source_callable_body_key(&compilation);

        let selections = compilation
            .semantic_selections(key)
            .unwrap_or_else(|error| panic!("semantic selections must be available: {error:?}"));

        assert!(
            selections
                .diagnostics()
                .by_kind(bray_diagnostics::DiagnosticKind::CheckingNoCompatiblePropagationBoundary)
                .next()
                .is_some()
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
            bray_ir::MirOperationKind::Borrow {
                kind: bray_symbols::BorrowKind::Shared,
                ..
            }
        )));
    }

    fn lowering_compilation() -> Compilation {
        compilation(LOWERING_SOURCE)
    }

    fn lowered_mir(result: &DiagnosticResult<Option<LoweredUnit>>) -> &MirUnit {
        result
            .value()
            .as_ref()
            .and_then(LoweredUnit::mir)
            .unwrap_or_else(|| panic!("checked executable unit must produce MIR"))
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
