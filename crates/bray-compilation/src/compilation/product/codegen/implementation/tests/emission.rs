use super::support::artifacts::{
    assert_source_emits_valid_native_units, generated_artifacts, generated_artifacts_of_kind,
    generated_artifacts_of_kind_with_options,
};
use super::support::compilation::{codegen_compilation_for_product, test_product_identity};
use super::support::dependencies::{GenericDependencyFixture, generic_dependency_from_fixture};
use super::support::linker::test_linker;
use super::support::runtime::{runtime_artifact, runtime_native_plan};
use crate::{CompilationOptions, CompilationRequest, SelectedTarget, WorkerBudget};
use bray_codegen::{
    BackendArtifactKind, CodeGenerator, CodeGeneratorRegistry, CodegenConfiguration,
    DebugInformationMode, OptimizationLevel,
};
use bray_ir::{MirOperand, MirOperationKind, MirProjectionKind};
use bray_runtime_interface::{ExecutableEntryResult, RootExecution};
use bray_symbols::{ConstantValueKind, ProductKind};
use bray_testing::TemporaryFile;
use std::sync::Arc;

const DIRECT_CALLABLE_TUPLE_INFERENCE_SOURCE: &str = concat!(
    "module app;\n",
    "\n",
    "func alternate() -> never\n",
    "{\n",
    "    loop {}\n",
    "}\n",
    "\n",
    "func identity<T>(pos value: T) -> T\n",
    "{\n",
    "    return value;\n",
    "}\n",
    "\n",
    "public func exercise() -> func() -> never\n",
    "{\n",
    "    return identity<(func() -> never,)>(value = (alternate,)).0;\n",
    "}\n",
);

#[test]
fn synchronous_i32_executable_hosts_emit_native_units() {
    let backend = Arc::new(
        bray_codegen_llvm::LlvmCodeGenerator::try_new()
            .unwrap_or_else(|error| panic!("LLVM backend must initialize: {error:?}")),
    );

    let registry = CodeGeneratorRegistry::try_new([Arc::clone(&backend) as Arc<dyn CodeGenerator>])
        .unwrap_or_else(|error| panic!("LLVM backend must register: {error:?}"));

    let codegen = CodegenConfiguration::try_new(registry, backend.identity().clone())
        .unwrap_or_else(|error| panic!("LLVM backend must select: {error:?}"));

    let request = CompilationRequest::with_options(
        crate::test_support::package_identity(),
        vec![crate::test_support::source_input(
            concat!(
                "module app;\n",
                "\n",
                "func main() -> i32\n",
                "{\n",
                "    return 42;\n",
                "}\n",
            ),
            0,
        )],
        CompilationOptions::new(
            WorkerBudget::serial(),
            ProductKind::Executable,
            SelectedTarget::baseline(),
        ),
    );

    let compilation = crate::Compilation::load_with_codegen(request, codegen)
        .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"));

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let product = test_product_identity();
    let archive = TemporaryFile::write("libbray_runtime.a", b"!<arch>\n");
    let available_runtime = runtime_artifact(&compilation, archive.path());

    let plan = compilation
        .native_product_plan(
            product.clone(),
            crate::BuildConfiguration::Development,
            Some(available_runtime.clone()),
            [],
            Some((&test_linker(), bray_linker::LinkedProductKind::Executable)),
        )
        .unwrap_or_else(|error| panic!("native plan must resolve: {error:?}"));

    let release = compilation
        .native_product_plan(
            product.clone(),
            crate::BuildConfiguration::Release,
            Some(available_runtime.clone()),
            [],
            Some((&test_linker(), bray_linker::LinkedProductKind::Executable)),
        )
        .unwrap_or_else(|error| panic!("release native plan must resolve: {error:?}"));

    let host = plan
        .executable_host()
        .unwrap_or_else(|| panic!("executable must retain its host"));

    assert!(host.requirements().requires_implementation());
    assert!(host.runtime_artifact().is_some());

    compilation
        .native_product_plan(
            product,
            crate::BuildConfiguration::Development,
            Some(available_runtime),
            [],
            Some((&test_linker(), bray_linker::LinkedProductKind::Executable)),
        )
        .unwrap_or_else(|error| panic!("available runtime must remain reusable: {error:?}"));

    assert_eq!(plan.options().optimization(), OptimizationLevel::Basic);

    assert_eq!(
        plan.options().debug_information(),
        DebugInformationMode::LineTables
    );

    assert_eq!(release.options().optimization(), OptimizationLevel::Full);

    assert_eq!(
        release.options().debug_information(),
        DebugInformationMode::None
    );

    assert!(!Arc::ptr_eq(&plan, &release));

    assert!(plan.mappings().iter().any(|mappings| {
        mappings
            .debug_locations()
            .iter()
            .any(|location| location.file().path() == "source-0" && location.line().get() > 1)
    }));

    assert!(
        release
            .mappings()
            .iter()
            .all(|mappings| mappings.debug_locations().is_empty())
    );

    let host = plan
        .executable_host()
        .unwrap_or_else(|| panic!("executable must own a host"));

    assert_eq!(host.entries()[0].root(), RootExecution::Synchronous);
    assert_eq!(host.entries()[0].result(), ExecutableEntryResult::I32);

    assert!(
        generated_artifacts(&backend, &plan)
            .iter()
            .all(|artifact| !artifact.is_empty())
    );
}

