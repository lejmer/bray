use super::support::artifacts::assert_source_emits_valid_native_units;
use super::support::compilation::test_product_identity;
use super::support::dependencies::{
    GenericDependencyFixture, generic_consumer_for_target_with_source,
    generic_dependency_from_fixture,
};
use super::support::mappings::realize_codegen_mappings;
use crate::compilation::CodegenPreparationError;
use crate::compilation::product::codegen::NativeProductPlanningError;
use crate::{
    CancellationToken, DependencyInterfaceInput, ImportedSemanticRecordKey, SelectedTarget,
};
use bray_codegen::{CodegenGenericArgument, CodegenOptions, CodegenSpecialization};
use bray_ir::{MirOperationKind, MirUnitKey};
use bray_package_interface::{ImportedSemanticRecord, InterfaceSemanticRecordKind};
use bray_runtime_interface::PlatformServiceRole;
use bray_symbols::ProductIdentity;
use bray_target::NativeTarget;
use std::collections::BTreeSet;
use std::sync::Arc;

const SCALAR_COMPARISON_CALL_SOURCE: &str = concat!(
    "module app;\n",
    "\n",
    "func compare_values<Value>(pos left: Value, pos right: Value) -> Ordering\n",
    "    with(Value: Comparable<Value>)\n",
    "{\n",
    "    let ordering: Ordering = left.compare(&right);\n",
    "\n",
    "    return ordering;\n",
    "}\n",
    "\n",
    "public func compare_u64(pos left: u64, pos right: u64) -> Ordering\n",
    "{\n",
    "    return compare_values<u64>(left, right);\n",
    "}\n",
);

#[test]
fn imported_generic_templates_specialize_with_private_helpers_in_the_consumer() {
    let compilation = generic_consumer(generic_dependency(true));

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let cancellation = CancellationToken::new();

    let target = compilation
        .selected_target()
        .target()
        .codegen_target()
        .unwrap_or_else(|error| panic!("consumer target must validate: {error:?}"));

    let semantic = compilation
        .product_semantics()
        .unwrap_or_else(|error| panic!("consumer product plan must resolve: {error:?}"));

    let roots = compilation
        .product_root_instances(semantic.value(), None, &target, &cancellation)
        .unwrap_or_else(|error| panic!("consumer roots must resolve: {error:?}"));

    let reachability = compilation
        .codegen_reachability(
            roots,
            None,
            &target,
            CodegenOptions::default(),
            false,
            &cancellation,
        )
        .unwrap_or_else(|error| panic!("consumer reachability must close: {error:?}"));

    let imported = reachability
        .graph()
        .instances()
        .iter()
        .filter(|instance| matches!(instance.key().template(), MirUnitKey::ImportedExecutable(_)))
        .collect::<Vec<_>>();

    assert_eq!(imported.len(), 3);

    assert!(imported.iter().all(|instance| {
        matches!(
            instance.key().specialization(),
            CodegenSpecialization::Generic(arguments)
                if arguments.iter().any(|argument| {
                    matches!(argument, CodegenGenericArgument::Type(_))
                })
        )
    }));

    assert!(imported.iter().any(|instance| {
        instance.mir().operations().iter().any(|operation| {
            matches!(
                operation.kind(),
                MirOperationKind::Call(call) if call.phase_behaviors().is_some()
            )
        })
    }));

    let roots = reachability.graph().roots().iter().cloned().collect();
    let first_product = test_product_identity();

    let other_product = ProductIdentity::try_new(first_product.package().clone(), "other")
        .expect("alternate test product identity must validate");

    for instance in imported {
        let first = compilation
            .codegen_partition_compatibility(
                instance,
                reachability
                    .instance(instance.key())
                    .expect("retained instance must be concrete"),
                &first_product,
                &roots,
                &cancellation,
            )
            .expect("imported compatibility must resolve");

        let other = compilation
            .codegen_partition_compatibility(
                instance,
                reachability
                    .instance(instance.key())
                    .expect("retained instance must be concrete"),
                &other_product,
                &roots,
                &cancellation,
            )
            .expect("imported compatibility must resolve");

        assert_eq!(first, other);
    }

    realize_codegen_mappings(&compilation, &target, &reachability, &cancellation);
}

