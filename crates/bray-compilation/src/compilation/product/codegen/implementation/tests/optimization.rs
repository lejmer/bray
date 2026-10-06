use super::support::compilation::{codegen_compilation, test_product_identity};
use super::support::dependencies::{
    GenericDependencyFixture, generic_consumer_for_target_with_source,
    generic_dependency_from_fixture,
};
use super::support::mappings::realize_codegen_mappings;
use crate::compilation::product::specialization::ConcreteCodegenReachability;
use crate::{CancellationToken, SelectedTarget};
use bray_codegen::{
    CodegenOptions, CodegenPartitionPolicy, CodegenSpecialization, partition_codegen_units,
};
use bray_ir::{MirBinaryOperator, MirOperationKind};
use bray_symbols::{ConstantTermData, ConstantValueKind, GenericArgument};
use std::collections::BTreeSet;

#[test]
fn scalar_and_nullable_branches_prune_native_demand_only_when_enabled() {
    let (_, compilation) = codegen_compilation(concat!(
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
fn numeric_constant_branch_removes_its_native_dependency() {
    let (_, compilation) = codegen_compilation(concat!(
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
    ));

    assert!(compilation.check_diagnostics().is_empty());

    let cancellation = CancellationToken::new();

    let target = compilation
        .selected_target()
        .target()
        .codegen_target()
        .unwrap();

    let semantic = compilation.product_semantics().unwrap();

    let roots = compilation
        .product_root_instances(semantic.value(), None, &target, &cancellation)
        .unwrap();

    let none = compilation
        .codegen_reachability(
            roots.clone(),
            None,
            &target,
            CodegenOptions::default(),
            false,
            &cancellation,
        )
        .unwrap();

    let basic = compilation
        .codegen_reachability(
            roots.clone(),
            None,
            &target,
            crate::BuildConfiguration::Development.codegen_options(),
            false,
            &cancellation,
        )
        .unwrap();

    let full = compilation
        .codegen_reachability(
            roots,
            None,
            &target,
            crate::BuildConfiguration::Release.codegen_options(),
            false,
            &cancellation,
        )
        .unwrap();

    assert_eq!(none.graph().instances().len(), 2);
    assert_eq!(basic.graph().instances().len(), 1);
    assert_eq!(full.graph().instances().len(), 1);

    assert_eq!(
        basic.graph().instances()[0].mir(),
        full.graph().instances()[0].mir()
    );

    assert!(basic.graph().instances()[0].mir().is_valid());
}

#[test]
fn repeated_scalar_expression_uses_one_dominating_result() {
    let (_, compilation) = codegen_compilation(concat!(
        "module app;\n",
        "func main() -> i32\n",
        "{\n",
        "    return (6 * 7) + (6 * 7);\n",
        "}\n",
    ));

    assert!(compilation.check_diagnostics().is_empty());

    let cancellation = CancellationToken::new();

    let target = compilation
        .selected_target()
        .target()
        .codegen_target()
        .unwrap();

    let semantic = compilation.product_semantics().unwrap();

    let roots = compilation
        .product_root_instances(semantic.value(), None, &target, &cancellation)
        .unwrap();

    let none = compilation
        .codegen_reachability(
            roots.clone(),
            None,
            &target,
            CodegenOptions::default(),
            false,
            &cancellation,
        )
        .unwrap();

    let basic = compilation
        .codegen_reachability(
            roots,
            None,
            &target,
            crate::BuildConfiguration::Development.codegen_options(),
            false,
            &cancellation,
        )
        .unwrap();

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
fn repeated_scalar_expressions_in_one_large_block_share_one_result() {
    let mut source = String::from("module app;\nfunc main() -> i32 {\n");

    for index in 0..256 {
        source.push_str(&format!("let value{index}: i32 = 6 * 7;\n"));
    }

    source.push_str("return value255;\n}\n");

    let (_, compilation) = codegen_compilation(&source);

    assert!(compilation.check_diagnostics().is_empty());

    let cancellation = CancellationToken::new();

    let target = compilation
        .selected_target()
        .target()
        .codegen_target()
        .unwrap();

    let semantic = compilation.product_semantics().unwrap();

    let roots = compilation
        .product_root_instances(semantic.value(), None, &target, &cancellation)
        .unwrap();

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
        .unwrap();

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
fn runtime_division_by_zero_remains_in_optimized_mir() {
    let (_, compilation) = codegen_compilation(concat!(
        "module app;\n",
        "func main() -> i32\n",
        "{\n",
        "    let numerator: i32 = 6;\n",
        "    let denominator: i32 = 0;\n",
        "    return numerator / denominator;\n",
        "}\n",
    ));

    assert!(compilation.check_diagnostics().is_empty());

    let cancellation = CancellationToken::new();

    let target = compilation
        .selected_target()
        .target()
        .codegen_target()
        .unwrap();

    let semantic = compilation.product_semantics().unwrap();

    let roots = compilation
        .product_root_instances(semantic.value(), None, &target, &cancellation)
        .unwrap();

    for options in [
        CodegenOptions::default(),
        crate::BuildConfiguration::Development.codegen_options(),
    ] {
        let reachability = compilation
            .codegen_reachability(roots.clone(), None, &target, options, false, &cancellation)
            .unwrap();

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
fn unknown_join_and_loop_values_keep_reachable_calls() {
    let (_, compilation) = codegen_compilation(concat!(
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
    ));

    assert!(compilation.check_diagnostics().is_empty());

    let cancellation = CancellationToken::new();

    let target = compilation
        .selected_target()
        .target()
        .codegen_target()
        .unwrap();

    let semantic = compilation.product_semantics().unwrap();

    let roots = compilation
        .product_root_instances(semantic.value(), None, &target, &cancellation)
        .unwrap();

    let basic = compilation
        .codegen_reachability(
            roots,
            None,
            &target,
            crate::BuildConfiguration::Development.codegen_options(),
            false,
            &cancellation,
        )
        .unwrap();

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

    let (_, compilation) = codegen_compilation(source);

    let cancellation = CancellationToken::new();

    let target = compilation
        .selected_target()
        .target()
        .codegen_target()
        .unwrap();

    let semantic = compilation.product_semantics().unwrap();

    let roots = compilation
        .product_root_instances(semantic.value(), None, &target, &cancellation)
        .unwrap();

    let none = compilation
        .codegen_reachability(
            roots.clone(),
            None,
            &target,
            CodegenOptions::default(),
            false,
            &cancellation,
        )
        .unwrap();

    let basic = compilation
        .codegen_reachability(
            roots,
            None,
            &target,
            crate::BuildConfiguration::Development.codegen_options(),
            false,
            &cancellation,
        )
        .unwrap();

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
            runtime_frames: None,
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
        .unwrap();

    let semantic = compilation.product_semantics().unwrap();

    let roots = compilation
        .product_root_instances(semantic.value(), None, &target, &cancellation)
        .unwrap();

    let none = compilation
        .codegen_reachability(
            roots.clone(),
            None,
            &target,
            CodegenOptions::default(),
            false,
            &cancellation,
        )
        .unwrap();

    let basic = compilation
        .codegen_reachability(
            roots,
            None,
            &target,
            crate::BuildConfiguration::Development.codegen_options(),
            false,
            &cancellation,
        )
        .unwrap();

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
            runtime_frames: None,
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
        .unwrap();

    let semantic = compilation.product_semantics().unwrap();

    let roots = compilation
        .product_root_instances(semantic.value(), None, &target, &cancellation)
        .unwrap();

    let none = compilation
        .codegen_reachability(
            roots.clone(),
            None,
            &target,
            CodegenOptions::default(),
            false,
            &cancellation,
        )
        .unwrap();

    let basic = compilation
        .codegen_reachability(
            roots,
            None,
            &target,
            crate::BuildConfiguration::Development.codegen_options(),
            false,
            &cancellation,
        )
        .unwrap();

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

    let values = compilation.semantic_value_store().unwrap();
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