#[test]
fn borrowed_literal_identity_remains_reachable_for_every_representation() {
    let (_, compilation) = codegen_compilation_for_product(
        concat!(
            "module app;\n",
            "\n",
            "public func owned() -> string\n",
            "{\n",
            "    return \"shared literal\";\n",
            "}\n",
            "\n",
            "public func borrowed() -> &string\n",
            "{\n",
            "    return &\"shared literal\";\n",
            "}\n",
        ),
        ProductKind::Library,
    );

    let plan = compilation
        .native_product_plan(
            test_product_identity(),
            crate::BuildConfiguration::Development,
            None,
            [],
            None,
        )
        .unwrap_or_else(|error| panic!("borrowed literal must remain reachable: {error:?}"));

    let mappings = plan
        .mappings()
        .iter()
        .flat_map(bray_codegen::CodegenMappings::constants)
        .filter(|mapping| {
            matches!(
                mapping.data().kind(),
                ConstantValueKind::String(text) if text.as_ref() == "shared literal"
            )
        })
        .collect::<Vec<_>>();

    let borrowed = mappings
        .iter()
        .copied()
        .find(|mapping| mapping.semantic_type() != mapping.representation())
        .unwrap_or_else(|| panic!("borrow representation must be materialized"));

    assert!(mappings.iter().any(|mapping| {
        mapping.value() == borrowed.value() && mapping.semantic_type() == mapping.representation()
    }));
}

#[test]
fn inlined_callee_parameter_remains_valid_across_blocks() {
    assert_source_emits_valid_native_units(
        concat!(
            "module app;\n",
            "public func choose(pos value: i32) -> bool\n",
            "{\n",
            "    if value == 1 { return true; }\n",
            "    return false;\n",
            "}\n",
            "public func entry(pos value: i32) -> bool\n",
            "{\n",
            "    return choose(value);\n",
            "}\n",
        ),
        crate::BuildConfiguration::Release,
    );
}

#[test]
fn borrowed_union_patterns_emit_valid_native_units() {
    assert_source_emits_valid_native_units(
        concat!(
            "module app;\n",
            "public union Choice { First; Second; }\n",
            "public func select(pos value: &Choice) -> bool\n",
            "{\n",
            "    match value\n",
            "    {\n",
            "        case .First { return true; }\n",
            "        case .Second { return false; }\n",
            "    }\n",
            "}\n",
        ),
        crate::BuildConfiguration::Development,
    );
}

#[test]
fn indexed_static_accesses_emit_valid_native_units() {
    assert_source_emits_valid_native_units(
        concat!(
            "module app;\n",
            "internal static VALUES: [core.atomic.Atomic<usize>; 1] = [\n",
            "    core.atomic.initialize<usize>(7)\n",
            "];\n",
            "public trusted func read() -> usize\n",
            "{\n",
            "    return core.atomic.load<usize, 0>(&VALUES[0]);\n",
            "}\n",
        ),
        crate::BuildConfiguration::Development,
    );
}

#[test]
fn aggregate_static_borrows_emit_valid_native_units_in_either_declaration_order() {
    let types = r#"
            module app;
            struct Counter { value: core.atomic.Atomic<usize>; }
            struct Guard { count: usize; counter: &Counter; }
        "#;

    let counter = "static COUNTER: Counter = Counter { value = core.atomic.initialize<usize>(7) };";

    let guards = r#"
            static FIRST: Guard = Guard { count = 1, counter = &COUNTER };
            static SECOND: Guard = Guard { count = 2, counter = &COUNTER };
        "#;

    let root = r#"
            public func read() -> usize
            {
                return FIRST.count + SECOND.count + core.atomic.load<usize, 0>(&COUNTER.value);
            }
        "#;

    for declarations in [
        format!("{guards}\n{counter}"),
        format!("{counter}\n{guards}"),
    ] {
        let source = format!("{types}\n{declarations}\n{root}");

        assert_source_emits_valid_native_units(&source, crate::BuildConfiguration::Development);
    }
}

