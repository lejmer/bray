use super::support::artifacts::{
    assert_source_emits_valid_native_units, generated_artifacts, generated_artifacts_of_kind,
};
use super::support::compilation::{codegen_compilation_for_product, test_product_identity};
use super::support::mappings::assert_native_callback_entry;
use super::support::runtime::{
    runtime_native_plan_for_sources_target,
    runtime_native_plan_for_sources_target_with_platform_overrides,
    runtime_native_plan_for_sources_target_with_platform_services,
};
use super::support::specialization::concrete_generic_specializations;
use crate::{
    CancellationToken, CompilationOptions, CompilationRequest, SelectedTarget, WorkerBudget,
};
use bray_bound_tree::BoundCallResult;
use bray_codegen::{BackendArtifactKind, CodegenLinkage, CodegenSpecialization};
use bray_compiler_known::RepresentationRole;
use bray_ir::{MirCallTarget, MirOperationKind, MirTerminatorKind};
use bray_runtime_interface::{
    PlatformServiceBinding, PlatformServiceRole, RuntimeAbiRole, RuntimeAbiVersion,
};
use bray_symbols::{NamedTypeSymbolId, ProductKind};
use bray_target::{NativeTarget, TargetAddressSpaces, TargetProfile, TargetProperties};

const TARGET_FENCE_SOURCE: &str = r#"module app;

using core.target;

@copy
public union FenceChoice
{
    Acquire;
    Release;
    AcquireRelease;
    SequentiallyConsistent;
}

public trusted func fence(order: FenceChoice) uses(intrinsic)
{
    match order
    {
        case FenceChoice.Acquire
        {
            trusted core.target.hardware_fence(MemoryOrder.Acquire);
        }
        case FenceChoice.Release
        {
            trusted core.target.hardware_fence(MemoryOrder.Release);
        }
        case FenceChoice.AcquireRelease
        {
            trusted core.target.hardware_fence(MemoryOrder.AcquireRelease);
        }
        case FenceChoice.SequentiallyConsistent
        {
            trusted core.target.hardware_fence(
                MemoryOrder.SequentiallyConsistent,
            );
        }
    }
}
"#;

const ATOMIC_GENERIC_SOURCE: &str = r#"module app;

using core.atomic;

func entry()
{
    main();
}

func initialize<T>(pos value: T) -> core.atomic.Atomic<T>
{
    return core.atomic.initialize<T>(value);
}

func main()
{
    let storage = initialize<u32>(1);
}
"#;

const ATOMIC_LIBRARY_SOURCE: &str = r#"module app;

using core.atomic;

public func initialize<T>(pos value: T) -> core.atomic.Atomic<T>
{
    return core.atomic.initialize<T>(value);
}
"#;

const ATOMIC_FENCE_SOURCE: &str = r#"module app;

using core.atomic;

@copy
public union FenceOrder
{
    Acquire;
    Release;
    AcquireRelease;
    SequentiallyConsistent;
}

public func fence(order: FenceOrder)
{
    match order
    {
        case FenceOrder.Acquire { core.atomic.fence<1>(); }
        case FenceOrder.Release { core.atomic.fence<2>(); }
        case FenceOrder.AcquireRelease { core.atomic.fence<3>(); }
        case FenceOrder.SequentiallyConsistent { core.atomic.fence<4>(); }
    }
}
"#;

const STRUCTURAL_ASSEMBLY_SOURCE: &str = r#"module app;

using core.target;

public trusted func assemble(pos value: i32) -> i32
    uses(device_memory, intrinsic, raw_memory, unchecked_alias, unchecked_init)
{
    let outputs: (i32, i32) = trusted core.target.assembly<
        (i32, i32, i32),
        (i32, i32),
    >(
        template = "",
        constraints = "+reg,=reg,reg,i",
        clobbers = "",
        features = "",
        options = 1,
        inputs = (value, if value == 0 { yield 1; } else { yield value; }, 7),
    );

    return outputs.0;
}

