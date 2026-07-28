use std::sync::Arc;

use bray_bound_tree::BoundUnitKey;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_ir::{MirTargetFacts, MirUnit, MirUnitKind};
use bray_lowering::{LoweringInput, lower_unit};

use super::Compilation;
use crate::fact::{
    CancellationToken, CompilationFactKey, FactQueryError, PublishedUnitFact, QueryPriority,
};

type MirUnitComputation = (
    DiagnosticResult<Option<MirUnit>>,
    Box<[bray_binder::BinderDependency]>,
);

impl Compilation {
    // TODO(BRA-157): Publish generated executable-host MIR through this fact surface.

    /// Returns validated MIR and dependency diagnostics for one checked semantic unit.
    pub fn mir_unit(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<Option<MirUnit>>>, FactQueryError> {
        let published = self.mir_unit_with_cancellation(key, &self.state.cancellation)?;

        Ok(Arc::clone(published.result()))
    }

    /// Returns validated MIR for a cancellable prioritized request.
    pub fn mir_unit_with_priority(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
        priority: QueryPriority,
    ) -> Result<Arc<DiagnosticResult<Option<MirUnit>>>, FactQueryError> {
        let published =
            self.mir_unit_with_cancellation_and_priority(key, cancellation, priority)?;

        Ok(Arc::clone(published.result()))
    }

    fn mir_unit_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitFact<Option<MirUnit>>>, FactQueryError> {
        let priority = self
            .state
            .fact_runtime
            .current_priority()?
            .unwrap_or(QueryPriority::Normal);

        self.mir_unit_with_cancellation_and_priority(key, cancellation, priority)
    }

    fn mir_unit_with_cancellation_and_priority(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
        priority: QueryPriority,
    ) -> Result<Arc<PublishedUnitFact<Option<MirUnit>>>, FactQueryError> {
        self.unit_fact_with_priority(
            &self.state.mir_units,
            CompilationFactKey::MirUnit(key.clone()),
            key.clone(),
            cancellation,
            priority,
            |cancellation| self.compute_mir_unit(&key, cancellation),
        )
    }

    fn compute_mir_unit(
        &self,
        key: &BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<MirUnitComputation, FactQueryError> {
        let unit = self.bound_unit_with_cancellation(key.clone(), cancellation)?;
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

        // TODO(BRA-157): Select protected async-frame MIR kinds from checked async facts.
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
            MirUnitKind::Synchronous,
            target,
        )
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let mir = lower_unit(input).map_err(|_| FactQueryError::InfrastructureFailure)?;

        cancellation.check()?;

        Ok((DiagnosticResult::new(Some(mir), diagnostics), Box::new([])))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_runtime_interface::RuntimeAbiVersion;
    use bray_symbols::ProductKind;

    use super::Compilation;
    use crate::test_support::{
        compilation, package_identity, source_callable_body_key, source_input,
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

    #[test]
    fn mir_units_are_lowered_lazily_and_published_once() {
        let compilation = lowering_compilation();
        let key = source_callable_body_key(&compilation);

        assert_eq!(compilation.state.mir_units.is_published(&key), Ok(false));

        let first = compilation
            .mir_unit(key.clone())
            .unwrap_or_else(|error| panic!("MIR must be available: {error:?}"));

        let second = compilation
            .mir_unit(key.clone())
            .unwrap_or_else(|error| panic!("MIR must remain available: {error:?}"));

        assert!(first.diagnostics().is_empty());
        assert!(first.value().is_some());
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(compilation.state.mir_units.is_published(&key), Ok(true));
    }

    #[test]
    fn cancelled_mir_requests_publish_no_partial_unit() {
        let compilation = lowering_compilation();
        let key = source_callable_body_key(&compilation);
        let cancellation = CancellationToken::new();

        cancellation.cancel();

        let result =
            compilation.mir_unit_with_priority(key.clone(), &cancellation, QueryPriority::Normal);

        assert_eq!(result, Err(FactQueryError::Cancelled));
        assert_eq!(compilation.state.mir_units.is_published(&key), Ok(false));

        assert!(compilation.mir_unit(key).is_ok());
    }

    #[test]
    fn updated_snapshots_reuse_only_target_and_source_compatible_mir() {
        let previous = lowering_compilation();
        let key = source_callable_body_key(&previous);

        let previous_mir = previous
            .mir_unit(key.clone())
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
            .mir_unit(key.clone())
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
            .mir_unit(key)
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
            .mir_unit(source_key)
            .unwrap_or_else(|error| panic!("revised source MIR must be available: {error:?}"));

        assert!(!Arc::ptr_eq(&previous_mir, &source_mir));
    }

    #[test]
    fn structured_values_lower_to_aggregates_construction_and_projections() {
        let compilation = compilation(STRUCTURED_LOWERING_SOURCE);
        let key = source_callable_body_key(&compilation);

        let result = compilation
            .mir_unit(key)
            .unwrap_or_else(|error| panic!("structured MIR must be available: {error:?}"));

        let mir = result.value().as_ref().unwrap_or_else(|| {
            panic!(
                "structured source must produce MIR: {:?}",
                result.diagnostics()
            )
        });

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
                        value: bray_ir::MirOperand::Copy(place),
                        ..
                    } if !place.projections().is_empty()
                )
            })
        }));

        assert!(mir.operations().iter().any(|operation| matches!(
            operation.kind(),
            bray_ir::MirOperationKind::Store {
                value: bray_ir::MirOperand::Move(place),
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
            Some(bray_ir::MirTerminatorKind::Return(Some(
                bray_ir::MirOperand::Value(_)
            )))
        ));
    }

    #[test]
    fn checked_control_patterns_and_iteration_lower_to_explicit_mir() {
        let compilation = compilation(CONTROL_LOWERING_SOURCE);
        let key = source_callable_body_key(&compilation);

        let result = compilation
            .mir_unit(key)
            .unwrap_or_else(|error| panic!("control MIR must be available: {error:?}"));

        let mir = result.value().as_ref().unwrap_or_else(|| {
            panic!(
                "checked control source must produce MIR: {:?}",
                result.diagnostics()
            )
        });

        assert!(mir.blocks().iter().any(|block| matches!(
            block.terminator().kind(),
            bray_ir::MirTerminatorKind::PatternBranch { .. }
        )));

        assert!(mir.blocks().iter().any(|block| matches!(
            block.terminator().kind(),
            bray_ir::MirTerminatorKind::Iterate { .. }
        )));

        assert!(mir.blocks().iter().any(|block| matches!(
            block.terminator().kind(),
            bray_ir::MirTerminatorKind::Branch { .. }
        )));

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
            .mir_unit(key)
            .unwrap_or_else(|error| panic!("generator MIR must be available: {error:?}"));

        let mir = result.value().as_ref().unwrap_or_else(|| {
            panic!(
                "checked generator source must produce MIR: {:?}",
                result.diagnostics()
            )
        });

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
            .mir_unit(key)
            .unwrap_or_else(|error| panic!("failure MIR must be available: {error:?}"));

        let mir = result.value().as_ref().unwrap_or_else(|| {
            panic!(
                "checked failure source must produce MIR: {:?}",
                result.diagnostics()
            )
        });

        assert!(mir.operations().iter().any(|operation| matches!(
            operation.kind(),
            bray_ir::MirOperationKind::PanicReport(_)
        )));

        assert!(mir.blocks().iter().any(|block| matches!(
            block.terminator().kind(),
            bray_ir::MirTerminatorKind::BeginCleanup(_) | bray_ir::MirTerminatorKind::Panic { .. }
        )));
    }

    #[test]
    fn diverging_assertion_messages_leave_the_success_path_available() {
        let compilation = compilation(DIVERGING_ASSERTION_MESSAGE_SOURCE);
        let key = source_callable_body_key(&compilation);

        let result = compilation
            .mir_unit(key)
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
            .mir_unit(key)
            .unwrap_or_else(|error| panic!("result propagation MIR must be available: {error:?}"));

        let mir = result.value().as_ref().unwrap_or_else(|| {
            panic!(
                "checked result propagation source must produce MIR: {:?}",
                result.diagnostics()
            )
        });

        assert!(mir.blocks().iter().any(|block| matches!(
            block.terminator().kind(),
            bray_ir::MirTerminatorKind::PatternBranch {
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
            .mir_unit(key)
            .unwrap_or_else(|error| panic!("result propagation MIR must be available: {error:?}"));

        let mir = result.value().as_ref().unwrap_or_else(|| {
            panic!(
                "compatible propagated errors must produce MIR: {:?}",
                result.diagnostics()
            )
        });

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
            .mir_unit(key)
            .unwrap_or_else(|error| panic!("borrow MIR must be available: {error:?}"));

        let mir = result.value().as_ref().unwrap_or_else(|| {
            panic!(
                "checked borrow source must produce MIR: {:?}",
                result.diagnostics()
            )
        });

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