#[test]
fn imported_aggregate_static_borrows_emit_valid_native_units() {
    let dependency = generic_dependency_from_fixture(
        true,
        false,
        GenericDependencyFixture {
            source: r#"
                    module templates;
                    public struct Counter { public value: core.atomic.Atomic<usize>; }
                    public static COUNTER: Counter = Counter
                    {
                        value = core.atomic.initialize<usize>(7)
                    };
                "#,
            runtime_frames: None,
            executable_templates: 1,
            platform_service: None,
        },
    );

    let source = r#"
            module app;
            using example.dependency.templates;
            struct Guard
            {
                count: usize;
                counter: &example.dependency.templates.Counter;
            }
            static FIRST: Guard = Guard
            {
                count = 1,
                counter = &example.dependency.templates.COUNTER
            };
            static SECOND: Guard = Guard
            {
                count = 2,
                counter = &example.dependency.templates.COUNTER
            };
            public func read() -> usize
            {
                return FIRST.count + SECOND.count;
            }
        "#;

    let backend = Arc::new(bray_codegen_llvm::LlvmCodeGenerator::try_new().unwrap());

    let registry =
        CodeGeneratorRegistry::try_new([Arc::clone(&backend) as Arc<dyn CodeGenerator>]).unwrap();

    let codegen = CodegenConfiguration::try_new(registry, backend.identity().clone()).unwrap();
    let target = SelectedTarget::baseline();

    let request = CompilationRequest::with_options(
        crate::test_support::package_identity(),
        vec![crate::test_support::source_input(source, 0)],
        CompilationOptions::new(WorkerBudget::serial(), ProductKind::Library, target.clone()),
    )
    .with_dependency_interfaces([
        dependency,
        crate::test_support::runtime_standard_library_dependency(&target),
    ]);

    let compilation = crate::Compilation::load_with_codegen(request, codegen).unwrap();

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let plan = compilation
        .native_product_plan(
            test_product_identity(),
            crate::BuildConfiguration::Development,
            None,
            [],
            None,
        )
        .unwrap();

    assert!(
        generated_artifacts(&backend, &plan)
            .iter()
            .all(|artifact| !artifact.is_empty())
    );
}

#[test]
fn aggregate_static_relocation_preserves_native_export_names() {
    let source = r#"
            module app;
            @layout(c)
            struct Counter { value: usize; }
            struct Guard { count: usize; counter: &Counter; }
            static FIRST: Guard = Guard { count = 1, counter = &COUNTER };
            @symbol(name = "counter")
            static COUNTER: Counter = Counter { value = 7 };
            @symbol(name = "counter.unresolved")
            static OTHER: usize = 9;
            public func read() -> usize
            {
                return FIRST.count + OTHER;
            }
        "#;

    let (backend, compilation) = codegen_compilation_for_product(source, ProductKind::Library);

    let plan = compilation
        .native_product_plan(
            test_product_identity(),
            crate::BuildConfiguration::Development,
            None,
            [],
            None,
        )
        .unwrap();

    let ir = generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendIr);

    assert!(
        ir.iter().any(|artifact| {
            String::from_utf8_lossy(artifact).contains("@counter.unresolved =")
        }),
        "the explicit native export must retain its exact symbol"
    );
}

#[test]
fn never_calls_in_typed_return_paths_emit_valid_native_units() {
    assert_source_emits_valid_native_units(
        concat!(
            "trusted module app;\n",
            "trusted func terminate() -> never uses(intrinsic)\n",
            "{\n",
            "    trusted core.target.abort();\n",
            "}\n",
            "public trusted func select(pos terminate_now: bool) -> u64\n",
            "{\n",
            "    if terminate_now\n",
            "    {\n",
            "        return trusted terminate();\n",
            "    }\n",
            "\n",
            "    return 7;\n",
            "}\n",
        ),
        crate::BuildConfiguration::Development,
    );
}

