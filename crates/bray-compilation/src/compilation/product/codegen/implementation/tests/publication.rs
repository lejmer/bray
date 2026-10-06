use super::support::artifacts::{generated_artifacts, generated_artifacts_of_kind};
use super::support::mappings::assert_native_callback_entry;
use super::support::runtime::{
    runtime_native_plan_for_sources_target,
    runtime_native_plan_for_sources_target_with_platform_services,
};
use crate::SelectedTarget;
use bray_base::NonEmptySharedStr;
use bray_codegen::{BackendArtifactKind, CodegenLinkage};
use bray_ir::MirUnitKind;
use bray_native_artifact::NativeUnitSummary;
use bray_runtime_interface::{PlatformServiceBinding, PlatformServiceRole};
use bray_symbols::{NativeLinkKind, NativeLinkRequirement, NativeSymbolBinding, ProductKind};
use bray_target::NativeTarget;
use std::collections::BTreeSet;
use std::fs;

#[test]
fn native_exports_and_opaque_storage_survive_reachability_and_codegen() {
    let source = concat!(
        "module app;\n",
        "@layout(c, size = 40, align = 8)\n",
        "struct NativeMutex;\n",
        "@link(name = \"native\")\n",
        "@symbol(name = \"native_mutex\")\n",
        "extern trusted static NATIVE_MUTEX_STORAGE: NativeMutex;\n",
        "@link(name = \"native\")\n",
        "@symbol(name = \"native_pointer\")\n",
        "extern trusted static mut NATIVE_POINTER: RawPointer<u8>;\n",
        "@symbol(name = \"unused_export\")\n",
        "static UNUSED_EXPORT: i32 = 7;\n",
        "@symbol(name = \"weak_export\", binding = weak)\n",
        "@abi(c)\n",
        "func weak_export() -> i32\n",
        "{\n",
        "    return 1;\n",
        "}\n",
        "func main()\n",
        "{\n",
        "    let value: i32 = weak_export();\n",
        "    let pointer: RawPointer<NativeMutex> = NATIVE_MUTEX_STORAGE;\n",
        "    let pointer_storage: RawPointer<RawPointer<u8>> = NATIVE_POINTER;\n",
        "}\n",
    );

    let native_link = NativeLinkRequirement::new(
        NonEmptySharedStr::try_new("native")
            .unwrap_or_else(|| panic!("native link name must be valid")),
        NativeLinkKind::Dynamic,
    );

    let (backend, plan) = runtime_native_plan_for_sources_target(
        &[source],
        ProductKind::Executable,
        SelectedTarget::for_native(NativeTarget::X86_64WindowsMsvc),
        &[native_link],
    );

    let host = plan
        .executable_host()
        .unwrap_or_else(|| panic!("executable plan must retain its host"));

    assert_eq!(host.entries().len(), 1);

    let callback_body = assert_native_callback_entry(&plan, "weak_export", CodegenLinkage::Weak);

    let definitions =
        crate::compilation::product::codegen::link::product_native_definitions(plan.mappings());

    let exports =
        crate::compilation::product::codegen::plan::product_native_exports(plan.mappings())
            .map(|name| name.as_str())
            .collect::<BTreeSet<_>>();

    assert!(exports.contains("weak_export"));
    assert!(exports.contains("unused_export"));
    assert!(!exports.contains(callback_body.as_str()));

    assert_eq!(
        definitions.get("weak_export"),
        Some(&NativeSymbolBinding::Weak)
    );

    assert_eq!(
        definitions.get(callback_body.as_str()),
        Some(&NativeSymbolBinding::Strong)
    );

    assert!(plan.mappings().iter().any(|mappings| {
        mappings.static_storages().iter().any(|mapping| {
            mapping.symbol().as_str() == "unused_export"
                && mapping.native_binding() == Some(NativeSymbolBinding::Strong)
                && mapping.defines_storage()
        })
    }));

    assert_eq!(
        definitions.get("unused_export"),
        Some(&NativeSymbolBinding::Strong)
    );

    assert_eq!(
        definitions.get("bray.static.host.unused_export"),
        Some(&NativeSymbolBinding::Strong)
    );

    assert!(plan.mappings().iter().any(|mappings| {
        mappings.types().iter().any(|mapping| {
            mapping
                .layout()
                .is_some_and(|layout| layout.size() == 40 && layout.alignment().get() == 8)
        })
    }));

    let backend_ir = generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendIr);

    for artifact in &backend_ir {
        let ir = String::from_utf8_lossy(artifact);

        for redundant_retention in [
            "dllexport",
            "@llvm.used",
            "@llvm.compiler.used",
            "section \".bray",
        ] {
            assert!(!ir.contains(redundant_retention));
        }
    }

    assert!(
        backend_ir
            .iter()
            .any(|artifact| { String::from_utf8_lossy(artifact).contains("@unused_export =") })
    );

    assert!(backend_ir.iter().any(|artifact| {
        String::from_utf8_lossy(artifact)
            .lines()
            .any(|line| line.contains(" call ") && line.contains(&callback_body))
    }));

    assert!(
        generated_artifacts(&backend, &plan)
            .iter()
            .all(|artifact| !artifact.is_empty())
    );
}

