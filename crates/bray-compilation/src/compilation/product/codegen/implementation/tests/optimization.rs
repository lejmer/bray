use super::support::artifacts::library_ir;
use super::support::compilation::{
    codegen_compilation, source_request_with_runtime, test_product_identity,
};
use super::support::dependencies::{
    GenericDependencyFixture, generic_consumer_for_target_with_source,
    generic_dependency_from_fixture,
};
use super::support::mappings::realize_codegen_mappings;
use crate::compilation::product::specialization::ConcreteCodegenReachability;
use crate::{CancellationToken, CompilationOptions, SelectedTarget, WorkerBudget};
use bray_codegen::{
    CodegenOptions, CodegenPartitionPolicy, CodegenSpecialization, partition_codegen_units,
};
use bray_ir::{MirBinaryOperator, MirOperationKind};
use bray_symbols::{ConstantTermData, ConstantValueKind, GenericArgument, ProductKind};
use std::collections::BTreeSet;

#[test]
#[ignore = "tier:native"]
fn abnormal_bray_calls_leave_language_results_unobserved_and_native_results_defined() {
    for abi in ["", "@abi(c)"] {
        let direct = library_ir(
            &format!(
                r"
                module app;
                {abi}
                public func checked(pos allowed: bool) -> u32 {{
                    assert(allowed);
                    return 17;
                }}
                "
            ),
            bray_codegen::OptimizationLevel::None,
        );

        let indirect = library_ir(
            &format!(
                r"
                module app;
                @layout(c)
                public struct Block {{ a: u64; b: u64; c: u64; d: u64; }}
                {abi}
                func checked(pos allowed: bool) -> Block {{
                    assert(allowed);
                    return {{ a = 7, b = 7, c = 7, d = 7 }};
                }}
                {abi}
                public func forward(pos allowed: bool) -> Block {{
                    return checked(allowed);
                }}
                "
            ),
            bray_codegen::OptimizationLevel::None,
        );

        if abi.is_empty() {
            assert!(direct.contains("ret i32 undef"), "{direct}");
        } else {
            assert!(direct.contains("ret i32 0"), "{direct}");
            assert!(!direct.contains("ret i32 undef"), "{direct}");
        }

        let epilogues = indirect
            .split("outcome.propagated:")
            .skip(1)
            .map(|body| body.split("\n\n").next().expect("failure epilogue"))
            .collect::<Vec<_>>();

        assert!(!epilogues.is_empty(), "{indirect}");
        assert!(indirect.contains("call.result"), "{indirect}");

        for epilogue in epilogues {
            assert_eq!(epilogue.contains("store "), !abi.is_empty(), "{epilogue}");
        }

        assert_eq!(
            indirect.contains("zeroinitializer, ptr %call.result"),
            !abi.is_empty(),
            "{indirect}"
        );
    }
}

#[test]
#[ignore = "tier:native"]
fn full_optimization_outlines_panic_construction_from_successful_calls() {
    let ir = library_ir(
        r"
        module app;
        public func checked(pos allowed: bool, pos value: u64) -> u64 {
            assert(allowed);
            return value;
        }
        ",
        bray_codegen::OptimizationLevel::Full,
    );

    let bodies = ir.split("\ndefine ").skip(1).collect::<Vec<_>>();

    assert!(
        bodies.iter().any(|body| body
            .lines()
            .next()
            .is_some_and(|header| header.contains(".cold."))),
        "the uncommon panic path remains in an outlined helper: {ir}"
    );

    let successful = bodies
        .iter()
        .filter(|body| {
            body.lines()
                .next()
                .is_some_and(|header| !header.contains(".cold."))
        })
        .collect::<Vec<_>>();

    assert!(!successful.is_empty(), "{ir}");

    for body in successful {
        let body = body.split("\n}").next().expect("function body");

        assert!(
            !body.contains("@bray_runtime_panic_report_construction"),
            "the successful callable delegates panic construction to its cold helper: {body}"
        );
    }
}