#[test]
fn imported_execution_guarantees_emit_native_units() {
    let dependency = generic_dependency_from_fixture(
        true,
        false,
        GenericDependencyFixture {
            source: "module templates; func helper<T>() -> bool executes(pure, total) when(true) { ensures(result) } { return true; } public func certified<T>() -> bool executes(pure, total) when(true) { ensures(result) } { return helper<T>(); }",
            runtime_frames: None,
            executable_templates: 2,
            platform_service: None,
        },
    );

    let backend = Arc::new(bray_codegen_llvm::LlvmCodeGenerator::try_new().unwrap());

    let registry =
        CodeGeneratorRegistry::try_new([Arc::clone(&backend) as Arc<dyn CodeGenerator>]).unwrap();

    let codegen = CodegenConfiguration::try_new(registry, backend.identity().clone()).unwrap();
    let target = SelectedTarget::baseline();

    let request = CompilationRequest::with_options(
            crate::test_support::package_identity(),
            vec![crate::test_support::source_input("module app; using example.dependency.templates.certified; public func root() -> bool executes(pure, total) when(true) { ensures(result) } { return example.dependency.templates.certified<bool>(); }", 0)],
            CompilationOptions::new(WorkerBudget::serial(), ProductKind::Library, target.clone()),
        ).with_dependency_interfaces([dependency, crate::test_support::runtime_standard_library_dependency(&target)]);

    let compilation = crate::Compilation::load_with_codegen(request, codegen).unwrap();

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:?}",
        compilation.check_diagnostics()
    );

    let plan = compilation
        .native_product_plan(
            test_product_identity(),
            crate::BuildConfiguration::Development,
            None,
            [],
            None,
        )
        .unwrap();

    let artifacts = generated_artifacts(&backend, &plan);

    assert!(!artifacts.is_empty());
    assert!(artifacts.iter().all(|artifact| !artifact.is_empty()));
}

#[test]
fn weakened_callable_execution_contract_emits_native_units() {
    assert_source_emits_valid_native_units(
        r#"
                module app;
                callable Strong = func() -> bool executes(pure, total);
                callable Plain = func() -> bool;

                func supplied() -> bool executes(pure, total)
                {
                    return true;
                }

                public func root() -> bool
                {
                    let provided: Strong = supplied;
                    let weakened = provided as Plain;
                    return weakened();
                }
            "#,
        crate::BuildConfiguration::Development,
    );
}

#[test]
fn explicit_generic_tuple_expectations_reach_direct_callable_elements() {
    let _ = codegen_compilation_for_product(
        DIRECT_CALLABLE_TUPLE_INFERENCE_SOURCE,
        ProductKind::Library,
    );
}

#[test]
fn concrete_literal_array_length_reaches_native_codegen() {
    let source = r#"
            module app;
            struct Guard
            {
                id: i32;

                destruct() {}
            }
            func make_guard(pos id: i32) -> Guard
            {
                return Guard { id = id };
            }
            func main()
            {
                let guards = box([
                    [make_guard(1), make_guard(2)],
                    [make_guard(3), make_guard(4)],
                ]);
            }
        "#;

    let (backend, plan) = runtime_native_plan(source);

    let artifacts = generated_artifacts(&backend, &plan);

    assert!(!artifacts.is_empty());

    assert!(artifacts.iter().all(|artifact| !artifact.is_empty()));
}