#[test]
fn imported_platform_service_templates_retain_their_role_during_specialization() {
    let role = PlatformServiceRole::StandardOutputFlush;

    let fixture = GenericDependencyFixture {
        source: r#"trusted module templates;

@layout(c)
internal struct PlatformStatus
{
    category: u32;
    reserved: u32;
    native_code: i64;
}

@abi(c)
trusted internal func flush() -> PlatformStatus
{
    return { category = 0, reserved = 0, native_code = 0 };
}

public func invoke<T>(pos value: T)
{
    let _: PlatformStatus = trusted flush();
}
"#,
        runtime_frames: None,
        executable_templates: 2,
        platform_service: Some((role, "templates.flush")),
    };

    let compilation = generic_consumer_for_target_with_source(
        generic_dependency_from_fixture(true, false, fixture),
        SelectedTarget::baseline(),
        concat!(
            "module application;\n",
            "using example.dependency.templates.invoke;\n",
            "func main() { example.dependency.templates.invoke<i32>(1); }\n",
        ),
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let cancellation = CancellationToken::new();

    let target = compilation
        .selected_target()
        .target()
        .codegen_target()
        .unwrap_or_else(|error| panic!("consumer target must validate: {error:?}"));

    let semantic = compilation
        .product_semantics()
        .unwrap_or_else(|error| panic!("consumer product plan must resolve: {error:?}"));

    let roots = compilation
        .product_root_instances(semantic.value(), None, &target, &cancellation)
        .unwrap_or_else(|error| panic!("consumer roots must resolve: {error:?}"));

    let reachability = compilation
        .codegen_reachability(
            roots,
            None,
            &target,
            CodegenOptions::default(),
            false,
            &cancellation,
        )
        .unwrap_or_else(|error| panic!("consumer reachability must close: {error:?}"));

    let roles = reachability
        .graph()
        .instances()
        .iter()
        .filter_map(|instance| match instance.key().template() {
            MirUnitKey::ImportedExecutable(key) => key.platform_service(),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(roles, [role]);

    realize_codegen_mappings(&compilation, &target, &reachability, &cancellation);
}

#[test]
fn scalar_comparison_callable_is_lowered_as_native_intrinsic() {
    assert_source_emits_valid_native_units(
        SCALAR_COMPARISON_CALL_SOURCE,
        crate::BuildConfiguration::Development,
    );
}

#[test]
fn imported_trait_default_bodies_specialize_with_the_consumer_implementation() {
    let compilation = generic_consumer_for_target_with_source(
        trait_default_dependency(),
        SelectedTarget::baseline(),
        TRAIT_DEFAULT_CONSUMER_SOURCE,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let cancellation = CancellationToken::new();

    let target = compilation
        .selected_target()
        .target()
        .codegen_target()
        .unwrap_or_else(|error| panic!("consumer target must validate: {error:?}"));

    let semantic = compilation
        .product_semantics()
        .unwrap_or_else(|error| panic!("consumer product plan must resolve: {error:?}"));

    let roots = compilation
        .product_root_instances(semantic.value(), None, &target, &cancellation)
        .unwrap_or_else(|error| panic!("consumer roots must resolve: {error:?}"));

    let reachability = compilation
        .codegen_reachability(
            roots,
            None,
            &target,
            CodegenOptions::default(),
            false,
            &cancellation,
        )
        .unwrap_or_else(|error| panic!("consumer reachability must close: {error:?}"));

    let imported_defaults = reachability
        .graph()
        .instances()
        .iter()
        .filter(|instance| {
            matches!(instance.key().template(), MirUnitKey::ImportedExecutable(_))
                && instance.key().contextual_self_witness().is_some()
        })
        .count();

    assert_eq!(imported_defaults, 4);

    realize_codegen_mappings(&compilation, &target, &reachability, &cancellation);
}

#[test]
fn imported_generic_template_families_merge_protected_frame_requirements() {
    let compilation = async_generic_consumer(generic_async_dependency());

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let address = imported_nested_template_address(&compilation).symbol();

    let runtime = compilation
        .imported_semantics(ImportedSemanticRecordKey::new(
            address.interface(),
            address.symbol(),
            InterfaceSemanticRecordKind::Runtime,
        ))
        .unwrap_or_else(|error| panic!("runtime requirement must resolve: {error:?}"));

    let [ImportedSemanticRecord::Runtime(runtime)] = runtime.value().as_ref() else {
        panic!("generic callable must import one family runtime requirement");
    };

    assert_eq!(runtime.frames().len(), 2);
    assert!(runtime.requirements().frame_abi().is_some());

    let target = compilation
        .selected_target()
        .target()
        .codegen_target()
        .unwrap_or_else(|error| panic!("consumer target must validate: {error:?}"));

    let semantic = compilation
        .product_semantics()
        .unwrap_or_else(|error| panic!("consumer product plan must resolve: {error:?}"));

    let roots = compilation
        .product_root_instances(
            semantic.value(),
            None,
            &target,
            &compilation.state.cancellation,
        )
        .unwrap_or_else(|error| panic!("consumer roots must resolve: {error:?}"));

    let reachability = compilation
        .codegen_reachability(
            roots,
            None,
            &target,
            CodegenOptions::default(),
            false,
            &compilation.state.cancellation,
        )
        .unwrap_or_else(|error| panic!("consumer reachability must close: {error:?}"));

    let frames = reachability
        .graph()
        .instances()
        .iter()
        .filter(|instance| matches!(instance.key().template(), MirUnitKey::ImportedExecutable(_)))
        .filter_map(|instance| instance.mir().frame_descriptor())
        .map(bray_ir::MirFrameDescriptor::frame)
        .collect::<BTreeSet<_>>();

    assert_eq!(frames.iter().copied().collect::<Vec<_>>(), runtime.frames());
}

#[test]
fn imported_generic_templates_are_shared_by_concurrent_requests() {
    let compilation = generic_consumer(generic_dependency(true));
    let address = imported_nested_template_address(&compilation);

    std::thread::scope(|scope| {
        let first = scope.spawn(|| {
            compilation.imported_executable_template_with_cancellation(
                address,
                &compilation.state.cancellation,
            )
        });

        let second = scope.spawn(|| {
            compilation.imported_executable_template_with_cancellation(
                address,
                &compilation.state.cancellation,
            )
        });

        let first = first
            .join()
            .unwrap_or_else(|_| panic!("first template query must not panic"))
            .unwrap_or_else(|error| panic!("first template query must complete: {error:?}"));

        let second = second
            .join()
            .unwrap_or_else(|_| panic!("second template query must not panic"))
            .unwrap_or_else(|error| panic!("second template query must complete: {error:?}"));

        assert!(Arc::ptr_eq(&first, &second));

        let first = first
            .value()
            .as_ref()
            .unwrap_or_else(|| panic!("first query must publish validated MIR"));

        let second = second
            .value()
            .as_ref()
            .unwrap_or_else(|| panic!("second query must publish validated MIR"));

        assert!(Arc::ptr_eq(first, second));
    });
}

#[test]
fn imported_executable_templates_reject_a_different_target_contract() {
    let compilation = generic_consumer_for_target(
        generic_dependency(true),
        SelectedTarget::for_native(NativeTarget::X86_64WindowsMsvc),
    );

    let result = compilation
        .imported_executable_template_with_cancellation(
            crate::fact::ImportedExecutableTemplateAddress::root(first_imported_function_address(
                &compilation,
            )),
            &compilation.state.cancellation,
        )
        .unwrap_or_else(|error| panic!("target mismatch must be diagnosed: {error:?}"));

    assert!(result.value().is_none());

    assert_eq!(
        result
            .diagnostics()
            .by_kind(bray_diagnostics::DiagnosticKind::InterfaceValidationFailed)
            .count(),
        1
    );
}

#[test]
fn malformed_imported_executable_templates_publish_dependency_diagnostics() {
    let compilation = generic_consumer(generic_dependency_with_templates(true, true));

    let result = compilation
        .imported_executable_template_with_cancellation(
            crate::fact::ImportedExecutableTemplateAddress::root(first_imported_function_address(
                &compilation,
            )),
            &compilation.state.cancellation,
        )
        .unwrap_or_else(|error| panic!("malformed template must be diagnosed: {error:?}"));

    assert!(result.value().is_none());

    assert_eq!(
        result
            .diagnostics()
            .by_kind(bray_diagnostics::DiagnosticKind::InterfaceValidationFailed)
            .count(),
        1
    );
}

#[test]
fn imported_generic_body_without_an_implementation_template_is_diagnosed() {
    let compilation = generic_consumer(generic_dependency(false));

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let cancellation = CancellationToken::new();

    let target = compilation
        .selected_target()
        .target()
        .codegen_target()
        .unwrap_or_else(|error| panic!("consumer target must validate: {error:?}"));

    let semantic = compilation
        .product_semantics()
        .unwrap_or_else(|error| panic!("consumer product plan must resolve: {error:?}"));

    let roots = compilation
        .product_root_instances(semantic.value(), None, &target, &cancellation)
        .unwrap_or_else(|error| panic!("consumer roots must resolve: {error:?}"));

    let error = match compilation.codegen_reachability(
        roots,
        None,
        &target,
        CodegenOptions::default(),
        false,
        &cancellation,
    ) {
        Ok(_) => panic!("missing imported templates must stop code generation reachability"),
        Err(error) => error,
    };

    let NativeProductPlanningError::Codegen(CodegenPreparationError::Diagnostics(diagnostics)) =
        error
    else {
        panic!("missing imported template must preserve its diagnostics: {error:?}");
    };

    assert_eq!(
        diagnostics
            .by_kind(bray_diagnostics::DiagnosticKind::InterfaceExecutableTemplateUnavailable)
            .count(),
        1
    );
}

const GENERIC_CONSUMER_SOURCE: &str = concat!(
    "module application;\n",
    "\n",
    "using example.dependency.templates.identity;\n",
    "\n",
    "func main()\n",
    "{\n",
    "    let value: i32 = example.dependency.templates.identity<i32>(1);\n",
    "}\n",
);

const ASYNC_GENERIC_CONSUMER_SOURCE: &str = concat!(
    "module application;\n",
    "\n",
    "using example.dependency.templates.identity;\n",
    "\n",
    "async func main()\n",
    "{\n",
    "    let value: i32 = await example.dependency.templates.identity<i32>(1);\n",
    "}\n",
);

const TRAIT_DEFAULT_CONSUMER_SOURCE: &str = concat!(
    "module application;\n",
    "\n",
    "using example.dependency.templates.read;\n",
    "using internal example.dependency.templates.I32Counter;\n",
    "\n",
    "func main()\n",
    "{\n",
    "    let value: i32 = 3;\n",
    "    let count: i32 = example.dependency.templates.read<i32>(&value);\n",
    "}\n",
);

const GENERIC_DEPENDENCY: GenericDependencyFixture = GenericDependencyFixture {
    source: concat!(
        "module templates;\n",
        "\n",
        "func helper<T>(pos value: T) -> T\n",
        "{\n",
        "    return value;\n",
        "}\n",
        "\n",
        "public func identity<T>(pos value: T) -> T\n",
        "{\n",
        "    let invoke = lambda(pos item: T) -> T\n",
        "    {\n",
        "        return helper<T>(item);\n",
        "    };\n",
        "\n",
        "    return invoke(value);\n",
        "}\n",
    ),
    runtime_frames: None,
    executable_templates: 3,
    platform_service: None,
};

const ASYNC_GENERIC_DEPENDENCY: GenericDependencyFixture = GenericDependencyFixture {
    source: concat!(
        "module templates;\n",
        "\n",
        "func helper<T>(pos value: T) -> T\n",
        "{\n",
        "    return value;\n",
        "}\n",
        "\n",
        "public async func identity<T>(pos value: T) -> T\n",
        "{\n",
        "    let invoke = async lambda(pos item: T) -> T\n",
        "    {\n",
        "        return helper<T>(item);\n",
        "    };\n",
        "\n",
        "    return await invoke(value);\n",
        "}\n",
    ),
    runtime_frames: Some(2),
    executable_templates: 3,
    platform_service: None,
};

const TRAIT_DEFAULT_DEPENDENCY: GenericDependencyFixture = GenericDependencyFixture {
    source: concat!(
        "module templates;\n",
        "\n",
        "public trait Counter\n",
        "{\n",
        "    func count() -> i32;\n",
        "    func doubled() -> i32\n",
        "    {\n",
        "        return self.count() + self.count();\n",
        "    }\n",
        "    func quadrupled() -> i32\n",
        "    {\n",
        "        return self.doubled() + self.through_lambda();\n",
        "    }\n",
        "    func through_lambda() -> i32\n",
        "    {\n",
        "        let invoke = lambda(pos value: &Self) -> i32\n",
        "        {\n",
        "            return value.count();\n",
        "        };\n",
        "\n",
        "        return invoke(&self);\n",
        "    }\n",
        "}\n",
        "\n",
        "public impl I32Counter = i32(Counter)\n",
        "{\n",
        "    func count() -> i32\n",
        "    {\n",
        "        return self;\n",
        "    }\n",
        "}\n",
        "\n",
        "public func read<T>(pos value: &T) -> i32\n",
        "    with(T: Counter)\n",
        "{\n",
        "    return value.quadrupled();\n",
        "}\n",
    ),
    runtime_frames: None,
    executable_templates: 6,
    platform_service: None,
};

fn generic_consumer(dependency: DependencyInterfaceInput) -> crate::Compilation {
    generic_consumer_for_target(dependency, SelectedTarget::baseline())
}

fn async_generic_consumer(dependency: DependencyInterfaceInput) -> crate::Compilation {
    generic_consumer_for_target_with_source(
        dependency,
        SelectedTarget::baseline(),
        ASYNC_GENERIC_CONSUMER_SOURCE,
    )
}

fn generic_consumer_for_target(
    dependency: DependencyInterfaceInput,
    target: SelectedTarget,
) -> crate::Compilation {
    generic_consumer_for_target_with_source(dependency, target, GENERIC_CONSUMER_SOURCE)
}

fn generic_dependency(include_implementation: bool) -> DependencyInterfaceInput {
    generic_dependency_with_templates(include_implementation, false)
}

fn generic_async_dependency() -> DependencyInterfaceInput {
    generic_dependency_from_fixture(true, false, ASYNC_GENERIC_DEPENDENCY)
}

fn trait_default_dependency() -> DependencyInterfaceInput {
    generic_dependency_from_fixture(true, false, TRAIT_DEFAULT_DEPENDENCY)
}

fn generic_dependency_with_templates(
    include_implementation: bool,
    malformed_templates: bool,
) -> DependencyInterfaceInput {
    generic_dependency_from_fixture(
        include_implementation,
        malformed_templates,
        GENERIC_DEPENDENCY,
    )
}

fn first_imported_function_address(
    compilation: &crate::Compilation,
) -> bray_symbols::ImportedSemanticAddress {
    let skeleton = compilation
        .imported_symbol_skeleton_result()
        .unwrap_or_else(|error| panic!("imported skeleton must load: {error:?}"));

    let skeleton = skeleton
        .value()
        .as_deref()
        .unwrap_or_else(|| panic!("valid dependency must publish a symbol skeleton"));

    skeleton
        .functions()
        .iter()
        .filter_map(|function| skeleton.imported_semantic_address(function.id().into()))
        .next()
        .unwrap_or_else(|| panic!("imported generic function must have a template address"))
}

fn imported_nested_template_address(
    compilation: &crate::Compilation,
) -> crate::fact::ImportedExecutableTemplateAddress {
    let skeleton = compilation
        .imported_symbol_skeleton_result()
        .unwrap_or_else(|error| panic!("imported skeleton must load: {error:?}"));

    let skeleton = skeleton
        .value()
        .as_deref()
        .unwrap_or_else(|| panic!("valid dependency must publish a symbol skeleton"));

    skeleton
        .functions()
        .iter()
        .filter_map(|function| skeleton.imported_semantic_address(function.id().into()))
        .find_map(|symbol| {
            let root = compilation
                .imported_executable_template_with_cancellation(
                    crate::fact::ImportedExecutableTemplateAddress::root(symbol),
                    &compilation.state.cancellation,
                )
                .ok()?;

            root.value()
                .as_ref()?
                .operations()
                .iter()
                .find_map(|operation| {
                    let MirOperationKind::AnonymousCallable(
                        bray_ir::MirAnonymousCallableReference::Imported(key),
                    ) = operation.kind()
                    else {
                        return None;
                    };

                    Some(crate::fact::ImportedExecutableTemplateAddress::new(
                        symbol,
                        key.template(),
                    ))
                })
        })
        .unwrap_or_else(|| panic!("imported generic callable must reference a nested template"))
}