const FALLBACK_LIBRARY_SOURCE: &str = r#"
            trusted module app;
            @layout(c)
            internal struct PlatformStatus {
                category: u32;
                reserved: u32;
                native_code: i64;
            }
            @abi(c)
            trusted internal func flush() -> PlatformStatus {
                return { category = 1, reserved = 0, native_code = 0 };
            }
            @symbol(name = "live")
            static LIVE: i32 = 3;
        "#;

#[test]
fn ordinary_library_fallback_publication_is_exact() {
    let source = FALLBACK_LIBRARY_SOURCE;

    let role = PlatformServiceRole::StandardOutputFlush;
    let binding = PlatformServiceBinding::try_new(role, "app.flush").unwrap();

    let (backend, plan) = runtime_native_plan_for_sources_target_with_platform_services(
        &[source],
        ProductKind::Library,
        SelectedTarget::for_native(NativeTarget::X86_64WindowsMsvc),
        &[],
        [binding],
    );

    let ir = generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendIr);
    let objects = generated_artifacts(&backend, &plan);

    for (ir, object) in ir.iter().zip(objects) {
        let text = String::from_utf8_lossy(ir);

        if !text.contains("/alternatename:") {
            continue;
        }

        let summary = bray_native_artifact::scan_object_unit_summary(&object).unwrap();

        let bray_native_artifact::NativeUnitSummary::Exact {
            definitions, roots, ..
        } = summary
        else {
            panic!("ordinary fallback object must publish exact selection");
        };

        assert!(roots.is_empty());

        assert!(
            definitions
                .iter()
                .any(|definition| definition.symbol().identity().name()
                    == Some(role.native_symbol())
                    && definition.selection()
                        == &bray_native_artifact::NativeDefinitionSelection::Fallback)
        );
    }

    assert!(
        ir.iter()
            .any(|ir| String::from_utf8_lossy(ir).contains("/alternatename:"))
    );

    let (backend, plan) = runtime_native_plan_for_sources_target_with_platform_services(
        &[source],
        ProductKind::Library,
        SelectedTarget::for_native(NativeTarget::X86_64LinuxGnu),
        &[],
        [PlatformServiceBinding::try_new(role, "app.flush").unwrap()],
    );

    let bitcode = generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendBitcode);

    let mut ordinary = false;

    for bytes in bitcode {
        let summary =
            bray_codegen_llvm::inspect_bitcode_unit_summary(&bytes, NativeTarget::X86_64LinuxGnu)
                .unwrap();

        match summary {
            bray_native_artifact::NativeUnitSummary::Exact { definitions, .. } => {
                ordinary |= definitions
                    .iter()
                    .any(|definition| definition.symbol().identity().name() == Some("live"));
            }
            bray_native_artifact::NativeUnitSummary::Opaque { provided, .. } => {
                assert!(
                    !provided.iter().any(|symbol| symbol.name() == Some("live")),
                    "an opaque fallback must not hide the ordinary export"
                );
            }
        }
    }

    assert!(
        ordinary,
        "the ordinary export must remain independently exact"
    );
}