func alternate() -> never
{
    loop {}
}

func generic_alternate<T>(pos value: T) -> never
{
    loop {}
}

public trusted func assemble_addresses(pos pointer: DataPointer<u8>)
    uses(device_memory, intrinsic, raw_memory, unchecked_alias, unchecked_init)
{
    let ignored: (i32,) = trusted core.target.assembly<
        (func() -> never, func(pos value: i32) -> never, DataPointer<u8>),
        (i32,),
    >(
        template = "",
        constraints = "=reg,s,s,m",
        clobbers = "",
        features = "",
        options = 1,
        inputs = (alternate, generic_alternate<i32>, pointer),
    );
}

public trusted func branch(pos value: i32) -> i32
    uses(device_memory, intrinsic, raw_memory, unchecked_alias, unchecked_init)
{
    let output: (i32,) = trusted core.target.branching_assembly<
        (i32,),
        (i32,),
        (func() -> never,),
    >(
        template = "",
        constraints = "+reg,label",
        clobbers = "",
        features = "",
        options = 1,
        inputs = (value,),
        labels = (alternate,),
    );

    return output.0;
}
"#;

const MEMORY_ASSEMBLY_SOURCE: &str = r#"trusted module memory_assembly;

using core.target;

public trusted func assemble_memory(pos pointer: DataPointer<u8>)
    uses(device_memory, intrinsic, raw_memory, unchecked_alias, unchecked_init)
{
    trusted core.target.assembly<(DataPointer<u8>,), unit>(
        template = "incb $0",
        constraints = "m",
        clobbers = "memory",
        features = "",
        options = 0,
        inputs = (pointer,),
    );
}
"#;

const VOID_BRANCHING_ASSEMBLY_SOURCE: &str = r#"trusted module void_branching_assembly;

using core.target;

func alternate() -> never
{
    loop {}
}

public trusted func branch_void(pos pointer: DataPointer<u8>)
    uses(device_memory, intrinsic, raw_memory, unchecked_alias, unchecked_init)
{
    trusted core.target.branching_assembly<
        (DataPointer<u8>,),
        unit,
        (func() -> never,),
    >(
        template = "",
        constraints = "m,label",
        clobbers = "",
        features = "",
        options = 0,
        inputs = (pointer,),
        labels = (alternate,),
    );
}
"#;

const DIVERGING_ASSEMBLY_SOURCE: &str = r#"trusted module diverging_assembly;

using core.target;

public trusted func diverge() -> never
    uses(device_memory, intrinsic, raw_memory, unchecked_alias, unchecked_init)
{
    trusted core.target.diverging_assembly<(i32,)>(
        template = "ud2",
        constraints = "reg",
        clobbers = "",
        features = "",
        options = 0,
        inputs = (0,),
    );
}
"#;

const DEVICE_VOLATILE_CONTRACT_SOURCE: &str = r#"trusted module device_contract;

using core.target;

trusted func device_roundtrip(pos pointer: DevicePointer<u8>, pos value: u8) -> u8
    requires(
        trusted core.target.device_valid_read<u8>(pointer = pointer, count = 1),
        trusted core.target.device_valid_write<u8>(pointer = pointer, count = 1),
        trusted core.target.device_aligned_for<u8>(pointer = pointer),
    )
    uses(device_memory, intrinsic, raw_memory, unchecked_alias, unchecked_init)
{
    trusted core.target.device_volatile_store<u8>(pointer, value);
    let result = trusted core.target.device_volatile_load<u8>(pointer);
    trusted core.target.assembly<(DevicePointer<u8>,), unit>(
        template = "",
        constraints = "m",
        clobbers = "memory",
        features = "",
        options = 0,
        inputs = (pointer,),
    );
    return result;
}
"#;