#[test]
#[ignore = "tier:native"]
fn verified_normal_calls_do_not_regain_backend_outcome_checks() {
    for (contract, checks_outcome) in [("executes(pure, total)", false), ("", true)] {
        let possible_panic = if checks_outcome {
            "assert(value != 0);"
        } else {
            ""
        };

        for source in [
            format!(
                r#"
                module app;
                func identity(pos value: u32) -> u32 {contract} {{ {possible_panic} return value; }}
                public func apply(pos value: u32) -> u32 {{ return identity(value); }}
                "#
            ),
            format!(
                r#"
                module app;
                public func apply(
                    pos operation: func(pos value: u32) -> u32 {contract},
                    pos value: u32,
                ) -> u32 {{ return operation(value); }}
                "#
            ),
        ] {
            let ir = library_ir(&source, bray_codegen::OptimizationLevel::None);

            assert_eq!(ir.contains("call.outcome.state"), checks_outcome, "{ir}");

            if !checks_outcome {
                assert!(!ir.contains("call.panic.report.context"), "{ir}");
            }
        }
    }
}

#[test]
#[ignore = "tier:native"]
fn small_union_results_do_not_initialize_the_inactive_large_payload() {
    for source in [
        r"
            module app;
            public func selected(pos value: u64) -> Result<[u8; 4096], u64> {
                return Error(value);
            }
        ",
        r"
            module app;
            const SMALL: Result<[u8; 4096], u64> = Error(17);
            public func selected() -> Result<[u8; 4096], u64> {
                return SMALL;
            }
        ",
    ] {
        let ir = library_ir(source, bray_codegen::OptimizationLevel::Full);

        assert!(!ir.contains("@llvm.memset"), "{ir}");
        assert!(!ir.contains("@llvm.memcpy"), "{ir}");
        assert!(!ir.contains("alloca"), "{ir}");
    }
}

#[test]
#[ignore = "tier:native"]
fn local_union_payloads_scalarize_through_checked_physical_projections() {
    let source = r#"
        module app;
        public func round_trip(pos choose: bool, pos value: u64) -> u64 {
            let result: Result<(u64, u64, u64, u64), u64> = if choose {
                yield Ok((value, value, value, value));
            } else {
                yield Error(value);
            };
            match consume result {
                case Ok(values) { return values.2; }
                case Error(error) { return error; }
            }
        }
    "#;

    let ir = library_ir(source, bray_codegen::OptimizationLevel::Full);

    assert!(ir.contains("ret i64 %1"), "{ir}");
    assert!(!ir.contains("alloca"), "{ir}");
    assert!(!ir.contains("@llvm.memcpy"), "{ir}");
    assert!(!ir.contains("@llvm.memset"), "{ir}");
}

#[test]
#[ignore = "tier:native"]
fn completed_empty_reports_do_not_copy_or_reset_the_incoming_context() {
    let ir = library_ir(
        r"
        module app;
        public func repeat(pos operation: func() -> bool) {
            while operation() {}
        }
        ",
        bray_codegen::OptimizationLevel::None,
    );

    let empty_routes = ir
        .split("\ncall.report.empty:")
        .skip(1)
        .map(|route| route.split("\n\n").next().expect("report route"))
        .collect::<Vec<_>>();

    assert!(!empty_routes.is_empty(), "{ir}");

    for route in empty_routes {
        assert!(!route.contains("memcpy"), "{route}");
        assert_eq!(route.matches("store ").count(), 1, "{route}");
        assert!(route.contains("zeroinitializer"), "{route}");
    }

    assert!(ir.contains("call.report.owned:"), "{ir}");
    assert!(ir.contains("call.outcome.report"), "{ir}");
}

#[test]
#[ignore = "tier:native"]
fn empty_normal_reports_do_not_write_the_callers_outcome() {
    let source = r#"
        module app;
        struct Resource {
            value: bool;
            destruct() executes(pure, total) {}
        }
        public func apply(pos value: bool) {
            let owner: Resource = { value = value };
        }
    "#;

    let ir = library_ir(source, bray_codegen::OptimizationLevel::Full);

    let bodies =
        ir.split("\ndefine ")
            .skip(1)
            .filter(|function| {
                function.lines().next().is_some_and(|signature| {
                    signature.contains("(i1 ") || signature.contains("(i8 ")
                })
            })
            .collect::<Vec<_>>();

    assert!(!bodies.is_empty(), "{ir}");

    for function in bodies {
        let body = function
            .split_once("\n}")
            .expect("LLVM definition has a complete body")
            .0;

        assert!(
            !body.lines().any(|line| {
                line.contains("zeroinitializer, ptr %1,")
                    || line.contains("llvm.memset") && line.contains(" %1,")
                    || line.trim_start().starts_with("store ") && line.contains(" 0, ptr %1,")
            }),
            "{body}"
        );
    }
}