#[test]
fn macho_string_and_static_helpers_keep_unrelated_exports_exact() {
    let target = NativeTarget::X86_64MacOs;

    for user in [
        r#"
                internal static VALUE: i32 = 42;

                func user() -> i32
                {
                    return VALUE;
                }
            "#,
        r#"
                func user() -> &string
                {
                    return &"shared literal";
                }
            "#,
    ] {
        let mut source = format!("module app;\n{user}");

        for index in 0..8 {
            source.push_str(&format!(
                r#"
                        func ordinary_{index}(pos value: i32) -> i32
                        {{
                            return value + {index};
                        }}
                    "#
            ));
        }

        let (backend, plan) = runtime_native_plan_for_sources_target(
            &[&source],
            ProductKind::Library,
            SelectedTarget::for_native(target),
            &[],
        );

        let mut ordinary = BTreeSet::new();
        let mut user_symbol = None;

        for (unit, mappings) in plan.units().iter().zip(plan.mappings()) {
            for instance in unit.instances() {
                if !matches!(instance.mir().kind(), MirUnitKind::Synchronous)
                    || !unit
                        .compatibility(instance.key())
                        .is_some_and(|class| class.linkage() == CodegenLinkage::Export)
                {
                    continue;
                }

                let symbol = mappings
                    .symbols()
                    .iter()
                    .find(|symbol| {
                        matches!(symbol.key(), bray_codegen::CodegenSymbolKey::Instance(key)
                            if key == instance.key())
                    })
                    .expect("a generated export must have a symbol mapping");

                let name = target
                    .object_symbol_name(symbol.name().as_str())
                    .into_owned();

                if instance
                    .mir()
                    .storages()
                    .iter()
                    .any(|storage| matches!(storage.kind(), bray_ir::MirStorageKind::Parameter(_)))
                {
                    ordinary.insert(name);
                } else {
                    assert!(
                        unit.compatibility(instance.key())
                            .expect("helper user metadata must resolve")
                            .native_selection_boundary()
                    );

                    user_symbol = Some(name);
                }
            }
        }

        assert_eq!(ordinary.len(), 8);

        assert!(user_symbol.is_some(), "the helper user must be exported");

        let mut exact = BTreeSet::new();
        let mut opaque_helpers = false;

        for bytes in
            generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendBitcode)
        {
            match bray_codegen_llvm::inspect_bitcode_unit_summary(&bytes, target).unwrap() {
                NativeUnitSummary::Exact { definitions, .. } => {
                    exact.extend(definitions.iter().filter_map(|definition| {
                        definition.symbol().identity().name().map(str::to_owned)
                    }));
                }
                NativeUnitSummary::Opaque { provided, .. } => {
                    opaque_helpers |= !provided.is_empty();

                    assert!(
                        provided.iter().all(|symbol| {
                            symbol.name().is_none_or(|name| !ordinary.contains(name))
                        }),
                        "MachO weak helpers must not hide unrelated strong exports"
                    );
                }
            }
        }

        assert!(
            opaque_helpers,
            "MachO weak helpers must retain their opaque summaries"
        );

        assert!(ordinary.is_subset(&exact));
    }
}