#[test]
fn target_fence_wrapper_emits_valid_native_units() {
    let (backend, compilation) =
        codegen_compilation_for_product(TARGET_FENCE_SOURCE, ProductKind::Library);

    let plan = compilation
        .native_product_plan(
            test_product_identity(),
            crate::BuildConfiguration::Development,
            None,
            [],
            None,
        )
        .unwrap_or_else(|error| panic!("target fence wrapper must realize: {error:?}"));

    assert!(
        generated_artifacts(&backend, &plan)
            .iter()
            .all(|artifact| !artifact.is_empty())
    );
}

#[test]
fn structural_assembly_emits_valid_native_units() {
    let (backend, compilation) =
        codegen_compilation_for_product(STRUCTURAL_ASSEMBLY_SOURCE, ProductKind::Library);

    let lowered = compilation
        .lowered_unit(crate::test_support::source_function_body_key(
            &compilation,
            "branch",
        ))
        .unwrap_or_else(|error| panic!("branching assembly must lower: {error:?}"));

    let mir = lowered
        .value()
        .as_ref()
        .and_then(bray_lowering::LoweredUnit::mir)
        .unwrap_or_else(|| panic!("branching assembly must produce MIR: {lowered:#?}"));

    let assembly = mir
        .blocks()
        .iter()
        .find_map(|block| match block.terminator().kind() {
            MirTerminatorKind::InlineAssembly(assembly) => Some(assembly),
            _ => None,
        })
        .unwrap_or_else(|| panic!("branching assembly must lower as a terminator"));

    let [alternate] = assembly.alternates() else {
        panic!("one checked label must produce one alternate trampoline");
    };

    let alternate = mir
        .block(*alternate)
        .unwrap_or_else(|| panic!("alternate trampoline must exist"));

    let calls = alternate
        .operations()
        .iter()
        .filter_map(|id| mir.operation(*id))
        .filter_map(|operation| match operation.kind() {
            MirOperationKind::Call(call) => Some(call),
            _ => None,
        })
        .collect::<Vec<_>>();

    let [call] = calls.as_slice() else {
        panic!("alternate trampoline must contain one callback call");
    };

    let MirCallTarget::Indirect { .. } = call.target() else {
        panic!("assembly labels must remain runtime callable values");
    };

    let never = compilation
        .available_compiler_known_symbols()
        .representation_symbol::<bray_symbols::StructSymbolId>(RepresentationRole::Never)
        .and_then(|definition| {
            compilation.semantic_value_store().ok().and_then(|values| {
                crate::compilation::substitution::named_type(
                    values,
                    NamedTypeSymbolId::Struct(definition),
                )
                .ok()
            })
        })
        .unwrap_or_else(|| panic!("never representation must resolve"));

    assert!(call.arguments().is_empty());
    assert_eq!(call.result(), BoundCallResult::Immediate(never));

    let MirTerminatorKind::CheckCallOutcome { completed, .. } = alternate.terminator().kind()
    else {
        panic!("a nonreturning Bray callback must still forward panic and cancellation");
    };

    assert!(matches!(
        mir.block(completed.target()).expect("fixture MIR block must exist").terminator().kind(),
        MirTerminatorKind::Unreachable
    ));

    let plan = compilation
        .native_product_plan(
            test_product_identity(),
            crate::BuildConfiguration::Development,
            None,
            [],
            None,
        )
        .unwrap_or_else(|error| panic!("structural assembly must realize: {error:?}"));

    assert!(
        generated_artifacts(&backend, &plan)
            .iter()
            .all(|artifact| !artifact.is_empty())
    );
}

