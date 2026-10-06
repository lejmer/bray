use super::support::artifacts::generated_artifacts;
use super::support::runtime::{
    runtime_native_plan, runtime_native_plan_for_product,
    runtime_native_plan_for_sources_target_with_platform_overrides,
};
use crate::SelectedTarget;

use bray_ir::{MirOperationKind, MirTerminatorKind, MirUnitKey};
use bray_symbols::ProductKind;

#[test]
fn nested_cleanup_native_fixture_emits() {
    let (backend, plan) = runtime_native_plan_for_product(
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../xtask/fixtures/composition/cleanup/main.bray"
        )),
        ProductKind::Test,
    );

    assert!(!generated_artifacts(&backend, &plan).is_empty());
}

#[test]
fn replacement_cleanup_helpers_emit_checked_outcomes() {
    for lifecycle in ["destruct() {}", "finalize() {} destruct() {}"] {
        let source = format!(
            "module app; struct Resource {{ value: i32; {lifecycle} }} \
                 func main() {{ let mut value: Resource = Resource {{ value = 1 }}; \
                 value = Resource {{ value = 2 }}; }}"
        );

        let (backend, plan) = runtime_native_plan(&source);

        for mir in plan
            .units()
            .iter()
            .flat_map(bray_codegen::CodegenUnit::mir_units)
        {
            if !matches!(mir.key(), MirUnitKey::GeneratedLifecycle(_)) {
                continue;
            }

            for block in mir.blocks() {
                let Some(operation) = block.operations().last().and_then(|id| mir.operation(*id))
                else {
                    continue;
                };

                if matches!(operation.kind(), MirOperationKind::Call(call) if call.may_propagate_panic())
                {
                    assert!(matches!(
                        block.terminator().kind(),
                        MirTerminatorKind::CheckCallOutcome { .. }
                    ));
                }
            }
        }

        for symbol in plan
            .mappings()
            .iter()
            .flat_map(bray_codegen::CodegenMappings::symbols)
        {
            if matches!(symbol.key(), bray_codegen::CodegenSymbolKey::Instance(instance)
                    if matches!(instance.template(), MirUnitKey::GeneratedLifecycle(_)))
            {
                assert!(symbol.signature().has_panic_report_context());
            }
        }

        assert!(!generated_artifacts(&backend, &plan).is_empty());
    }
}

#[test]
fn library_source_bodies_are_not_skipped_by_compiler_known_names() {
    for module in ["app", "std.memory"] {
        let source = format!("module {module}; func allocate() -> i32 {{ return 3; }}");

        let (backend, plan) = runtime_native_plan_for_product(&source, ProductKind::Library);

        assert!(!plan.units().is_empty());
        assert!(!generated_artifacts(&backend, &plan).is_empty());
    }
}

#[test]
fn synchronous_finalizer_can_propagate_run_result_cancellation() {
    let source = r#"
            module app;
            struct Guard {
                finalize() {
                    let outcome: RunResult<unit> = Cancelled;
                    try outcome;
                }
                destruct() {}
            }
            async func main() {
                let mut value = Guard {};
                value = Guard {};
            }
        "#;

    for configuration in [
        crate::BuildConfiguration::Development,
        crate::BuildConfiguration::Release,
    ] {
        let (backend, plan) = runtime_native_plan_for_sources_target_with_platform_overrides(
            &[source],
            ProductKind::Executable,
            SelectedTarget::baseline(),
            &[],
            [],
            [],
            configuration,
        );

        assert!(!generated_artifacts(&backend, &plan).is_empty());
    }
}