#[test]
fn library_publication_preserves_named_native_dependencies() {
    for callable in [false, true] {
        let mut source = String::from("trusted module app;\n");

        source.push_str(if callable {
            r#"
            @link(name = "native")
            @symbol(name = "native_a")
            @abi(c)
            extern trusted func NATIVE_A() -> i32 uses(foreign_call);
            @link(name = "native")
            @symbol(name = "native_b")
            @abi(c)
            extern trusted func NATIVE_B() -> i32 uses(foreign_call);
        "#
        } else {
            r#"
            @link(name = "native")
            @symbol(name = "native_a")
            extern trusted static NATIVE_A: i32;
            @link(name = "native")
            @symbol(name = "native_b")
            extern trusted static NATIVE_B: i32;
        "#
        });

        for (storage, suffix) in [("NATIVE_A", "a"), ("NATIVE_B", "b")] {
            for index in 0..4 {
                source.push_str(&if callable {
                    format!(
                        r#"
                        trusted func get_{suffix}_{index}() -> i32 uses(foreign_call)
                        {{
                            return trusted {storage}();
                        }}
                    "#
                    )
                } else {
                    format!(
                        r#"
                        trusted func get_{suffix}_{index}() -> RawPointer<i32>
                        {{
                            return {storage};
                        }}
                    "#
                    )
                });
            }
        }

        let target = NativeTarget::X86_64LinuxGnu;

        let (backend, plan) = runtime_native_plan_for_sources_target(
            &[&source],
            ProductKind::Library,
            SelectedTarget::for_native(target),
            &[NativeLinkRequirement::new(
                NonEmptySharedStr::try_new("native").unwrap(),
                NativeLinkKind::Dynamic,
            )],
        );

        assert!(plan.units().iter().any(|unit| unit.instances().len() > 1));

        let mut selected = BTreeSet::new();

        for bytes in
            generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendBitcode)
        {
            let summary = bray_codegen_llvm::inspect_bitcode_unit_summary(&bytes, target).unwrap();

            let references = summary
                .references()
                .iter()
                .filter_map(|symbol| symbol.identity().name())
                .filter(|name| matches!(*name, "native_a" | "native_b"))
                .collect::<BTreeSet<_>>();

            assert!(
                references.len() <= 1,
                "unrelated named native providers must not share a publication unit"
            );

            selected.extend(references.into_iter().map(str::to_owned));
        }

        assert_eq!(
            selected,
            BTreeSet::from(["native_a".to_owned(), "native_b".to_owned()])
        );
    }
}

#[test]
fn optional_native_storage_keeps_unrelated_library_exports_exact() {
    let source = r#"
            trusted module app;
            @link(name = "native")
            @symbol(name = "optional_native_value", presence = optional)
            extern trusted static NATIVE_VALUE: i32;

            trusted func optional_value() -> RawPointer<i32> {
                return NATIVE_VALUE;
            }

            func ordinary_value(pos value: i32) -> i32 {
                return value * 3 + 7;
            }
        "#;

    let (backend, plan) = runtime_native_plan_for_sources_target(
        &[source],
        ProductKind::Library,
        SelectedTarget::for_native(NativeTarget::X86_64LinuxGnu),
        &[NativeLinkRequirement::new(
            NonEmptySharedStr::try_new("native").unwrap(),
            NativeLinkKind::Dynamic,
        )],
    );

    let symbol_for_storage = |native_storage| {
        plan.units()
            .iter()
            .zip(plan.mappings())
            .find_map(|(unit, mappings)| {
                let instance = unit.instances().iter().find(|instance| {
                    matches!(instance.mir().kind(), MirUnitKind::Synchronous)
                        && unit
                            .compatibility(instance.key())
                            .is_some_and(|class| class.linkage() == CodegenLinkage::Export)
                        && instance.mir().storages().iter().any(|storage| {
                            matches!(storage.kind(), bray_ir::MirStorageKind::NativeStatic(_))
                        }) == native_storage
                })?;

                mappings.symbols().iter().find_map(|symbol| {
                    matches!(symbol.key(), bray_codegen::CodegenSymbolKey::Instance(key)
                            if key == instance.key())
                    .then(|| symbol.name())
                })
            })
            .expect("the library must publish both strong Bray function exports")
    };

    let ordinary_symbol = symbol_for_storage(false);
    let optional_symbol = symbol_for_storage(true);

    let bitcode = generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendBitcode);

    let mut ordinary = false;
    let mut optional = false;

    for bytes in bitcode {
        let summary =
            bray_codegen_llvm::inspect_bitcode_unit_summary(&bytes, NativeTarget::X86_64LinuxGnu)
                .unwrap();

        match summary {
            bray_native_artifact::NativeUnitSummary::Exact { definitions, .. } => {
                ordinary |= definitions.iter().any(|definition| {
                    definition.symbol().identity().name() == Some(ordinary_symbol.as_str())
                });
            }
            bray_native_artifact::NativeUnitSummary::Opaque { provided, .. } => {
                optional |= provided
                    .iter()
                    .any(|symbol| symbol.name() == Some(optional_symbol.as_str()));

                assert!(
                    !provided
                        .iter()
                        .any(|symbol| symbol.name() == Some(ordinary_symbol.as_str())),
                    "an optional native reference must not hide the ordinary export"
                );
            }
        }
    }

    assert!(
        ordinary && optional,
        "both definitions must survive with their native selection semantics"
    );
}