#[test]
fn indirect_memory_assembly_emits_valid_native_units() {
    let (backend, compilation) =
        codegen_compilation_for_product(MEMORY_ASSEMBLY_SOURCE, ProductKind::Library);

    let plan = compilation
        .native_product_plan(
            test_product_identity(),
            crate::BuildConfiguration::Development,
            None,
            [],
            None,
        )
        .unwrap_or_else(|error| panic!("memory assembly must realize: {error:?}"));

    let artifacts = generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendIr);

    assert!(artifacts.iter().any(|artifact| {
        std::str::from_utf8(artifact).is_ok_and(|artifact| artifact.contains("ptr elementtype(i8)"))
    }));
}

#[test]
fn void_branching_assembly_emits_valid_native_units() {
    assert_source_emits_valid_native_units(
        VOID_BRANCHING_ASSEMBLY_SOURCE,
        crate::BuildConfiguration::Development,
    );
}

#[test]
fn impure_diverging_assembly_survives_optimized_native_codegen() {
    let (backend, compilation) =
        codegen_compilation_for_product(DIVERGING_ASSEMBLY_SOURCE, ProductKind::Library);

    let plan = compilation
        .native_product_plan(
            test_product_identity(),
            crate::BuildConfiguration::Release,
            None,
            [],
            None,
        )
        .unwrap_or_else(|error| panic!("diverging assembly must realize: {error:?}"));

    let artifacts = generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendIr);

    assert!(artifacts.iter().any(|artifact| {
        std::str::from_utf8(artifact)
            .is_ok_and(|artifact| artifact.contains("asm sideeffect \"ud2\""))
    }));
}