#[test]
#[ignore = "tier:native"]
fn host_volatile_contracts_bind_memory_predicates_for_scalars_and_pointers() {
    for element in ["u8", "DataPointer<u8>"] {
        let ir = library_ir(
            &format!(
                r"
                trusted module app;
                using core.memory;
                using core.target;
                public trusted func transfer(
                    pos source: DataPointer<{element}>,
                    pos destination: DataPointer<{element}>
                ) -> {element}
                    requires(
                        trusted core.memory.valid_read<{element}>(pointer = source, count = 1),
                        trusted core.memory.aligned_for<{element}>(pointer = source),
                        trusted core.memory.initialized_as<{element}>(pointer = source),
                        trusted core.memory.valid_write<{element}>(pointer = destination, count = 1),
                        trusted core.memory.aligned_for<{element}>(pointer = destination),
                    )
                    uses(raw_memory, unchecked_init)
                {{
                    let value: {element} = trusted core.target.volatile_load<{element}>(source);
                    trusted core.target.volatile_store<{element}>(destination, value);
                    return value;
                }}
                "
            ),
            bray_codegen::OptimizationLevel::None,
        );

        assert_eq!(ir.matches("load volatile ").count(), 1, "{ir}");
        assert_eq!(ir.matches("store volatile ").count(), 1, "{ir}");
    }
}

#[test]
#[ignore = "tier:native"]
fn pure_assembly_preserves_read_only_memory_inputs() {
    for (options, clobbers, read_only) in [(1, "", true), (0, "memory", false)] {
        let source = format!(
            r#"
            trusted module app;
            using core.memory;
            using core.target;
            public trusted func observe(pos pointer: DataPointer<[u8; 16]>, pos length: usize) -> usize
                requires(
                    trusted core.memory.valid_read<[u8; 16]>(pointer = pointer, count = 1),
                    trusted core.memory.aligned_for<[u8; 16]>(pointer = pointer),
                    trusted core.memory.initialized_as<[u8; 16]>(pointer = pointer),
                )
                uses(device_memory, intrinsic, raw_memory, unchecked_alias, unchecked_init)
            {{
                let observed: (usize, ) = trusted core.target.assembly<
                    (usize, DataPointer<[u8; 16]>), (usize, )
                >(
                    template = "",
                    constraints = "+reg,m",
                    clobbers = "{clobbers}",
                    features = "",
                    options = {options},
                    inputs = (length, pointer),
                );
                return observed.0;
            }}
            "#
        );

        let ir = library_ir(&source, bray_codegen::OptimizationLevel::Full);

        let module = ir
            .split("; ModuleID =")
            .find(|module| module.contains(" asm ") && module.contains("*m"))
            .expect("assembly module");

        let assembly = module
            .lines()
            .find(|line| line.contains(" asm ") && line.contains("*m"))
            .expect("the complete memory input remains observable");

        let group = assembly
            .rsplit_once(" #")
            .expect("assembly attributes")
            .1
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>();

        let attributes = module
            .lines()
            .find(|line| line.starts_with(&format!("attributes #{group} =")))
            .unwrap_or_else(|| panic!("assembly attribute definition: {ir}"));

        assert_eq!(attributes.contains("memory(read)"), read_only, "{ir}");
        assert!(assembly.contains("elementtype([16 x i8])"), "{ir}");
    }
}

#[test]
#[ignore = "tier:native"]
fn large_repeated_bytes_use_one_bulk_initialization_before_optimization() {
    for element in ["u8", "i8"] {
        let source = format!(
            r#"
            module application;

            public func repeated(pos value: {element}) -> [{element}; 4096]
            {{
                return [value; 4096];
            }}
            "#
        );

        let ir = library_ir(&source, bray_codegen::OptimizationLevel::None);

        let fills = ir
            .lines()
            .filter(|line| line.contains("call void @llvm.memset"))
            .collect::<Vec<_>>();

        assert_eq!(fills.len(), 1, "{ir}");
        assert!(fills[0].contains("i64 4096"), "{ir}");
        assert_eq!(ir.matches("store i8").count(), 1, "{ir}");
        assert!(!ir.contains("[4096 x i8] zeroinitializer"), "{ir}");
    }
}

