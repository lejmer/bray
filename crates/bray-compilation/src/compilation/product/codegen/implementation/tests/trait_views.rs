use super::support::artifacts::unoptimized_library_ir;

#[test]
#[ignore = "tier:native"]
fn borrowed_trait_view_emits_async_runtime_dispatch() {
    let ir = unoptimized_library_ir(
        r#"
        module app;

        using sink_value;

        trait Value
        {
            async func read() -> i32;
        }

        struct Sink
        {
            marker: i32;
        }

        impl sink_value = Sink(Value)
        {
            async func read() -> i32
            {
                return self.marker;
            }
        }

        async func read(pos selected: &view Value) -> i32
        {
            return await selected.read();
        }

        async func exercise()
        {
            let sink = Sink
            {
                marker = 42,
            };

            let selected: &view Value = &sink;
            let result = await selected.read();
        }
        "#,
    );

    assert!(ir.contains("view.witness"), "{ir}");
    assert!(ir.contains("call.indirect"), "{ir}");
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
#[ignore = "tier:native"]
fn borrowed_trait_view_dispatches_distinct_implementations_natively() {
    use super::support::compilation::codegen_compilation_for_product;

    let (backend, compilation) = codegen_compilation_for_product(
        r#"
        module app;

        using first_value;
        using second_value;

        trait Value
        {
            func seed() -> i32
                executes(pure, total);

            func read(pos fallback: i32 = self.seed(), adjusted: i32 = fallback) -> i32
                executes(pure, total);
        }

        struct First
        {
            marker: i32;
        }

        struct Second
        {
            marker: i32;
        }

        impl first_value = First(Value)
        {
            func seed() -> i32
                executes(pure, total)
            {
                return self.marker;
            }

            func read(pos fallback: i32 = 0, adjusted: i32 = 0) -> i32
                executes(pure, total)
            {
                return adjusted;
            }
        }

        impl second_value = Second(Value)
        {
            func seed() -> i32
                executes(pure, total)
            {
                return 31;
            }

            func read(pos fallback: i32 = 0, adjusted: i32 = 0) -> i32
                executes(pure, total)
            {
                if adjusted == 44
                {
                    return 45;
                }

                return adjusted;
            }
        }

        func read(pos value: &view Value) -> i32
            executes(pure)
        {
            return value.read();
        }

        func read_explicit(pos value: &view Value) -> i32
            executes(pure)
        {
            return value.read(44);
        }

        func exercise() -> i32
            executes(pure)
        {
            let first = First
            {
                marker = 11,
            };

            let second = Second
            {
                marker = 7,
            };

            let left: &view Value = &first;
            let right: &view Value = &second;

            if read(left) == 11 && read(right) == 31 && read_explicit(left) == 44 && read_explicit(right) == 45
            {
                return 0;
            }

            return 1;
        }
        "#,
        bray_symbols::ProductKind::Library,
    );

    assert_trait_view_call_guarantees(&compilation, "read", 1);
    assert_trait_view_call_guarantees(&compilation, "read_explicit", 1);
    assert_native_exercise(&backend, &compilation);
}

#[test]
#[ignore = "tier:native"]
fn borrowed_trait_view_preserves_imported_generic_witness_and_slot_order() {
    use super::support::compilation::codegen_configuration;
    use super::support::dependencies::{GenericDependencyFixture, generic_dependency_from_fixture};

    let dependency = generic_dependency_from_fixture(
        true,
        false,
        GenericDependencyFixture {
            source: r#"
            module app;

            trait Value<T>
            {
                static func create() -> i32;

                func zeta() -> T
                    executes(pure, total);

                func alpha(pos fallback: T = self.zeta()) -> i32
                    executes(pure, total);
            }

            struct Cell
            {
                value: i32;
            }

            impl cell_value = Cell(Value<i32>)
            {
                static func create() -> i32
                {
                    return 0;
                }

                func zeta() -> i32
                    executes(pure, total)
                {
                    return self.value;
                }

                func alpha(pos fallback: i32 = 0) -> i32
                    executes(pure, total)
                {
                    if fallback == 42
                    {
                        return 7;
                    }

                    return 8;
                }
            }

            func erase<T>(pos value: &T) -> &view Value<i32>
                with(T: Value<i32>)
                executes(pure, total)
            {
                return &value;
            }
            "#,
            runtime_frames: &[],
            executable_templates: 6,
            platform_service: None,
        },
    );

    for (promise, valid) in [("pure", true), ("pure, total", false)] {
        let (backend, codegen) = codegen_configuration();

        let request = crate::CompilationRequest::with_options(
            crate::test_support::package_identity(),
            vec![crate::test_support::source_input(
                &r#"
            module consumer;

            using example.dependency.app.Cell;
            using example.dependency.app.cell_value;
            using example.dependency.app.erase;

            func exercise() -> i32
                executes(PROMISE)
            {
                let cell = example.dependency.app.Cell
                {
                    value = 42,
                };

                let selected = example.dependency.app.erase<example.dependency.app.Cell>(&cell);

                if selected.zeta() == 42 && selected.alpha() == 7 && selected.alpha(1) == 8
                {
                    return 0;
                }

                return 1;
            }
            "#
                .replace("PROMISE", promise),
                0,
            )],
            crate::CompilationOptions::new(
                crate::WorkerBudget::serial(),
                bray_symbols::ProductKind::Library,
                crate::SelectedTarget::baseline(),
            ),
        )
        .with_dependency_interfaces([dependency.clone()]);

        let compilation = crate::Compilation::load_with_codegen(request, codegen)
            .expect("imported view consumer must load");

        assert_eq!(
            compilation.check_diagnostics().is_empty(),
            valid,
            "{promise}: {:?}",
            compilation.check_diagnostics()
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingExecutionGuaranteeNotProven,
            );

            continue;
        }

        assert_trait_view_call_guarantees(&compilation, "exercise", 3);

        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        assert_native_exercise(&backend, &compilation);

        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        {
            let plan = compilation
                .native_product_plan(
                    super::support::compilation::test_product_identity(),
                    crate::BuildConfiguration::Development,
                    None,
                    [],
                    None,
                )
                .expect("trait-view product must specialize");

            assert!(!super::support::artifacts::generated_artifacts(&backend, &plan).is_empty());
        }
    }
}