#[test]
fn device_volatile_contracts_accept_device_pointer_storage_contracts() {
    let native = NativeTarget::X86_64LinuxGnu.profile();
    let baseline = native.properties();

    let address_spaces = TargetAddressSpaces::try_new(true, true)
        .unwrap_or_else(|| panic!("test target must expose host and device address spaces"));

    let properties = TargetProperties::new(
        baseline.identity().clone(),
        baseline.scalars(),
        baseline.atomics(),
        baseline.abis(),
        baseline.c_abi(),
        address_spaces,
        baseline.alignments(),
        baseline.operations(),
    );

    let profile = TargetProfile::try_new(
        native.identity().clone(),
        native.machine().clone(),
        properties,
    )
    .unwrap_or_else(|error| panic!("device-capable target profile must validate: {error:?}"));

    let request = CompilationRequest::with_options(
        crate::test_support::package_identity(),
        vec![crate::test_support::source_input(
            DEVICE_VOLATILE_CONTRACT_SOURCE,
            0,
        )],
        CompilationOptions::new(
            WorkerBudget::serial(),
            ProductKind::Library,
            SelectedTarget::new(profile, RuntimeAbiVersion::new(1, 0)),
        ),
    );

    let compilation = crate::Compilation::load(request)
        .unwrap_or_else(|error| panic!("device contract compilation must load: {error:?}"));

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn supported_atomic_generic_specializations_reach_codegen() {
    let compilation = crate::test_support::compilation(ATOMIC_GENERIC_SOURCE);
    let specializations = concrete_generic_specializations(&compilation);

    assert!(specializations.iter().any(|specialization| {
        matches!(specialization, CodegenSpecialization::Generic(arguments) if !arguments.is_empty())
    }));
}

#[test]
fn library_atomic_roots_exclude_open_generic_wrappers() {
    let compilation =
        crate::test_support::compilation_with_product(ATOMIC_LIBRARY_SOURCE, ProductKind::Library);

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
        .unwrap_or_else(|error| panic!("test codegen target must validate: {error:?}"));

    let semantic = compilation
        .product_semantics()
        .unwrap_or_else(|error| panic!("test product plan must resolve: {error:?}"));

    let roots = compilation
        .product_root_instances(semantic.value(), None, &target, &cancellation)
        .unwrap_or_else(|error| panic!("test roots must resolve: {error:?}"));

    assert!(roots.is_empty());
}

#[test]
fn atomic_fence_wrapper_emits_valid_native_units() {
    let (backend, compilation) =
        codegen_compilation_for_product(ATOMIC_FENCE_SOURCE, ProductKind::Library);

    let plan = compilation
        .native_product_plan(
            test_product_identity(),
            crate::BuildConfiguration::Development,
            None,
            [],
            None,
        )
        .unwrap_or_else(|error| panic!("atomic fence plan must resolve: {error:?}"));

    assert!(
        generated_artifacts(&backend, &plan)
            .iter()
            .all(|artifact| !artifact.is_empty())
    );
}

#[test]
fn direct_platform_bindings_and_callback_entries_cover_every_native_target() {
    let callback_source = concat!(
        "module app;\n",
        "@symbol(name = \"native_callback\")\n",
        "@abi(c)\n",
        "func callback(pos value: i32) -> i32\n",
        "{\n",
        "    return value + 1;\n",
        "}\n",
        "func main()\n",
        "{\n",
        "    let result: i32 = callback(1);\n",
        "}\n",
    );

    let platform_source = concat!(
        "trusted module app;\n",
        "@layout(c)\n",
        "internal struct PlatformStatus\n",
        "{\n",
        "    category: u32;\n",
        "    reserved: u32;\n",
        "    native_code: i64;\n",
        "}\n",
        "@abi(c)\n",
        "trusted internal func flush() -> PlatformStatus\n",
        "{\n",
        "    return { category = 0, reserved = 0, native_code = 0 };\n",
        "}\n",
        "func main()\n",
        "{\n",
        "    let status: PlatformStatus = trusted flush();\n",
        "}\n",
    );

    let binding =
        PlatformServiceBinding::try_new(PlatformServiceRole::StandardOutputFlush, "app.flush")
            .unwrap_or_else(|| panic!("platform service binding must validate"));

    let platform_symbol = PlatformServiceRole::StandardOutputFlush.native_symbol();

    for target in NativeTarget::ALL {
        let selected = SelectedTarget::for_native(target);

        let (backend, callback_plan) = runtime_native_plan_for_sources_target(
            &[callback_source],
            ProductKind::Executable,
            selected.clone(),
            &[],
        );

        let callback_body =
            assert_native_callback_entry(&callback_plan, "native_callback", CodegenLinkage::Export);

        let backend_ir =
            generated_artifacts_of_kind(&backend, &callback_plan, BackendArtifactKind::BackendIr)
                .into_iter()
                .map(|artifact| String::from_utf8_lossy(&artifact).into_owned())
                .collect::<String>();

        assert!(
            backend_ir
                .lines()
                .any(|line| line.contains(" call ") && line.contains(&callback_body)),
            "{target:?}"
        );

        let (_, platform_plan) = runtime_native_plan_for_sources_target_with_platform_services(
            &[platform_source],
            ProductKind::Executable,
            selected,
            &[],
            [binding.clone()],
        );

        assert_direct_platform_service(&platform_plan, platform_symbol);
    }
}

#[test]
fn native_platform_cleanup_terminates_uncaught_panic() {
    assert_native_platform_cleanup_emits(
        "",
        r#"
            destruct()
            {
                panic("native provider cleanup");
            }
        "#,
        bray_runtime_interface::RuntimeAbiRole::PanicPropagation,
    );
}

#[test]
fn native_platform_cleanup_terminates_uncaught_cancellation() {
    assert_native_platform_cleanup_emits(
        "",
        r#"
            finalize()
            {
                let outcome: RunResult<unit> = Cancelled;
                try outcome;
            }
        "#,
        bray_runtime_interface::RuntimeAbiRole::PanicPropagation,
    );
}

#[test]
fn native_platform_normal_return_consumes_retained_cleanup_incidents() {
    assert_native_platform_cleanup_emits(
        r#"
            struct Incident {}
            struct Scope {}
            impl Scope
            {
                enter() -> bool
                {
                    return true;
                }

                exit(pos capability: bool) -> Result<unit, Incident>
                {
                    return Error(Incident {});
                }
            }
        "#,
        r#"
            destruct()
            {
                let scope = Scope {};

                _ = with capability = scope
                {
                    return;
                };
            }
        "#,
        bray_runtime_interface::RuntimeAbiRole::PanicReporting,
    );
}

fn assert_native_platform_cleanup_emits(
    definitions: &str,
    cleanup: &str,
    report: bray_runtime_interface::RuntimeAbiRole,
) {
    let source = format!(
        r#"
            trusted module app;

            @layout(c)
            internal struct PlatformStatus
            {{
                category: u32;
                reserved: u32;
                native_code: i64;
            }}

            {definitions}

            struct Owner
            {{
                {cleanup}
            }}

            @abi(c)
            trusted internal func flush() -> PlatformStatus
            {{
                let owner = Owner {{}};
                return {{ category = 0, reserved = 0, native_code = 0 }};
            }}
        "#,
    );

    let binding =
        PlatformServiceBinding::try_new(PlatformServiceRole::StandardOutputFlush, "app.flush")
            .expect("platform service binding must validate");

    let (backend, plan) = runtime_native_plan_for_sources_target_with_platform_services(
        &[&source],
        ProductKind::Library,
        SelectedTarget::baseline(),
        &[],
        [binding],
    );

    let artifacts = generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendIr);

    let implementation = artifacts
        .iter()
        .find_map(|artifact| {
            let ir = std::str::from_utf8(artifact).expect("LLVM IR is UTF-8");

            let start = ir.lines().position(|line| {
                line.starts_with("define ")
                    && line.contains("@bray_platform_standard_output_flush(")
            })?;

            Some(
                ir.lines()
                    .skip(start)
                    .take_while(|line| *line != "}")
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        })
        .expect("the selected platform service must have an emitted definition");

    assert!(
        implementation.lines().any(|line| {
            line.contains("call ")
                && line.contains(&format!(
                    "@{}(",
                    report
                        .native_symbol()
                        .expect("report role has its native symbol")
                ))
        }),
        "{implementation}"
    );

    assert!(!implementation.contains("@bray_runtime_current_run_cancellation_propagation("));

    if report == bray_runtime_interface::RuntimeAbiRole::PanicReporting {
        assert!(
            implementation
                .lines()
                .any(|line| line.trim_start().starts_with("ret ")),
            "{implementation}"
        );
    }
}

#[test]
fn bray_platform_service_implementations_are_native_fallbacks() {
    let source = concat!(
        "trusted module app;\n",
        "@layout(c)\n",
        "internal struct PlatformStatus\n",
        "{\n",
        "    category: u32;\n",
        "    reserved: u32;\n",
        "    native_code: i64;\n",
        "}\n",
        "@abi(c)\n",
        "trusted internal func flush() -> PlatformStatus\n",
        "{\n",
        "    return { category = 0, reserved = 0, native_code = 0 };\n",
        "}\n",
        "func main()\n",
        "{\n",
        "    let status: PlatformStatus = trusted flush();\n",
        "}\n",
        "@test\n",
        "func platform_service_test()\n",
        "{\n",
        "    let status: PlatformStatus = trusted flush();\n",
        "}\n",
    );

    let binding =
        PlatformServiceBinding::try_new(PlatformServiceRole::StandardOutputFlush, "app.flush")
            .unwrap_or_else(|| panic!("platform service binding must validate"));

    let (backend, plan) = runtime_native_plan_for_sources_target_with_platform_services(
        &[source],
        ProductKind::Executable,
        SelectedTarget::baseline(),
        &[],
        [binding.clone()],
    );

    let symbol = PlatformServiceRole::StandardOutputFlush.native_symbol();

    assert_direct_platform_service(&plan, symbol);

    let provided =
        crate::compilation::product::codegen::link::product_native_definitions(plan.mappings());

    assert!(
        provided.get(symbol) == Some(&bray_symbols::NativeSymbolBinding::Weak),
        "a weak platform fallback must not suppress a strong provider"
    );

    let runtime_reference = plan
        .mappings()
        .iter()
        .flat_map(bray_codegen::CodegenMappings::symbols)
        .find(|mapping| matches!(mapping.key(), bray_codegen::CodegenSymbolKey::Runtime(_)))
        .expect("host plan must reference a runtime role");

    assert!(
        !provided.contains_key(runtime_reference.name().as_str()),
        "a runtime reference must not count as a product definition"
    );

    assert!(
        plan.preservation_roots()
            .any(|root| root.as_str() == symbol)
    );

    assert!(
        generated_artifacts(&backend, &plan)
            .iter()
            .all(|artifact| !artifact.is_empty())
    );

    let (backend, plan) = runtime_native_plan_for_sources_target_with_platform_overrides(
        &[source],
        ProductKind::Test,
        SelectedTarget::baseline(),
        &[],
        [binding],
        [PlatformServiceRole::StandardOutputFlush],
        crate::BuildConfiguration::Development,
    );

    let backend_ir = generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendIr)
        .into_iter()
        .map(|artifact| String::from_utf8_lossy(&artifact).into_owned())
        .collect::<String>();

    assert!(plan.mappings().iter().any(|mappings| {
        mappings.symbols().iter().any(|mapping| {
            mapping.name().as_str() == symbol && mapping.linkage() == CodegenLinkage::Import
        })
    }));

    assert!(
        backend_ir
            .lines()
            .any(|line| line.starts_with("declare ") && line.contains(symbol))
    );

    assert!(
        !backend_ir
            .lines()
            .any(|line| line.starts_with("define ") && line.contains(symbol))
    );
}