#[test]
#[ignore = "tier:native"]
fn scalar_and_nullable_branches_prune_native_demand_only_when_enabled() {
    let (_, compilation) = codegen_compilation(source_request_with_runtime(
        &[concat!(
            "module app;\n",
            "\n",
            "func unused()\n",
            "{\n",
            "}\n",
            "\n",
            "func main()\n",
            "{\n",
            "    let enabled: bool = false;\n",
            "    if enabled\n",
            "    {\n",
            "        unused();\n",
            "    }\n",
            "    let absent: i32? = none;\n",
            "    if absent.is_present()\n",
            "    {\n",
            "        unused();\n",
            "    }\n",
            "}\n",
        )],
        CompilationOptions::new(
            WorkerBudget::serial(),
            ProductKind::Executable,
            SelectedTarget::baseline(),
        ),
    ));

    let cancellation = CancellationToken::new();

    let target = compilation
        .selected_target()
        .target()
        .codegen_target()
        .expect("test target must validate");

    let semantic = compilation
        .product_semantics()
        .expect("test product must resolve");

    let roots = compilation
        .product_root_instances(semantic.value(), None, &target, &cancellation)
        .expect("test roots must resolve");

    let none = compilation
        .codegen_reachability(
            roots.clone(),
            None,
            &target,
            CodegenOptions::default(),
            false,
            &cancellation,
        )
        .expect("unoptimized reachability must close");

    let basic = compilation
        .codegen_reachability(
            roots.clone(),
            None,
            &target,
            crate::BuildConfiguration::Development.codegen_options(),
            false,
            &cancellation,
        )
        .expect("development reachability must close");

    let full = compilation
        .codegen_reachability(
            roots,
            None,
            &target,
            crate::BuildConfiguration::Release.codegen_options(),
            false,
            &cancellation,
        )
        .expect("release reachability must close");

    assert_eq!(none.graph().instances().len(), 2);
    assert_eq!(basic.graph().instances().len(), 1);
    assert_eq!(full.graph().instances().len(), 1);
    assert!(basic.graph().instances()[0].mir().is_valid());

    let partition_work = |reachability: &ConcreteCodegenReachability| {
        let roots = reachability.graph().roots().iter().cloned().collect();

        let units = partition_codegen_units(
            CodegenPartitionPolicy::NATIVE_BALANCED,
            reachability.graph(),
            |instance| {
                Some(
                    compilation
                        .codegen_partition_compatibility(
                            instance,
                            reachability
                                .instance(instance.key())
                                .expect("retained instance must be concrete"),
                            &test_product_identity(),
                            &roots,
                            &cancellation,
                        )
                        .expect("optimized partition metadata must resolve"),
                )
            },
            |mir| {
                crate::compilation::product::mir_content_identity(&compilation, mir)
                    .expect("optimized partition content must resolve")
            },
        )
        .expect("optimized semantic demand must partition");

        units
            .iter()
            .map(|unit| unit.estimated_work().units())
            .sum::<u64>()
    };

    assert!(partition_work(&none) > partition_work(&basic));
    assert_eq!(partition_work(&basic), partition_work(&full));

    assert_eq!(
        basic.graph().instances()[0].mir(),
        full.graph().instances()[0].mir()
    );
}