fn assert_trait_view_call_guarantees(
    compilation: &crate::Compilation,
    function: &str,
    expected: usize,
) {
    let selections = compilation
        .semantic_selections(crate::test_support::source_function_body_key(
            compilation,
            function,
        ))
        .expect("trait-view calls retain checked selections");

    let mut count = 0;

    for entry in selections.value().entries() {
        let bray_bound_tree::SemanticSelection::Call(call) = entry.selection() else {
            continue;
        };

        if !matches!(
            call.target(),
            bray_bound_tree::BoundCallableTarget::TraitView { .. }
        ) {
            continue;
        }

        let properties = call.phase_behaviors().invocation().execution_properties();

        assert!(properties.contains(&bray_symbols::ExecutionProperty::Pure));
        assert!(properties.contains(&bray_symbols::ExecutionProperty::Total));
        count += 1;
    }

    assert_eq!(
        count, expected,
        "{function} retains each carried invocation contract"
    );
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn assert_native_exercise(
    backend: &bray_codegen_llvm::LlvmCodeGenerator,
    compilation: &crate::Compilation,
) {
    use super::support::artifacts::generated_artifacts;
    use super::support::compilation::test_product_identity;

    let plan = compilation
        .native_product_plan(
            test_product_identity(),
            crate::BuildConfiguration::Development,
            None,
            [],
            None,
        )
        .expect("trait-view product must specialize");

    let root = crate::test_support::source_function_body_key(compilation, "exercise");

    let name = plan
        .units()
        .iter()
        .zip(plan.mappings())
        .find_map(|(unit, mappings)| {
            let instance = unit.instances().iter().find(|instance| {
                instance.key().template() == &bray_ir::MirUnitKey::Bound(root.clone())
            })?;

            Some(
                mappings
                    .instance_symbol(instance.key())
                    .expect("exercise instance must have a native symbol")
                    .callable_address_name()
                    .as_str()
                    .to_owned(),
            )
        })
        .expect("native entry retains its selected symbol");

    let directory = tempfile::tempdir().expect("native view fixture directory must exist");
    let mut objects = Vec::new();

    for (index, bytes) in generated_artifacts(backend, &plan).into_iter().enumerate() {
        let path = directory.path().join(format!("unit-{index}.o"));

        std::fs::write(&path, bytes).expect("view fixture object must be written");
        objects.push(path);
    }

    let source = directory.path().join("caller.c");
    let executable = directory.path().join("caller");

    let caller = format!(
        r#"#include <stdint.h>
#include <string.h>
extern int {name}(void*);
int main(void) {{
    _Alignas({align}) unsigned char outcome[{size}] = {{0}};
    int result = {name}(outcome);
    uint32_t state;
    memcpy(&state, outcome, sizeof(state));
    return state == {completed} ? result : 2;
}}
"#,
        align = std::mem::align_of::<bray_runtime_abi::NativeRunOutcome>(),
        size = std::mem::size_of::<bray_runtime_abi::NativeRunOutcome>(),
        completed = bray_runtime_abi::NativeRunState::COMPLETED.code(),
    );

    std::fs::write(&source, caller).expect("native view caller source must be written");

    let compiler = std::path::Path::new(
        bray_codegen_llvm::COMPILED_LLVM_PREFIX
            .expect("native fixture requires the configured LLVM toolchain"),
    )
    .join("bin/clang");

    let output = std::process::Command::new(compiler)
        .arg(&source)
        .args(objects)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("native view caller must link");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert_eq!(
        std::process::Command::new(executable)
            .status()
            .expect("linked view caller must execute")
            .code(),
        Some(0)
    );
}