#[test]
fn compile_only_emission_preserves_specialized_cleanup_mir() {
    let source = r#"
            module app;
            struct Guard {}
            impl Guard
            {
                finalize() {}
            }
            func dispose<T>(pos value: T) -> i32
            {
                return 7;
            }
            func main() -> i32
            {
                return dispose<[Guard; 2]>([Guard {}, Guard {}]);
            }
        "#;

    let (_, compilation) = codegen_compilation_for_product(source, ProductKind::Executable);

    let archive = TemporaryFile::write("libbray_runtime.a", b"!<arch>\n");
    let runtime = runtime_artifact(&compilation, archive.path());
    let linker = test_linker();

    let plan = compilation
        .native_product_plan(
            test_product_identity(),
            crate::BuildConfiguration::Development,
            Some(runtime),
            [],
            Some((&linker, bray_linker::LinkedProductKind::Executable)),
        )
        .expect("generic cleanup native plan must prepare");

    let profile = compilation.selected_target().target().profile().clone();

    let name =
        bray_target::TargetOutputName::try_new(bray_target::TargetOutputKind::BackendIr, "", ".ll")
            .expect("backend IR output name must validate");

    let outputs = bray_target::TargetOutputDescription::try_new(profile.clone(), [name])
        .expect("backend IR outputs must validate");

    let destination = tempfile::tempdir().expect("output directory must exist");

    let request = bray_emitter::EmissionRequest::try_new(
        test_product_identity(),
        ProductKind::Executable,
        plan.executable_host().cloned(),
        profile.identity().clone(),
        bray_emitter::RequestedArtifactDestination::FilesystemDirectory(destination.path().into()),
        [bray_emitter::RequestedArtifact::new(
            bray_emitter::ArtifactKind::BackendIr,
            bray_emitter::ArtifactRequirement::Required,
        )],
        bray_emitter::ReplacementPolicy::RequireAbsent,
    )
    .expect("compile-only request must validate");

    let linking = plan.link().expect("native plan must retain link inputs");

    let composed = crate::ProductEmissionInputs::new(&outputs)
        .with_native_codegen(&plan)
        .with_linking(&linker, linking);

    let error = compilation
        .emit_product(request.clone(), composed)
        .expect_err("compile-only requests must reject explicitly supplied linking");

    assert!(matches!(
        error.kind(),
        crate::ProductEmissionErrorKind::UnexpectedLinker
    ));

    let outcome = compilation
        .emit_product(
            request,
            crate::ProductEmissionInputs::new(&outputs).with_native_codegen(&plan),
        )
        .expect("compile-only emission must consume specialized cleanup MIR");

    assert!(matches!(
        outcome.status(),
        bray_emitter::EmissionStatus::Complete
    ));

    assert_eq!(outcome.artifacts().artifacts().len(), plan.units().len());

    assert!(
        outcome
            .artifacts()
            .artifacts()
            .iter()
            .all(|artifact| artifact.id().kind() == bray_emitter::ArtifactKind::BackendIr)
    );
}

#[test]
fn tuple_destructuring_projects_each_initializer_field_once() {
    let (_, compilation) = codegen_compilation_for_product(
        concat!(
            "module app;\n",
            "public func sum(pos value: (u64, u64)) -> u64\n",
            "{\n",
            "    let(first, second) = value;\n",
            "    return first + second;\n",
            "}\n",
        ),
        ProductKind::Library,
    );

    let lowered = compilation
        .lowered_unit(crate::test_support::source_function_body_key(
            &compilation,
            "sum",
        ))
        .unwrap_or_else(|error| panic!("tuple destructuring must lower: {error:?}"));

    let mir = lowered
        .value()
        .as_ref()
        .and_then(bray_lowering::LoweredUnit::mir)
        .unwrap_or_else(|| panic!("tuple destructuring must produce MIR: {lowered:#?}"));

    let fields = mir
        .operations()
        .iter()
        .filter_map(|operation| match operation.kind() {
            MirOperationKind::Store {
                value: MirOperand::Move(place),
                ..
            } => match place.projections() {
                [projection] => match projection.kind() {
                    MirProjectionKind::TupleField(field) => Some(*field),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(fields, [0, 1]);
}

#[test]
fn returned_padded_tuple_destructuring_preserves_the_aggregate_payload() {
    let (backend, compilation) = codegen_compilation_for_product(
        r#"
module app;

@copy
@layout(c)
struct Records
{
    head: usize;
    tail: usize;
    count: usize;
}

func admit(pos records: Records) -> (u32, Records)
{
    return (7, records);
}

public func update(pos records: Records) -> Records
{
    let (status, updated) = admit(records);

    assert(status == 7);
    return updated;
}
"#,
        ProductKind::Library,
    );

    let plan = compilation
        .native_product_plan(
            test_product_identity(),
            crate::BuildConfiguration::Development,
            None,
            [],
            None,
        )
        .unwrap_or_else(|error| panic!("padded tuple source must realize: {error:?}"));

    let options = plan.options().with_optimization(OptimizationLevel::None);

    let artifacts = generated_artifacts_of_kind_with_options(
        &backend,
        &plan,
        BackendArtifactKind::BackendIr,
        &options,
    );

    let ir = artifacts
        .iter()
        .map(|artifact| String::from_utf8_lossy(artifact))
        .collect::<Vec<_>>()
        .join("\n");

    let tuple_type = ir
        .lines()
        .find(|line| line.contains("= type { i32, [4 x i8],"))
        .and_then(|line| line.split_whitespace().next())
        .expect("the returned tuple must contain padding before its records payload");

    assert!(
        ir.lines().any(|line| {
            line.contains(&format!("extractvalue {tuple_type} "))
                && line.split(", !dbg").next().unwrap().ends_with(", 2")
        }),
        "the records payload must be extracted after tuple padding: {ir}"
    );
}