#[test]
#[ignore = "tier:native"]
fn numeric_constant_branch_removes_its_native_dependency() {
    let (_, compilation) = codegen_compilation(source_request_with_runtime(
        &[concat!(
            "module app;\n",
            "func unused() {}\n",
            "func main()\n",
            "{\n",
            "    let seed: i32 = 6;\n",
            "    let value: i32 = seed * 7;\n",
            "    if value != 42\n",
            "    {\n",
            "        unused();\n",
            "    }\n",
            "}\n",
        )],
        CompilationOptions::new(
            WorkerBudget::serial(),
            ProductKind::Executable,
            SelectedTarget::baseline(),
        ),
    ));

    assert!(compilation.check_diagnostics().is_empty());

    let cancellation = CancellationToken::new();

    let target = compilation
        .selected_target()
        .target()
        .codegen_target()
        .expect("fixture target must support code generation");

    let semantic = compilation
        .product_semantics()
        .expect("fixture product semantics must resolve");

    let roots = compilation
        .product_root_instances(semantic.value(), None, &target, &cancellation)
        .expect("fixture product roots must resolve");

    let none = compilation
        .codegen_reachability(
            roots.clone(),
            None,
            &target,
            CodegenOptions::default(),
            false,
            &cancellation,
        )
        .expect("fixture product must resolve code-generation reachability");

    let basic = compilation
        .codegen_reachability(
            roots.clone(),
            None,
            &target,
            crate::BuildConfiguration::Development.codegen_options(),
            false,
            &cancellation,
        )
        .expect("fixture product must resolve code-generation reachability");

    let full = compilation
        .codegen_reachability(
            roots,
            None,
            &target,
            crate::BuildConfiguration::Release.codegen_options(),
            false,
            &cancellation,
        )
        .expect("fixture product must resolve code-generation reachability");

    // Checking already establishes that this source branch is unreachable.
    assert_eq!(none.graph().instances().len(), 1);
    assert_eq!(basic.graph().instances().len(), 1);
    assert_eq!(full.graph().instances().len(), 1);

    assert_eq!(
        basic.graph().instances()[0].mir(),
        full.graph().instances()[0].mir()
    );

    assert!(basic.graph().instances()[0].mir().is_valid());
}

#[test]
#[ignore = "tier:native"]
fn repeated_scalar_expression_uses_one_dominating_result() {
    let (_, compilation) = codegen_compilation(source_request_with_runtime(
        &[concat!(
            "module app;\n",
            "func main() -> i32\n",
            "{\n",
            "    return (6 * 7) + (6 * 7);\n",
            "}\n",
        )],
        CompilationOptions::new(
            WorkerBudget::serial(),
            ProductKind::Executable,
            SelectedTarget::baseline(),
        ),
    ));

    assert!(compilation.check_diagnostics().is_empty());

    let cancellation = CancellationToken::new();

    let target = compilation
        .selected_target()
        .target()
        .codegen_target()
        .expect("fixture target must support code generation");

    let semantic = compilation
        .product_semantics()
        .expect("fixture product semantics must resolve");

    let roots = compilation
        .product_root_instances(semantic.value(), None, &target, &cancellation)
        .expect("fixture product roots must resolve");

    let none = compilation
        .codegen_reachability(
            roots.clone(),
            None,
            &target,
            CodegenOptions::default(),
            false,
            &cancellation,
        )
        .expect("fixture product must resolve code-generation reachability");

    let basic = compilation
        .codegen_reachability(
            roots,
            None,
            &target,
            crate::BuildConfiguration::Development.codegen_options(),
            false,
            &cancellation,
        )
        .expect("fixture product must resolve code-generation reachability");

    let multiply_count = |graph: &bray_codegen::CodegenReachability| {
        graph
            .instances()
            .iter()
            .flat_map(|instance| instance.mir().operations())
            .filter(|operation| {
                matches!(
                    operation.kind(),
                    MirOperationKind::Binary {
                        operator: MirBinaryOperator::Multiply,
                        ..
                    }
                )
            })
            .count()
    };

    assert_eq!(multiply_count(none.graph()), 2);
    assert_eq!(multiply_count(basic.graph()), 1);

    assert!(
        basic
            .graph()
            .instances()
            .iter()
            .all(|instance| instance.mir().is_valid())
    );
}