#[test]
fn mixed_ordinary_library_fallbacks_yield_to_strong_objects_and_lazy_archives() {
    let source = FALLBACK_LIBRARY_SOURCE;

    let role = PlatformServiceRole::StandardOutputFlush;

    let (backend, plan) = runtime_native_plan_for_sources_target_with_platform_services(
        &[source],
        ProductKind::Library,
        SelectedTarget::for_native(NativeTarget::X86_64WindowsMsvc),
        &[],
        [PlatformServiceBinding::try_new(role, "app.flush").unwrap()],
    );

    let directory = tempfile::tempdir().unwrap();

    let prefix = std::path::Path::new(bray_codegen_llvm::COMPILED_LLVM_PREFIX.unwrap()).join("bin");

    let run = |tool: &str, arguments: Vec<std::ffi::OsString>| {
        let output = std::process::Command::new(
            prefix.join(format!("{tool}{}", std::env::consts::EXE_SUFFIX)),
        )
        .args(arguments)
        .current_dir(directory.path())
        .output()
        .unwrap();

        assert!(
            output.status.success(),
            "{tool}: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        output
    };

    let ir = generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendIr);
    let mut sources = Vec::new();

    for (ordinal, bytes) in ir.iter().enumerate() {
        let text = String::from_utf8_lossy(bytes);

        if text.contains("/alternatename:") || text.contains("@live =") {
            let name = format!("source-{ordinal}.ll");

            fs::write(directory.path().join(&name), bytes).unwrap();
            sources.push(name.into());
        }
    }

    assert!(
        ir.iter()
            .any(|bytes| String::from_utf8_lossy(bytes).contains("/alternatename:")),
        "the fallback definition must be emitted",
    );

    assert!(
        ir.iter()
            .any(|bytes| String::from_utf8_lossy(bytes).contains("@live =")),
        "the ordinary storage definition must be emitted",
    );

    sources.extend(["-o".into(), "mixed.bc".into()]);
    run("llvm-link", sources);

    run(
        "opt",
        vec![
            "-passes=default<O2>".into(),
            "mixed.bc".into(),
            "-o".into(),
            "optimized.bc".into(),
        ],
    );

    run(
        "llc",
        vec![
            "-filetype=obj".into(),
            "optimized.bc".into(),
            "-o".into(),
            "mixed.obj".into(),
        ],
    );

    let first_ir = String::from_utf8_lossy(&ir[0]);

    let preamble = first_ir
        .lines()
        .filter(|line| line.starts_with("target "))
        .collect::<Vec<_>>()
        .join("\n");

    let symbol = role.native_symbol();

    let main = format!(
        r#"
{preamble}
declare void @{symbol}(ptr)
@live = external global i32
define i32 @main() {{
    %out = alloca [16 x i8], align 8
    call void @{symbol}(ptr %out)
    %category = load i32, ptr %out
    %live = load i32, ptr @live
    %result = add i32 %category, %live
    ret i32 %result
}}
"#
    );

    let strong = format!(
        r#"
{preamble}
define void @{symbol}(ptr %out) {{
    store i32 2, ptr %out
    ret void
}}
"#
    );

    fs::write(directory.path().join("main.ll"), main).unwrap();
    fs::write(directory.path().join("strong.ll"), strong).unwrap();

    fs::write(
        directory.path().join("unused.ll"),
        format!("{preamble}@unused = global [4096 x i8] zeroinitializer"),
    )
    .unwrap();

    for name in ["main", "strong", "unused"] {
        run(
            "llc",
            vec![
                "-filetype=obj".into(),
                format!("{name}.ll").into(),
                "-o".into(),
                format!("{name}.obj").into(),
            ],
        );
    }

    run(
        "llvm-ar",
        vec![
            "rc".into(),
            "strong.lib".into(),
            "strong.obj".into(),
            "unused.obj".into(),
        ],
    );

    for (kind, payload) in [
        (bray_native_artifact::NativeUnitKind::Object, "mixed.obj"),
        (
            bray_native_artifact::NativeUnitKind::Bitcode,
            "optimized.bc",
        ),
    ] {
        let bytes = fs::read(directory.path().join(payload)).unwrap();

        let summary = if kind == bray_native_artifact::NativeUnitKind::Object {
            bray_native_artifact::scan_object_unit_summary(&bytes).unwrap()
        } else {
            bray_codegen_llvm::inspect_bitcode_unit_summary(&bytes, NativeTarget::X86_64WindowsMsvc)
                .unwrap()
        };

        let bray_native_artifact::NativeUnitSummary::Exact {
            definitions, roots, ..
        } = &summary
        else {
            panic!("mixed {kind:?} must retain exact provider selection: {summary:?}");
        };

        assert!(roots.is_empty());

        assert!(
            definitions
                .iter()
                .any(
                    |definition| definition.symbol().identity().name() == Some("live")
                        && definition.selection()
                            == &bray_native_artifact::NativeDefinitionSelection::Ordinary
                )
        );

        assert!(
            definitions
                .iter()
                .any(|definition| definition.symbol().identity().name()
                    == Some(role.native_symbol())
                    && definition.selection()
                        == &bray_native_artifact::NativeDefinitionSelection::Fallback)
        );

        for provider in [None, Some("strong.obj"), Some("strong.lib")] {
            for reverse in [false, true] {
                let mut inputs = vec![std::ffi::OsString::from(payload)];

                if let Some(provider) = provider {
                    if reverse {
                        inputs.insert(0, provider.into());
                    } else {
                        inputs.push(provider.into());
                    }
                }

                let mut arguments = vec![
                    "/entry:main".into(),
                    "/subsystem:console".into(),
                    "/nodefaultlib".into(),
                    "/out:result.exe".into(),
                    "/map:result.map".into(),
                    "main.obj".into(),
                ];

                arguments.extend(inputs);
                run("lld-link", arguments);

                #[cfg(windows)]
                {
                    let result = std::process::Command::new(directory.path().join("result.exe"))
                        .status()
                        .unwrap();

                    assert_eq!(result.code(), Some(if provider.is_some() { 5 } else { 4 }));
                }

                let map = fs::read_to_string(directory.path().join("result.map")).unwrap();

                if provider.is_some() {
                    assert!(
                        map.lines().any(|line| line.contains(symbol)
                            && !line.contains("__bray_fallback.")
                            && line.contains("strong.obj")),
                        "the public platform symbol must resolve to the strong provider: {map}",
                    );
                } else {
                    assert!(
                        map.contains(&format!("__bray_fallback.{symbol}")),
                        "the default provider must remain available: {map}"
                    );
                }

                assert!(
                    !map.contains("unused"),
                    "unreferenced foreign members must remain lazy"
                );

                eprintln!(
                    "{kind:?} provider={provider:?} reverse={reverse} payload={} linked={} bytes",
                    bytes.len(),
                    fs::metadata(directory.path().join("result.exe"))
                        .unwrap()
                        .len()
                );
            }
        }
    }
}