fn assert_direct_platform_service(
    plan: &crate::compilation::product::codegen::NativeProductPlan,
    symbol: &str,
) {
    assert!(plan.mappings().iter().any(|mappings| {
        mappings.symbols().iter().any(|mapping| {
            mapping.name().as_str() == symbol
                && mapping.linkage() == CodegenLinkage::Fallback
                && mapping.native_entry().is_none()
        })
    }));

    let host = plan
        .executable_host()
        .unwrap_or_else(|| panic!("platform plan must retain an executable host"));

    assert!(
        !host
            .requirements()
            .requires_role(RuntimeAbiRole::ForeignCallbackExecution)
    );
}

#[test]
fn platform_source_bindings_export_and_retain_the_canonical_role_symbol() {
    let source = concat!(
        "trusted module app;\n",
        "@layout(c)\n",
        "internal struct PlatformStatus\n",
        "{\n",
        "    category: u32;\n",
        "    reserved: u32;\n",
        "    native_code: i64;\n",
        "}\n",
        "@abi(c)\n",
        "trusted internal func flush() -> PlatformStatus\n",
        "{\n",
        "    return { category = 0, reserved = 0, native_code = 0 };\n",
        "}\n",
    );

    let role = PlatformServiceRole::StandardOutputFlush;

    let binding = PlatformServiceBinding::try_new(role, "app.flush")
        .unwrap_or_else(|| panic!("platform source binding must validate"));

    let (backend, plan) = runtime_native_plan_for_sources_target_with_platform_services(
        &[source],
        ProductKind::Library,
        SelectedTarget::baseline(),
        &[],
        [binding],
    );

    let symbol = role.native_symbol();

    assert!(plan.mappings().iter().any(|mappings| {
        mappings.symbols().iter().any(|mapping| {
            mapping.name().as_str() == symbol && mapping.linkage() == CodegenLinkage::Fallback
        })
    }));

    assert!(
        plan.preservation_roots()
            .any(|root| root.as_str() == symbol)
    );

    assert!(
        generated_artifacts(&backend, &plan)
            .iter()
            .all(|artifact| !artifact.is_empty())
    );
}