#[test]
#[ignore = "tier:native"]
fn repeated_scalar_expressions_in_one_large_block_share_one_result() {
    let mut source = String::from("module app;\nfunc main() -> i32 {\n");

    for index in 0..256 {
        source.push_str(&format!("let value{index}: i32 = 6 * 7;\n"));
    }

    source.push_str("return value255;\n}\n");

    let (_, compilation) = codegen_compilation(source_request_with_runtime(
        &[&source],
        CompilationOptions::new(
            WorkerBudget::serial(),
            ProductKind::Executable,
            SelectedTarget::baseline(),
        ),
    ));

    assert!(compilation.check_diagnostics().is_empty());

    let cancellation = CancellationToken::new();

    let target = compilation
        .selected_target()
        .target()
        .codegen_target()
        .expect("fixture target must support code generation");

    let semantic = compilation
        .product_semantics()
        .expect("fixture product semantics must resolve");

    let roots = compilation
        .product_root_instances(semantic.value(), None, &target, &cancellation)
        .expect("fixture product roots must resolve");

    let start = std::time::Instant::now();

    let basic = compilation
        .codegen_reachability(
            roots,
            None,
            &target,
            crate::BuildConfiguration::Development.codegen_options(),
            false,
            &cancellation,
        )
        .expect("fixture product must resolve code-generation reachability");

    eprintln!("256 repeated scalar expressions: {:?}", start.elapsed());

    let multiply_count = basic
        .graph()
        .instances()
        .iter()
        .flat_map(|instance| instance.mir().operations())
        .filter(|operation| {
            matches!(
                operation.kind(),
                MirOperationKind::Binary {
                    operator: MirBinaryOperator::Multiply,
                    ..
                }
            )
        })
        .count();

    assert_eq!(multiply_count, 1);

    assert!(
        basic
            .graph()
            .instances()
            .iter()
            .all(|instance| instance.mir().is_valid())
    );
}

#[test]
#[ignore = "tier:native"]
fn runtime_division_by_zero_remains_in_optimized_mir() {
    let (_, compilation) = codegen_compilation(source_request_with_runtime(
        &[concat!(
            "module app;\n",
            "func main() -> i32\n",
            "{\n",
            "    let numerator: i32 = 6;\n",
            "    let denominator: i32 = 0;\n",
            "    return numerator / denominator;\n",
            "}\n",
        )],
        CompilationOptions::new(
            WorkerBudget::serial(),
            ProductKind::Executable,
            SelectedTarget::baseline(),
        ),
    ));

    assert!(compilation.check_diagnostics().is_empty());

    let cancellation = CancellationToken::new();

    let target = compilation
        .selected_target()
        .target()
        .codegen_target()
        .expect("fixture target must support code generation");

    let semantic = compilation
        .product_semantics()
        .expect("fixture product semantics must resolve");

    let roots = compilation
        .product_root_instances(semantic.value(), None, &target, &cancellation)
        .expect("fixture product roots must resolve");

    for options in [
        CodegenOptions::default(),
        crate::BuildConfiguration::Development.codegen_options(),
    ] {
        let reachability = compilation
            .codegen_reachability(roots.clone(), None, &target, options, false, &cancellation)
            .expect("fixture product must resolve code-generation reachability");

        assert!(
            reachability
                .graph()
                .instances()
                .iter()
                .flat_map(|instance| instance.mir().operations())
                .any(|operation| matches!(
                    operation.kind(),
                    MirOperationKind::Binary {
                        operator: MirBinaryOperator::Divide,
                        ..
                    }
                ))
        );
    }
}

#[test]
#[ignore = "tier:native"]
fn unknown_join_and_loop_values_keep_reachable_calls() {
    let (_, compilation) = codegen_compilation(source_request_with_runtime(
        &[concat!(
            "module app;\n",
            "func used() {}\n",
            "func gate(flag: bool)\n",
            "{\n",
            "    let mut selected: bool = false;\n",
            "    if flag\n",
            "    {\n",
            "        selected = true;\n",
            "    }\n",
            "    while flag\n",
            "    {\n",
            "        selected = true;\n",
            "    }\n",
            "    if selected\n",
            "    {\n",
            "        used();\n",
            "    }\n",
            "}\n",
            "func main() { gate(flag = false); }\n",
        )],
        CompilationOptions::new(
            WorkerBudget::serial(),
            ProductKind::Executable,
            SelectedTarget::baseline(),
        ),
    ));

    assert!(compilation.check_diagnostics().is_empty());

    let cancellation = CancellationToken::new();

    let target = compilation
        .selected_target()
        .target()
        .codegen_target()
        .expect("fixture target must support code generation");

    let semantic = compilation
        .product_semantics()
        .expect("fixture product semantics must resolve");

    let roots = compilation
        .product_root_instances(semantic.value(), None, &target, &cancellation)
        .expect("fixture product roots must resolve");

    let basic = compilation
        .codegen_reachability(
            roots,
            None,
            &target,
            crate::BuildConfiguration::Development.codegen_options(),
            false,
            &cancellation,
        )
        .expect("fixture product must resolve code-generation reachability");

    assert_eq!(basic.graph().instances().len(), 3);

    assert!(
        basic
            .graph()
            .instances()
            .iter()
            .all(|instance| instance.mir().is_valid())
    );
}

