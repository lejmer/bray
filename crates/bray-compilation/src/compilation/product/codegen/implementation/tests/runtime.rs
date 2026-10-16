use super::support::artifacts::{generated_artifacts, generated_artifacts_of_kind};
use super::support::compilation::{
    codegen_compilation, codegen_compilation_for_sources_target_with_source_roles,
    test_product_identity,
};
use super::support::linker::{test_linked_product, test_linker};
use super::support::runtime::{
    runtime_artifact, runtime_artifact_with_roles, runtime_native_plan_for_product,
};
use crate::compilation::product::codegen::NativeProductPlanningError;
use crate::{SelectedTarget, WorkerBudget};
use bray_codegen::{BackendArtifactKind, CodegenLinkage};
use bray_ir::MirUnitKey;
use bray_runtime_interface::{
    ExecutableHostContractBuildError, RuntimeAbiRole, RuntimeCompatibilityError,
};
use bray_symbols::ProductKind;
use bray_testing::TemporaryFile;
use std::sync::Arc;

#[test]
fn demanded_runtime_roles_cannot_disappear_from_product_validation() {
    let (_, compilation) = codegen_compilation(include_str!(
        "../../../../../../../../xtask/fixtures/native-execution/sync-panic.bray"
    ));

    let archive = TemporaryFile::write("libbray_runtime.a", b"!<arch>\n");

    let roles = RuntimeAbiRole::ALL
        .into_iter()
        .filter(|role| *role != RuntimeAbiRole::PanicPropagation);

    let runtime = runtime_artifact_with_roles(&compilation, archive.path(), roles);

    let product = test_product_identity();

    let result = compilation.native_product_plan(
        product,
        crate::BuildConfiguration::Development,
        Some(runtime),
        [],
        Some((&test_linker(), bray_linker::LinkedProductKind::Executable)),
    );

    let Err(error) = result else {
        panic!("incomplete runtime must fail product validation");
    };

    assert!(
        matches!(
            error.as_ref(),
            NativeProductPlanningError::InvalidExecutableHost(
                ExecutableHostContractBuildError::IncompatibleRuntime(
                    RuntimeCompatibilityError::MissingRole(RuntimeAbiRole::PanicPropagation)
                )
            )
        ),
        "{error:?}"
    );
}

fn runtime_native_plan_with_source_roles(
    sources: &[&str],
    product_kind: ProductKind,
    target: SelectedTarget,
    runtime_roles: impl IntoIterator<Item = bray_runtime_interface::RuntimeRoleSourceBinding>,
) -> (
    Arc<bray_codegen_llvm::LlvmCodeGenerator>,
    Arc<crate::compilation::product::codegen::NativeProductPlan>,
) {
    let (backend, compilation) = codegen_compilation_for_sources_target_with_source_roles(
        sources,
        product_kind,
        target,
        &[],
        [],
        runtime_roles,
        WorkerBudget::serial(),
    );

    let archive = TemporaryFile::write("libbray_runtime.a", b"!<arch>\n");
    let runtime = runtime_artifact(&compilation, archive.path());

    let plan = compilation
        .native_product_plan(
            test_product_identity(),
            crate::BuildConfiguration::Development,
            Some(runtime),
            [],
            Some((&test_linker(), test_linked_product(product_kind))),
        )
        .unwrap_or_else(|error| panic!("runtime source-role plan must resolve: {error:?}"));

    (backend, plan)
}

