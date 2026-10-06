use std::sync::Arc;

use bray_ir::{MirOperand, MirTerminatorKind};
use bray_runtime_interface::RuntimeAbiVersion;
use bray_symbols::{ProductKind, TypeData};

use super::support::lowered_mir;
use crate::test_support::{
    compilation, compilation_with_sources_and_worker_budget, package_identity,
    source_callable_body_key, source_input,
};
use crate::{
    CancellationToken, Compilation, CompilationOptions, CompilationRequest, FactQueryError,
    QueryPriority, SelectedTarget, WorkerBudget,
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

    let parallel = compilation_with_sources_and_worker_budget(&[LOWERING_SOURCE], parallel_budget);

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

    let serial_type = serial_values.type_data(*serial_type);

    let parallel_type = parallel_values.type_data(*parallel_type);

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

    let serial_constant = serial_values.constant_value_data(*serial_value);

    let parallel_constant = parallel_values.constant_value_data(*parallel_value);

    assert_eq!(serial_constant.kind(), parallel_constant.kind());
}

#[test]
fn cancelled_mir_requests_publish_no_partial_unit() {
    let compilation = lowering_compilation();
    let key = source_callable_body_key(&compilation);
    let cancellation = CancellationToken::new();

    cancellation.cancel();

    let result =
        compilation.lowered_unit_with_priority(key.clone(), &cancellation, QueryPriority::Normal);

    assert_eq!(result, Err(FactQueryError::Cancelled));

    assert_eq!(
        compilation.state.lowered_units.is_published(&key),
        Ok(false)
    );

    assert!(compilation.lowered_unit(key).is_ok());
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
            CompilationOptions::new(WorkerBudget::serial(), ProductKind::Library, revised_target),
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

fn lowering_compilation() -> Compilation {
    compilation(LOWERING_SOURCE)
}

fn lowering_request(source: &str, version: u32, options: CompilationOptions) -> CompilationRequest {
    CompilationRequest::with_options(
        package_identity(),
        vec![source_input(source, version)],
        options,
    )
}