#[test]
#[ignore = "tier:native"]
fn generic_scalar_branches_follow_each_concrete_substitution() {
    let source = concat!(
        "module app;\n",
        "func unused() {}\n",
        "func gate<const enabled: bool>()\n",
        "{\n",
        "    if enabled\n",
        "    {\n",
        "        unused();\n",
        "    }\n",
        "}\n",
        "func main()\n",
        "{\n",
        "    gate<false>();\n",
        "    gate<true>();\n",
        "}\n",
    );

    let (_, compilation) = codegen_compilation(source_request_with_runtime(
        &[source],
        CompilationOptions::new(
            WorkerBudget::serial(),
            ProductKind::Executable,
            SelectedTarget::baseline(),
        ),
    ));

    let cancellation = CancellationToken::new();

    let target = compilation
        .selected_target()
        .target()
        .codegen_target()
        .expect("fixture target must support code generation");

    let semantic = compilation
        .product_semantics()
        .expect("fixture product semantics must resolve");

    let roots = compilation
        .product_root_instances(semantic.value(), None, &target, &cancellation)
        .expect("fixture product roots must resolve");

    let none = compilation
        .codegen_reachability(
            roots.clone(),
            None,
            &target,
            CodegenOptions::default(),
            false,
            &cancellation,
        )
        .expect("fixture product must resolve code-generation reachability");

    let basic = compilation
        .codegen_reachability(
            roots,
            None,
            &target,
            crate::BuildConfiguration::Development.codegen_options(),
            false,
            &cancellation,
        )
        .expect("fixture product must resolve code-generation reachability");

    assert_eq!(none.graph().instances().len(), 4);
    assert_eq!(basic.graph().instances().len(), 4);

    assert!(
        basic
            .graph()
            .instances()
            .iter()
            .all(|instance| instance.mir().is_valid())
    );

    realize_codegen_mappings(&compilation, &target, &none, &cancellation);

    assert_boolean_specialization_dependencies(&compilation, &basic);
}

#[test]
#[ignore = "tier:native"]
fn imported_generic_constant_prunes_its_transitive_native_dependency() {
    let dependency = generic_dependency_from_fixture(
        true,
        false,
        GenericDependencyFixture {
            source: concat!(
                "module templates;\n",
                "func called() {}\n",
                "public func gate<const enabled: bool>()\n",
                "{\n",
                "    if enabled\n",
                "    {\n",
                "        called();\n",
                "    }\n",
                "}\n",
            ),
            runtime_frames: &[],
            executable_templates: 2,
            platform_service: None,
        },
    );

    let compilation = generic_consumer_for_target_with_source(
        dependency,
        SelectedTarget::baseline(),
        "module app; using example.dependency.templates.gate; func main() { example.dependency.templates.gate<false>(); example.dependency.templates.gate<true>(); }",
    );

    assert!(compilation.check_diagnostics().is_empty());

    let cancellation = CancellationToken::new();

    let target = compilation
        .selected_target()
        .target()
        .codegen_target()
        .expect("fixture target must support code generation");

    let semantic = compilation
        .product_semantics()
        .expect("fixture product semantics must resolve");

    let roots = compilation
        .product_root_instances(semantic.value(), None, &target, &cancellation)
        .expect("fixture product roots must resolve");

    let none = compilation
        .codegen_reachability(
            roots.clone(),
            None,
            &target,
            CodegenOptions::default(),
            false,
            &cancellation,
        )
        .expect("fixture product must resolve code-generation reachability");

    let basic = compilation
        .codegen_reachability(
            roots,
            None,
            &target,
            crate::BuildConfiguration::Development.codegen_options(),
            false,
            &cancellation,
        )
        .expect("fixture product must resolve code-generation reachability");

    assert_eq!(none.graph().instances().len(), 4);
    assert_eq!(basic.graph().instances().len(), 4);

    assert!(
        basic
            .graph()
            .instances()
            .iter()
            .all(|instance| instance.mir().is_valid())
    );

    realize_codegen_mappings(&compilation, &target, &none, &cancellation);

    assert_boolean_specialization_dependencies(&compilation, &basic);
}