#[test]
fn library_runtime_defaults_coalesce_with_consumer_generated_bodies() {
    let (backend, plan) = runtime_native_plan_for_product(
        "module app; public func answer(pos value: i32 = 42) -> i32 { return value; }",
        ProductKind::Library,
    );

    let defaults = plan
        .mappings()
        .iter()
        .flat_map(bray_codegen::CodegenMappings::symbols)
        .filter_map(|symbol| match symbol.key() {
            bray_codegen::CodegenSymbolKey::Instance(instance)
                if matches!(instance.template(), MirUnitKey::Bound(unit)
                        if unit.kind() == bray_bound_tree::BoundUnitKind::RuntimeDefault) =>
            {
                Some((symbol, instance))
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(defaults.len(), 1);

    let (symbol, instance) = defaults[0];

    assert_eq!(symbol.linkage(), CodegenLinkage::LinkOnce);

    let compatibility = plan
        .units()
        .iter()
        .find_map(|unit| unit.key().compatibility(instance))
        .expect("default body must retain its partition compatibility");

    assert_eq!(compatibility.linkage(), CodegenLinkage::LinkOnce);

    let backend_ir = generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendIr);

    assert!(backend_ir.iter().any(|bytes| {
        String::from_utf8_lossy(bytes).lines().any(|line| {
            line.starts_with("define weak_odr ") && line.contains(symbol.name().as_str())
        })
    }));
}

#[test]
fn runtime_source_imports_share_generated_runtime_calls() {
    let source = r#"
            trusted module app;
            @abi(c)
            extern trusted internal func reserve(pos count: usize, pos outcome: RawPointer<u8>) uses(foreign_call);
            struct Guard { destruct() {} }
            public trusted func invoke(pos outcome: RawPointer<u8>) uses(foreign_call) {
                let guard = Guard {};
                trusted reserve(1, outcome);
            }
        "#;

    let role = RuntimeAbiRole::OutgoingAdmission;

    let binding = bray_runtime_interface::RuntimeRoleSourceBinding::try_new(role, "app.reserve")
        .unwrap_or_else(|| panic!("runtime source binding must validate"));

    let (backend, plan) = runtime_native_plan_with_source_roles(
        &[source],
        ProductKind::Library,
        SelectedTarget::baseline(),
        [binding],
    );

    assert!(plan.mappings().iter().any(|mappings| mappings.symbols().iter().any(|symbol| matches!(symbol.key(), bray_codegen::CodegenSymbolKey::Runtime(reference) if reference.role() == role))));

    assert!(plan.mappings().iter().all(|mappings| {
        mappings
            .symbols()
            .iter()
            .filter(|symbol| symbol.name().as_str() == role.native_symbol().unwrap())
            .count()
            <= 1
    }));

    assert!(
        generated_artifacts(&backend, &plan)
            .iter()
            .all(|artifact| !artifact.is_empty())
    );
}

#[test]
fn runtime_source_bindings_export_and_retain_the_canonical_role_symbol() {
    let source = concat!(
        "trusted module app;\n",
        "@abi(c)\n",
        "trusted internal func attachment_identity(pos descriptor: RawPointer<u8>) -> u64\n",
        "{\n",
        "    let _: RawPointer<u8> = descriptor;\n",
        "    return 7;\n",
        "}\n",
    );

    let role = RuntimeAbiRole::ThreadAttachmentIdentity;

    let binding =
        bray_runtime_interface::RuntimeRoleSourceBinding::try_new(role, "app.attachment_identity")
            .unwrap_or_else(|| panic!("runtime source binding must validate"));

    let (backend, plan) = runtime_native_plan_with_source_roles(
        &[source],
        ProductKind::Library,
        SelectedTarget::baseline(),
        [binding],
    );

    let symbol = role
        .native_symbol()
        .unwrap_or_else(|| panic!("runtime role must have a native symbol"));

    assert!(plan.mappings().iter().any(|mappings| {
        mappings.symbols().iter().any(|mapping| {
            mapping.name().as_str() == symbol && mapping.linkage() == CodegenLinkage::Export
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

#[test]
fn bootstrap_status_entries_contain_body_failures_without_attaching() {
    for target in bray_target::NativeTarget::ALL {
        for body in [
            "return 73;",
            "panic(\"registration failed\");",
            "let outcome: RunResult<unit> = Cancelled; try outcome; return 0;",
        ] {
            let source = format!(
                r#"
            trusted module app;
            @abi(c)
            trusted internal func register(pos callback: RawPointer<u8>) -> u32
            {{ let _ = callback; {body} }}
        "#
            );

            let role = RuntimeAbiRole::ThreadAttachmentCleanup;

            let binding =
                bray_runtime_interface::RuntimeRoleSourceBinding::try_new(role, "app.register")
                    .unwrap();

            let (backend, plan) = runtime_native_plan_with_source_roles(
                &[&source],
                ProductKind::Library,
                SelectedTarget::for_native(target),
                [binding],
            );

            let symbol = role.native_symbol().unwrap();

            let mapping = plan
                .mappings()
                .iter()
                .flat_map(bray_codegen::CodegenMappings::symbols)
                .find(|mapping| {
                    mapping
                        .native_entry()
                        .is_some_and(|entry| entry.name().as_str() == symbol)
                })
                .expect("bootstrap status operation has a native containment boundary");

            assert_eq!(
                mapping.native_entry().unwrap().kind(),
                bray_runtime_interface::NativeEntryKind::RuntimeStatus
            );

            assert!(mapping.signature().has_panic_report_context());

            let artifacts =
                generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendIr);

            let ir = artifacts
                .iter()
                .map(|artifact| String::from_utf8_lossy(artifact))
                .collect::<String>();

            assert!(
                ir.lines()
                    .any(|line| line.starts_with("define ") && line.contains(symbol))
            );

            assert!(
                !ir.contains(
                    RuntimeAbiRole::ForeignCallbackExecution
                        .native_symbol()
                        .unwrap()
                )
            );

            assert!(
                generated_artifacts(&backend, &plan)
                    .iter()
                    .all(|artifact| !artifact.is_empty())
            );
        }
    }
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn bootstrap_status_cancellation_returns_native_failure_without_reporting_a_panic() {
    let source = r#"
        trusted module app;
        @abi(c)
        trusted internal func register(pos callback: RawPointer<u8>) -> u32 {
            let _ = callback;
            let outcome: RunResult<unit> = Cancelled;
            try outcome;
            return 0;
        }
    "#;

    let role = RuntimeAbiRole::ThreadAttachmentCleanup;

    let binding =
        bray_runtime_interface::RuntimeRoleSourceBinding::try_new(role, "app.register").unwrap();

    let (backend, plan) = runtime_native_plan_with_source_roles(
        &[source],
        ProductKind::Library,
        SelectedTarget::for_native(bray_target::NativeTarget::X86_64WindowsMsvc),
        [binding],
    );

    let caller = format!(
        r#"
        static unsigned reports;

        unsigned {report}(void* value)
        {{
            (void)value;
            ++reports;

            return 0;
        }}

        extern unsigned {entry}(void* callback);

        int main(void)
        {{
            unsigned status = {entry}(0);

            return status == {failure} && reports == 0 ? 0 : 1;
        }}
        "#,
        report = RuntimeAbiRole::PanicReporting.native_symbol().unwrap(),
        entry = role.native_symbol().unwrap(),
        failure = bray_runtime_abi::NativeRuntimeStatus::RUNTIME_FAILURE.code(),
    );

    super::support::linker::run_native_caller(&backend, &plan, &caller);
}

#[test]
fn runtime_source_bindings_retain_referenced_static_atomic_storage() {
    let source = concat!(
        "trusted module app;\n",
        "internal static STATE: core.atomic.Atomic<u32> = core.atomic.initialize<u32>(0);\n",
        "@abi(c)\n",
        "trusted internal func initialize(pos worker_capacity: usize, pos timer_capacity: usize) -> u32\n",
        "{\n",
        "    let _: usize = timer_capacity;\n",
        "    if worker_capacity == 0\n",
        "    {\n",
        "        let _: u32 = core.atomic.load<u32, 1>(&STATE);\n",
        "    }\n",
        "    return core.atomic.load<u32, 1>(&STATE);\n",
        "}\n",
    );

    let role = RuntimeAbiRole::RuntimeInitialization;

    let binding = bray_runtime_interface::RuntimeRoleSourceBinding::try_new(role, "app.initialize")
        .unwrap_or_else(|| panic!("runtime source binding must validate"));

    let (backend, plan) = runtime_native_plan_with_source_roles(
        &[source],
        ProductKind::Library,
        SelectedTarget::baseline(),
        [binding],
    );

    let symbol = role
        .native_symbol()
        .unwrap_or_else(|| panic!("runtime role must have a native symbol"));

    assert!(
        plan.preservation_roots()
            .any(|root| root.as_str() == symbol)
    );

    assert!(plan.mappings().iter().any(|mappings| {
        mappings
            .static_storages()
            .iter()
            .any(bray_codegen::CodegenStaticStorageMapping::defines_storage)
    }));

    assert!(
        generated_artifacts(&backend, &plan)
            .iter()
            .all(|artifact| !artifact.is_empty())
    );
}

#[test]
fn native_atomic_word_calls_survive_generic_specialization() {
    let source = r#"
        module app;
        func generic<T>(pos word: &core.atomic.Atomic<T>, pos expected: T)
            requires(blocking_execution())
        {
            core.atomic.wait<T, 1>(word, expected);
            core.atomic.notify_one<T>(word);
            core.atomic.notify_all<T>(word);
        }
        public func direct(pos word: &core.atomic.Atomic<u32>, pos expected: u32)
            requires(blocking_execution())
        {
            core.atomic.wait<u32, 1>(word, expected);
            core.atomic.notify_one<u32>(word);
            core.atomic.notify_all<u32>(word);
        }
        public func specialized(pos word: &core.atomic.Atomic<u32>, pos expected: u32)
            requires(blocking_execution())
        {
            generic<u32>(word, expected);
        }
    "#;

    let (backend, plan) = runtime_native_plan_for_product(source, ProductKind::Library);

    let artifacts = generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendIr);

    let ir = artifacts
        .iter()
        .map(|artifact| String::from_utf8_lossy(artifact))
        .collect::<String>();

    for role in [
        RuntimeAbiRole::AtomicWordWait,
        RuntimeAbiRole::AtomicWordNotifyOne,
        RuntimeAbiRole::AtomicWordNotifyAll,
    ] {
        let symbol = role.native_symbol().unwrap();

        let calls = ir
            .lines()
            .filter(|line| line.contains("call ") && line.contains(symbol))
            .count();

        assert_eq!(calls, 2, "direct and generic calls must use {symbol}: {ir}");
    }

    assert!(
        ir.contains("load atomic i32") && ir.contains("acquire"),
        "{ir}"
    );
}

#[test]
fn native_atomic_word_production_source_bindings_export() {
    use crate::test_support::source_input;
    use crate::{Compilation, CompilationOptions, CompilationRequest};
    use bray_codegen::{CodeGenerator, CodeGeneratorRegistry, CodegenConfiguration};
    use bray_symbols::{NativeLinkKind, NativeLinkRequirement, PackageIdentity, ProductIdentity};

    let native_apis = [
        include_str!(
            "../../../../../../../../standard-library/std/src/os/generated/x86_64_unknown_linux_gnu.bray"
        ),
        include_str!(
            "../../../../../../../../standard-library/std/src/os/generated/aarch64_unknown_linux_gnu.bray"
        ),
        include_str!(
            "../../../../../../../../standard-library/std/src/os/generated/x86_64_pc_windows_msvc.bray"
        ),
        include_str!(
            "../../../../../../../../standard-library/std/src/os/generated/aarch64_pc_windows_msvc.bray"
        ),
        include_str!(
            "../../../../../../../../standard-library/std/src/os/generated/x86_64_apple_darwin.bray"
        ),
        include_str!(
            "../../../../../../../../standard-library/std/src/os/generated/aarch64_apple_darwin.bray"
        ),
    ];

    // Retain real SDK declarations. Loader convenience bodies need a different library closure.
    let native_apis = native_apis.map(|source| {
        source
            .split_once("// Dynamic code symbol")
            .expect("generated SDK separates its loader helpers")
            .0
    });

    let sources = [
        include_str!("../../../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../../../standard-library/std/src/numeric/policy.bray"),
        include_str!("../../../../../../../../standard-library/std/src/os.bray"),
        include_str!("../../../../../../../../standard-library/std/src/os/windows.bray"),
        include_str!("../../../../../../../../runtime/bootstrap/src/atomic.bray"),
        include_str!("../../../../../../../../runtime/bootstrap/src/atomic/linux.bray"),
        include_str!("../../../../../../../../runtime/bootstrap/src/atomic/darwin.bray"),
        include_str!("../../../../../../../../runtime/bootstrap/src/atomic/windows.bray"),
    ];

    let roles = [
        RuntimeAbiRole::AtomicWordWait,
        RuntimeAbiRole::AtomicWordNotifyOne,
        RuntimeAbiRole::AtomicWordNotifyAll,
    ];

    for target in bray_target::NativeTarget::ALL {
        let backend = Arc::new(bray_codegen_llvm::LlvmCodeGenerator::try_new().unwrap());

        let registry =
            CodeGeneratorRegistry::try_new([Arc::clone(&backend) as Arc<dyn CodeGenerator>])
                .unwrap();

        let codegen = CodegenConfiguration::try_new(registry, backend.identity().clone()).unwrap();

        let request = CompilationRequest::with_options(
            PackageIdentity::try_new("std").unwrap(),
            sources
                .iter()
                .chain(&native_apis)
                .enumerate()
                .map(|(index, source)| source_input(source, u32::try_from(index).unwrap()))
                .collect(),
            CompilationOptions::new(
                WorkerBudget::serial(),
                ProductKind::Library,
                SelectedTarget::for_native(target),
            )
            .with_native_link_inputs(
                [
                    ("c", NativeLinkKind::System),
                    ("pthread", NativeLinkKind::System),
                    ("dl", NativeLinkKind::Dynamic),
                    ("CoreFoundation", NativeLinkKind::Framework),
                    ("Kernel32", NativeLinkKind::System),
                    ("Bcrypt", NativeLinkKind::System),
                    ("Mincore", NativeLinkKind::System),
                    ("Shell32", NativeLinkKind::System),
                    ("ucrt", NativeLinkKind::System),
                ]
                .map(|(name, kind)| {
                    NativeLinkRequirement::new(
                        bray_base::NonEmptySharedStr::try_new(name).unwrap(),
                        kind,
                    )
                }),
            ),
        )
        .with_standard_library_source_authority()
        .with_runtime_roles(roles.into_iter().map(|role| role.source_binding().unwrap()));

        let compilation = Compilation::load_with_codegen(request, codegen).unwrap();

        let product =
            ProductIdentity::try_new(PackageIdentity::try_new("std").unwrap(), "atomic").unwrap();

        let plan = compilation
            .native_product_plan(
                product,
                crate::BuildConfiguration::Development,
                None,
                [],
                None,
            )
            .unwrap_or_else(|error| {
                panic!("actual native atomic source must export for {target:?}: {error:?}")
            });

        for role in roles {
            assert!(
                plan.mappings()
                    .iter()
                    .flat_map(bray_codegen::CodegenMappings::symbols)
                    .any(
                        |mapping| mapping.name().as_str() == role.native_symbol().unwrap()
                            && mapping.linkage() == CodegenLinkage::Export
                    ),
                "{target:?}: {role:?}"
            );
        }

        assert!(
            generated_artifacts(&backend, &plan)
                .iter()
                .all(|artifact| !artifact.is_empty()),
            "{target:?}"
        );
    }
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn foreign_callbacks_resolve_abnormal_outcomes_before_boundary_release() {
    let source = r#"
        trusted module app;
        @link(name = "native")
        @symbol(name = "record_cleanup")
        @abi(c)
        extern trusted func record_cleanup() executes(total) uses(foreign_call);
        struct Local {
            tag: u32;
            trusted destruct() executes(total) uses(foreign_call) {
                trusted record_cleanup();
            }
        }
        @layout(c)
        struct Aggregate { first: u64; second: u64; third: u64; }
        @layout(c)
        struct Packed
        {
            first: u32;
            second: u32;
        }
        @link(name = "native")
        @symbol(name = "native_packed")
        @abi(c)
        extern trusted func native_packed(pos value: Packed) -> Packed uses(foreign_call);
        @abi(c)
        func packed(pos value: Packed) -> Packed
        {
            return { first = value.second, second = value.first };
        }
        callable PackedCallback = @abi(c) func(pos value: Packed) -> Packed;
        @symbol(name = "packed_round_trip")
        @abi(c)
        trusted func packed_round_trip(pos value: Packed) -> Packed
            uses(foreign_call)
        {
            let callback: PackedCallback = packed;
            let result = callback(value);

            return trusted native_packed(result);
        }
        @symbol(name = "scalar")
        @abi(c)
        func scalar(pos mode: u32) -> u32 {
            if mode == 1 {
                let outcome: RunResult<unit> = Cancelled;
                try outcome;
            }
            if mode == 2 { panic("scalar"); }
            return 73;
        }
        @symbol(name = "aggregate")
        @abi(c)
        func aggregate(pos mode: u32) -> Aggregate {
            if mode == 1 {
                let outcome: RunResult<unit> = Cancelled;
                try outcome;
            }
            if mode == 2 { panic("aggregate"); }
            return { first = 17, second = 29, third = 41 };
        }
        @abi(c)
        func plain(pos mode: u32) -> u32 { return scalar(mode); }
        callable Callback = @abi(c) func(pos mode: u32) -> u32;
        @symbol(name = "through_pointer")
        @abi(c)
        func through_pointer(pos mode: u32) -> u32 {
            let callback: Callback = plain;
            return callback(mode);
        }
        @symbol(name = "through_lambda")
        @abi(c)
        func through_lambda(pos mode: u32) -> u32 {
            let callback = @abi(c) lambda(pos value: u32) -> u32 { return scalar(value); };
            return callback(mode);
        }
        @symbol(name = "nested")
        @abi(c)
        func nested(pos mode: u32) -> u32 {
            let local: Local = { tag = 1 };
            return plain(mode);
        }
    "#;

    let caller = format!(
        r#"
        #include <stdint.h>
        #include <stdlib.h>

        typedef struct
        {{
            _Alignas({report_align}) unsigned char data[{report_size}];
        }} Report;

        typedef struct
        {{
            uint32_t state;
            uintptr_t payload;
            Report report;
        }} Outcome;

        typedef struct
        {{
            uint64_t first, second, third;
        }} Aggregate;

        typedef struct
        {{
            uint32_t first, second;
        }} Packed;

        Packed native_packed(Packed value)
        {{
            return (Packed){{ value.first ^ 1, value.second ^ 2 }};
        }}

        extern Packed packed_round_trip(Packed value);

        static unsigned cleanups;

        void record_cleanup(void)
        {{
            ++cleanups;
        }}

        static unsigned shield_depth;

        void {shield_enter}(void)
        {{
            ++shield_depth;
        }}

        void {shield_leave}(void)
        {{
            if (!shield_depth)
                _Exit(10);

            --shield_depth;
        }}

        void {propagate_panic}(Report report)
        {{
            (void)report;
            _Exit(11);
        }}

        void {propagate_cancellation}(void)
        {{
            _Exit(12);
        }}

        Report {suppress}(Report first, Report second)
        {{
            (void)first;
            (void)second;
            _Exit(13);
        }}

        static unsigned reports;
        static unsigned cancelled;
        static unsigned terminal_status;
        static unsigned reject_entry;

        // The C final image supplies the descriptor consumed by this stubbed control.
        const unsigned char bray_linked_product_host = 0;

        typedef struct
        {{
            uint32_t status, state;
            uintptr_t entries, external, threads, initialized, cleaned, incidents;
            unsigned char identity[32];
        }} Observation;

        void {control}(Observation* result, const void* descriptor, unsigned operation)
        {{
            (void)descriptor;
            *result = (Observation){{0}};

            if (operation == {acquire} && reject_entry)
                result->status = {closed};
        }}

        unsigned {consumer}(Report* value, unsigned reporting)
        {{
            (void)value;
            (void)reporting;

            return 0;
        }}

        unsigned {boundary}(
            void (*callback)(uintptr_t, Outcome*, void*), uintptr_t context, Outcome* outcome)
        {{
            callback(context, outcome, NULL);

            if (outcome->state == {cancelled})
                ++cancelled;

            if (terminal_status == {represented})
                ++reports;

            return terminal_status;
        }}

        Report {construction}(uint32_t cause,
            uint32_t present,
            uint32_t identity,
            uint32_t start,
            uint32_t end,
            uint64_t version,
            uint64_t n0,
            uint64_t n1,
            uint64_t n2,
            uint64_t n3,
            const void* message,
            uintptr_t length)
        {{
            Report report = {{0}};

            return report;
        }}

        unsigned {report}(Report value)
        {{
            (void)value;
            ++reports;

            return 0;
        }}

        extern uint32_t scalar(uint32_t mode);
        extern uint32_t nested(uint32_t mode);
        extern uint32_t through_pointer(uint32_t mode);
        extern uint32_t through_lambda(uint32_t mode);
        extern Aggregate aggregate(uint32_t mode);

        int main(void)
        {{
            Packed packed = packed_round_trip((Packed){{ 0x12345678, 0x9abcdef0 }});

            if (packed.first != 0x9abcdef1 || packed.second != 0x1234567a)
                return 40;

            if (scalar(0) != 73 || scalar(1) != 0 || scalar(2) != 0)
                return 1;

            Aggregate normal = aggregate(0), failure = aggregate(1), panic = aggregate(2);

            if (normal.first != 17 || normal.second != 29 || normal.third != 41)
                return 2;

            if (failure.first || failure.second || failure.third)
                return 3;

            if (panic.first || panic.second || panic.third)
                return 4;

            if (nested(0) != 73 || nested(1) != 0 || nested(2) != 0)
                return 6;

            if (through_pointer(0) != 73 || through_pointer(1) != 0 || through_pointer(2) != 0)
                return 7;

            if (through_lambda(0) != 73 || through_lambda(1) != 0 || through_lambda(2) != 0)
                return 8;

            if (reports != 5)
                return 31;

            if (cleanups != 3)
                return 32;

            if (shield_depth != 0)
                return 33;

            // The callback resolves its body outcome before releasing the entry.
            if (cancelled != 0)
                return 34;

            terminal_status = {represented};

            if (scalar(0) != 73 || reports != 6)
                return 14;

            terminal_status = {unrepresented};

            if (scalar(0) != 73 || reports != 7)
                return 15;

            terminal_status = {represented};
            normal = aggregate(0);

            if (normal.first != 17 || normal.second != 29 || normal.third != 41 || reports != 8)
                return 16;

            reject_entry = 1;

            if (nested(0) != 0 || cleanups != 3)
                return 17;

            failure = aggregate(0);

            if (failure.first || failure.second || failure.third || reports != 8)
                return 18;

            return 0;
        }}

    "#,
        shield_enter = RuntimeAbiRole::CleanupShieldEnter.native_symbol().unwrap(),
        shield_leave = RuntimeAbiRole::CleanupShieldLeave.native_symbol().unwrap(),
        propagate_panic = RuntimeAbiRole::PanicPropagation.native_symbol().unwrap(),
        propagate_cancellation = RuntimeAbiRole::CurrentRunCancellationPropagation
            .native_symbol()
            .unwrap(),
        construction = RuntimeAbiRole::PanicReportConstruction
            .native_symbol()
            .unwrap(),
        suppress = RuntimeAbiRole::PanicReportSuppression
            .native_symbol()
            .unwrap(),
        acquire = bray_runtime_abi::NativeProductHostOperation::ACQUIRE_ENTRY.code(),
        closed = bray_runtime_abi::NativeProductHostStatus::CLOSED.code(),
        represented = bray_runtime_abi::NativeRuntimeStatus::PANICKED.code(),
        unrepresented = bray_runtime_abi::NativeRuntimeStatus::RUNTIME_FAILURE.code(),
        report_align = std::mem::align_of::<bray_runtime_abi::NativePanicReport>(),
        report_size = std::mem::size_of::<bray_runtime_abi::NativePanicReport>(),
        boundary = RuntimeAbiRole::ForeignCallbackExecution
            .native_symbol()
            .unwrap(),
        cancelled = bray_runtime_abi::NativeRunState::CANCELLED.code(),
        control = RuntimeAbiRole::ProductHostControl.native_symbol().unwrap(),
        consumer = RuntimeAbiRole::ReportConsumer.native_symbol().unwrap(),
        report = RuntimeAbiRole::PanicReporting.native_symbol().unwrap(),
    );

    use super::support::dependencies::{
        GenericDependencyFixture, dependency_from_fixture_for_target,
    };

    use bray_codegen::{CodeGenerator, CodeGeneratorRegistry, CodegenConfiguration};

    let backend = std::sync::Arc::new(bray_codegen_llvm::LlvmCodeGenerator::try_new().unwrap());

    let registry = CodeGeneratorRegistry::try_new([
        std::sync::Arc::clone(&backend) as std::sync::Arc<dyn CodeGenerator>
    ])
    .unwrap();

    let codegen = CodegenConfiguration::try_new(registry, backend.identity().clone()).unwrap();
    let target = SelectedTarget::for_native(bray_target::NativeTarget::X86_64WindowsMsvc);

    for imported in [false, true] {
        let mut dependencies = vec![crate::test_support::runtime_standard_library_dependency(
            &target,
        )];

        let source = if imported {
            dependencies.push(dependency_from_fixture_for_target(
                true,
                false,
                GenericDependencyFixture {
                    source: r#"
                    module templates;
                    @abi(c)
                    func leaf(pos mode: u32) -> u32 {
                        if mode == 1 {
                            let outcome: RunResult<unit> = Cancelled;
                            try outcome;
                        }
                        if mode == 2 { panic("imported"); }
                        return 73;
                    }
                    @abi(c)
                    public func forwarded(pos mode: u32) -> u32 { return leaf(mode); }
                "#,
                    runtime_frames: None,
                    executable_templates: 2,
                    platform_service: None,
                },
                None,
                target.clone(),
            ));

            source
                .replace(
                    "trusted module app;",
                    "trusted module app; using example.dependency.templates.forwarded;",
                )
                .replace(
                    "return scalar(mode);",
                    "return example.dependency.templates.forwarded(mode);",
                )
        } else {
            source.to_owned()
        };

        let request = crate::CompilationRequest::with_options(
            crate::test_support::package_identity(),
            vec![crate::test_support::source_input(&source, 0)],
            crate::CompilationOptions::new(
                crate::WorkerBudget::serial(),
                ProductKind::Library,
                target.clone(),
            )
            .with_native_link_inputs([bray_symbols::NativeLinkRequirement::new(
                bray_base::NonEmptySharedStr::try_new("native").unwrap(),
                bray_symbols::NativeLinkKind::Static,
            )]),
        )
        .with_dependency_interfaces(dependencies);

        let compilation = crate::Compilation::load_with_codegen(request, codegen.clone()).unwrap();

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );

        let archive = bray_testing::TemporaryFile::write("libbray_runtime.a", b"!<arch>\n");
        let runtime = super::support::runtime::runtime_artifact(&compilation, archive.path());

        let plan = compilation
            .native_product_plan(
                super::support::compilation::test_product_identity(),
                crate::BuildConfiguration::Development,
                Some(runtime),
                [],
                Some((
                    &super::support::linker::test_linker(),
                    super::support::linker::test_linked_product(ProductKind::Library),
                )),
            )
            .unwrap();

        super::support::linker::run_native_caller(&backend, &plan, &caller);
    }
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn entry_cleanup_panic_preserves_completion_or_appends_after_the_body_panic() {
    let source = r#"
        trusted module app;
        trusted internal predicate native_entry();
        @abi(c)
        trusted internal func invoke(pos callback: RawPointer<u8>, pos context: usize,
            pos main: u32, pos outcome: RawPointer<u8>) -> u32
            requires(trusted native_entry()) {
            let _ = callback;
            let _ = context;
            let _ = main;
            let _ = outcome;
            panic("entry cleanup");
        }
    "#;

    let role = RuntimeAbiRole::EntryContinuation;

    let binding =
        bray_runtime_interface::RuntimeRoleSourceBinding::try_new(role, "app.invoke").unwrap();

    let (backend, plan) = runtime_native_plan_with_source_roles(
        &[source],
        ProductKind::Library,
        SelectedTarget::for_native(bray_target::NativeTarget::X86_64WindowsMsvc),
        [binding],
    );

    let caller = format!(
        r#"
        #include <stdint.h>
        #include <stdlib.h>

        typedef struct
        {{
            _Alignas({align}) unsigned char data[{size}];
        }} Report;

        typedef struct
        {{
            uint32_t state;
            uintptr_t payload;
            Report report;
        }} Outcome;

        static unsigned reported, merged, shield_depth;

        void {shield_enter}(void)
        {{
            ++shield_depth;
        }}

        void {shield_leave}(void)
        {{
            if (!shield_depth)
                _Exit(8);

            --shield_depth;
        }}

        void {propagate_panic}(Report* report)
        {{
            (void)report;
            _Exit(9);
        }}

        void {propagate_cancellation}(void)
        {{
            _Exit(10);
        }}

        Report {construct}(uint32_t cause,
            uint32_t present,
            uint32_t identity,
            uint32_t start,
            uint32_t end,
            uint64_t version,
            uint64_t n0,
            uint64_t n1,
            uint64_t n2,
            uint64_t n3,
            const void* text,
            uintptr_t length)
        {{
            Report result = {{0}};
            result.data[0] = 22;

            return result;
        }}

        unsigned {report}(Report value)
        {{
            if (value.data[0] != 22)
                _Exit(1);

            ++reported;

            return 0;
        }}

        Report {suppress}(Report* first, Report* second)
        {{
            if (first->data[0] != 11 || second->data[0] != 22)
                _Exit(2);

            Report result = *first;
            *first = (Report){{0}};
            *second = (Report){{0}};
            result.data[1] = 22;
            ++merged;

            return result;
        }}

        extern unsigned {entry}(void*, uintptr_t, unsigned, Outcome*);

        int main(void)
        {{
            Outcome completed = {{0, 73, {{0}}}};

            if ({entry}(0, 0, 0, &completed) != {represented})
                return 3;

            if (completed.state || completed.payload != 73 || reported != 1 || merged)
                return 4;

            Outcome panicked = {{{panicked}, 0, {{0}}}};
            panicked.report.data[0] = 11;

            if ({entry}(0, 0, 0, &panicked) != {represented})
                return 5;

            if (panicked.state != {panicked} || panicked.report.data[0] != 11 ||
                panicked.report.data[1] != 22)
                return 6;

            return reported == 1 && merged == 1 && shield_depth == 0 ? 0 : 7;
        }}

    "#,
        shield_enter = RuntimeAbiRole::CleanupShieldEnter.native_symbol().unwrap(),
        shield_leave = RuntimeAbiRole::CleanupShieldLeave.native_symbol().unwrap(),
        propagate_panic = RuntimeAbiRole::PanicPropagation.native_symbol().unwrap(),
        propagate_cancellation = RuntimeAbiRole::CurrentRunCancellationPropagation
            .native_symbol()
            .unwrap(),
        align = std::mem::align_of::<bray_runtime_abi::NativePanicReport>(),
        size = std::mem::size_of::<bray_runtime_abi::NativePanicReport>(),
        construct = RuntimeAbiRole::PanicReportConstruction
            .native_symbol()
            .unwrap(),
        report = RuntimeAbiRole::PanicReporting.native_symbol().unwrap(),
        suppress = RuntimeAbiRole::PanicReportSuppression
            .native_symbol()
            .unwrap(),
        entry = role.native_symbol().unwrap(),
        represented = bray_runtime_abi::NativeRuntimeStatus::PANICKED.code(),
        panicked = bray_runtime_abi::NativeRunState::PANICKED.code(),
    );

    super::support::linker::run_native_caller(&backend, &plan, &caller);
}