#[test]
#[ignore = "tier:native"]
fn imported_numeric_generic_constant_prunes_only_its_false_branch() {
    let dependency = generic_dependency_from_fixture(
        true,
        false,
        GenericDependencyFixture {
            source: concat!(
                "module templates;\n",
                "func called() {}\n",
                "public func gate<const seed: i32>()\n",
                "{\n",
                "    if seed * 7 != 42\n",
                "    {\n",
                "        called();\n",
                "    }\n",
                "}\n",
            ),
            runtime_frames: &[],
            executable_templates: 2,
            platform_service: None,
        },
    );

    let compilation = generic_consumer_for_target_with_source(
        dependency,
        SelectedTarget::baseline(),
        "module app; using example.dependency.templates.gate; func main() { example.dependency.templates.gate<6>(); example.dependency.templates.gate<7>(); }",
    );

    assert!(compilation.check_diagnostics().is_empty());

    let cancellation = CancellationToken::new();

    let target = compilation
        .selected_target()
        .target()
        .codegen_target()
        .expect("fixture target must support code generation");

    let semantic = compilation
        .product_semantics()
        .expect("fixture product semantics must resolve");

    let roots = compilation
        .product_root_instances(semantic.value(), None, &target, &cancellation)
        .expect("fixture product roots must resolve");

    let none = compilation
        .codegen_reachability(
            roots.clone(),
            None,
            &target,
            CodegenOptions::default(),
            false,
            &cancellation,
        )
        .expect("fixture product must resolve code-generation reachability");

    let basic = compilation
        .codegen_reachability(
            roots,
            None,
            &target,
            crate::BuildConfiguration::Development.codegen_options(),
            false,
            &cancellation,
        )
        .expect("fixture product must resolve code-generation reachability");

    assert_eq!(none.graph().instances().len(), 4);
    assert_eq!(basic.graph().instances().len(), 4);

    let generic = basic
        .graph()
        .instances()
        .iter()
        .filter(|instance| {
            matches!(
                instance.key().specialization(),
                CodegenSpecialization::Generic(_)
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(generic.len(), 2);
    assert_ne!(generic[0].mir(), generic[1].mir());

    assert_eq!(
        generic
            .iter()
            .map(|instance| instance.dependencies().len())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([0, 1]),
    );
}

fn assert_boolean_specialization_dependencies(
    compilation: &crate::Compilation,
    reachability: &ConcreteCodegenReachability,
) {
    let generic = reachability
        .graph()
        .instances()
        .iter()
        .filter(|instance| {
            matches!(
                instance.key().specialization(),
                CodegenSpecialization::Generic(_)
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(generic.len(), 2);
    assert_eq!(generic[0].key().template(), generic[1].key().template());
    assert_ne!(generic[0].mir(), generic[1].mir());

    let values = compilation
        .semantic_value_store()
        .expect("fixture compilation must provide its semantic value store");

    let mut seen = BTreeSet::new();

    for instance in generic {
        let realization = reachability
            .instance(instance.key())
            .expect("reachable specialization must have a concrete realization");

        let substitution = values.generic_substitution_data(
            realization
                .substitution()
                .expect("generic callable must carry a substitution"),
        );

        let [binding] = substitution.bindings() else {
            panic!("test gate must have exactly one constant argument");
        };

        let GenericArgument::Constant(term) = binding.argument() else {
            panic!("test gate argument must be a constant");
        };

        let term_data = values.constant_term_data(term);

        let ConstantTermData::Value(value) = term_data.as_ref() else {
            panic!("concrete test argument must have a value");
        };

        let value_data = values.constant_value_data(*value);

        let ConstantValueKind::Boolean(enabled) = value_data.kind() else {
            panic!("test gate argument must be Boolean");
        };

        assert!(seen.insert(*enabled));
        assert_eq!(instance.dependencies().len(), usize::from(*enabled));
    }
}
