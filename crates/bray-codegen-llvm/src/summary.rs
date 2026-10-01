use std::collections::{BTreeMap, BTreeSet};

use bray_base::NonEmptySharedStr;
use bray_codegen::CodegenFailure;
use bray_native_artifact::{
    NativeComdatSelection, NativeDefinition, NativeDefinitionSelection, NativeRoot,
    NativeUnitSummary, alternate_name, scan_object_unit_summary, summarize_native_unit,
};
use bray_symbols::{
    NativeSymbolBinding, NativeSymbolContract, NativeSymbolIdentity, NativeSymbolPresence,
};
use bray_target::{NativeTarget, ObjectFormat};
use inkwell::comdat::ComdatSelectionKind;
use inkwell::context::Context;
use inkwell::module::{Linkage, Module};
use inkwell::values::BasicMetadataValueEnum;

/// Observes final LLVM bitcode bytes using the same native summary contract as object artifacts.
/// Unmodeled retention or selection semantics remain opaque with their known references.
pub fn inspect_bitcode_unit_summary(
    bytes: &[u8],
    target: NativeTarget,
) -> Result<NativeUnitSummary, CodegenFailure> {
    let context = Context::create();
    let module = crate::serialization::parse_bitcode(bytes, &context)?;

    module_summary(&module, target)
}

fn module_summary(
    module: &Module<'_>,
    target: NativeTarget,
) -> Result<NativeUnitSummary, CodegenFailure> {
    let mut definitions = BTreeSet::new();
    let mut references = BTreeSet::new();
    let mut roots = BTreeSet::new();

    // The safe LLVM API omits alias/ifunc/module-assembly inventories and COMDAT names.
    // Observe those properties once, without rebuilding symbol inventories from IR text.
    let structure = module.print_to_string();

    let structure = structure
        .to_str()
        .map_err(CodegenFailure::backend_library)?;

    let mut groups = BTreeMap::new();
    let mut exact = true;
    let mut module_assembly = false;
    let mut aliases_provided = BTreeSet::new();

    for line in structure.lines().map(str::trim) {
        if let Some((group, _)) = line
            .strip_prefix('$')
            .and_then(|s| s.split_once(" = comdat "))
        {
            if group.starts_with('"') {
                exact = false;
            } else {
                groups.insert(module.get_or_insert_comdat(group).as_mut_ptr(), group);
            }
        }

        if line.starts_with(';') || line.starts_with('!') || line.starts_with('^') {
            continue;
        }

        if line.starts_with('@')
            && line
                .split_whitespace()
                .any(|token| matches!(token, "alias" | "ifunc"))
        {
            if !line.contains(" = internal ") && !line.contains(" = private ") {
                if let Some(name) = alias_name(line) {
                    if let Some(name) = object_symbol(target, &name) {
                        aliases_provided.insert(NativeSymbolIdentity::Name(name));
                    }
                }
            }
        }

        module_assembly |= line.starts_with("module asm ");

        if line.starts_with("module asm ")
            || line.contains(" asm ")
            || line
                .split_whitespace()
                .any(|token| matches!(token, "alias" | "ifunc"))
        {
            exact = false;
        }
    }

    for value in module
        .get_globals()
        .chain(module.get_functions().map(|f| f.as_global_value()))
    {
        let name = value
            .get_name()
            .to_str()
            .map_err(CodegenFailure::backend_library)?;

        match name {
            "llvm.global_ctors" => {
                roots.insert(NativeRoot::Initialization);
                continue;
            }
            "llvm.global_dtors" => {
                roots.insert(NativeRoot::Finalization);
                continue;
            }
            _ if name.starts_with("llvm.") => {
                if !value.is_declaration() {
                    exact = false;
                }

                continue;
            }
            _ => {}
        }

        let Some(name) = object_symbol(target, name) else {
            exact = false;
            continue;
        };

        if value.get_section().is_some()
            || value.get_dll_storage_class() != inkwell::DLLStorageClass::Default
        {
            exact = false;
        }

        let linkage = value.get_linkage();

        if value.is_declaration() {
            let optional = linkage == Linkage::ExternalWeak;

            references.insert(NativeSymbolContract::new(
                NativeSymbolIdentity::Name(name),
                None,
                if optional {
                    NativeSymbolBinding::Weak
                } else {
                    NativeSymbolBinding::Strong
                },
                if optional {
                    NativeSymbolPresence::Optional
                } else {
                    NativeSymbolPresence::Required
                },
            ));

            exact &= !optional;
            continue;
        }

        if matches!(
            linkage,
            Linkage::Internal | Linkage::Private | Linkage::AvailableExternally
        ) {
            // An available_externally body is optimization-only, never an object definition.
            if linkage == Linkage::AvailableExternally {
                references.insert(NativeSymbolContract::required_name(name));
            }

            continue;
        }

        let weak = matches!(
            linkage,
            Linkage::WeakAny | Linkage::WeakODR | Linkage::LinkOnceODR
        );

        let selection = if let Some(comdat) = value.get_comdat() {
            match groups.get(&comdat.as_mut_ptr()) {
                Some(group) => NativeDefinitionSelection::Comdat {
                    group: NativeSymbolIdentity::Name(
                        NonEmptySharedStr::try_new(*group).expect("COMDAT must be named"),
                    ),
                    rule: match comdat.get_selection_kind() {
                        ComdatSelectionKind::Any => NativeComdatSelection::Any,
                        ComdatSelectionKind::ExactMatch => NativeComdatSelection::ExactMatch,
                        ComdatSelectionKind::Largest => NativeComdatSelection::Largest,
                        ComdatSelectionKind::NoDuplicates => NativeComdatSelection::NoDuplicates,
                        ComdatSelectionKind::SameSize => NativeComdatSelection::SameSize,
                    },
                    associative_with: None,
                },
                None => {
                    exact = false;

                    NativeDefinitionSelection::Ordinary
                }
            }
        } else {
            exact &= !weak;

            NativeDefinitionSelection::Ordinary
        };

        exact &= linkage == Linkage::External || weak;

        definitions.insert(NativeDefinition::new(
            NativeSymbolContract::new(
                NativeSymbolIdentity::Name(name),
                None,
                if weak {
                    NativeSymbolBinding::Weak
                } else {
                    NativeSymbolBinding::Strong
                },
                NativeSymbolPresence::Required,
            ),
            selection,
        ));
    }

    let mut aliases = Vec::new();

    for node in module.get_global_metadata("llvm.linker.options") {
        if target.object_format() != ObjectFormat::Coff {
            exact = false;
        }

        let option = node.get_node_values().and_then(|values| {
            let [BasicMetadataValueEnum::MetadataValue(value)] = values.as_slice() else {
                return None;
            };

            value
                .get_string_value()
                .and_then(|bytes| std::str::from_utf8(bytes).ok())
                .and_then(alternate_name)
        });

        match option {
            Some(alias) => aliases.push(alias),
            None => exact = false,
        }
    }

    if module.get_global_metadata_size("llvm.dependent-libraries") != 0 {
        exact = false;
    }

    let summary = if exact {
        summarize_native_unit(definitions, references, roots, aliases)
    } else {
        NativeUnitSummary::opaque_with_providers(
            definitions
                .iter()
                .map(|definition| definition.symbol().identity().clone())
                .chain(aliases_provided),
            references,
        )
    };

    if module_assembly {
        module_assembly_summary(module, target, &summary)
    } else {
        Ok(summary)
    }
}

fn module_assembly_summary(
    module: &Module<'_>,
    target: NativeTarget,
    summary: &NativeUnitSummary,
) -> Result<NativeUnitSummary, CodegenFailure> {
    // LLVM's assembler, rather than an assembly-text parser, owns physical symbol spelling.
    let machine = crate::machine::LlvmTargetMachine::create(
        &bray_codegen::CodegenTarget::for_native(target),
    )?;

    machine.configure_module(module);

    let bytes = machine.serialize(module, inkwell::targets::FileType::Object)?;

    let native = scan_object_unit_summary(&bytes).unwrap_or_else(|error| {
        panic!("LLVM must emit a valid object while observing module assembly: {error}")
    });

    Ok(NativeUnitSummary::opaque_archive(
        summary
            .defined_symbols()
            .chain(native.defined_symbols())
            .cloned(),
        summary
            .references()
            .iter()
            .chain(native.references())
            .cloned(),
    ))
}

fn object_symbol(target: NativeTarget, name: &str) -> Option<NonEmptySharedStr> {
    NonEmptySharedStr::try_new(
        name.strip_prefix('\u{1}')
            .map(std::borrow::Cow::Borrowed)
            .unwrap_or_else(|| target.object_symbol_name(name)),
    )
}

// The safe LLVM wrapper cannot enumerate alias/ifunc names. LLVM prints these declarations
// on one line with hexadecimal escapes inside quoted identifiers.
fn alias_name(line: &str) -> Option<std::borrow::Cow<'_, str>> {
    let name = line.strip_prefix('@')?;

    if let Some(quoted) = name.strip_prefix('"') {
        let quoted = quoted.split_once("\" =")?.0;
        let mut bytes = Vec::with_capacity(quoted.len());
        let mut input = quoted.bytes();

        while let Some(byte) = input.next() {
            if byte == b'\\' {
                let high = char::from(input.next()?).to_digit(16)?;
                let low = char::from(input.next()?).to_digit(16)?;

                bytes.push(u8::try_from(high * 16 + low).expect("two hex digits fit a byte"));
            } else {
                bytes.push(byte);
            }
        }

        String::from_utf8(bytes).ok().map(std::borrow::Cow::Owned)
    } else {
        name.split_once(" =")
            .map(|(name, _)| std::borrow::Cow::Borrowed(name))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::process::Command;

    use bray_native_artifact::{
        NativeComdatSelection, NativeDefinitionSelection, NativeRoot, NativeUnitSummary,
    };
    use bray_symbols::{NativeSymbolBinding, NativeSymbolPresence};
    use bray_target::NativeTarget;
    use bray_testing::TemporaryFile;
    use inkwell::context::Context;
    use inkwell::memory_buffer::MemoryBuffer;

    use super::inspect_bitcode_unit_summary;

    fn bitcode(ir: &str) -> Vec<u8> {
        let context = Context::create();
        let terminated = format!("{ir}\0");

        let buffer =
            MemoryBuffer::create_from_memory_range(&terminated.as_bytes(), "native fixture");

        let module = context.create_module_from_ir(buffer).unwrap();

        crate::serialization::bitcode_bytes(&module)
    }

    fn summary(ir: &str, target: NativeTarget) -> NativeUnitSummary {
        inspect_bitcode_unit_summary(&bitcode(ir), target).unwrap()
    }

    #[test]
    fn final_code_data_address_and_optional_references_match_llvm_symbols() {
        let ir = r#"
            @callback_table = global ptr @callback
            @imported = external thread_local global i64
            @optional_table = global ptr @optional
            declare void @callback()
            declare extern_weak void @optional()
            declare void @unused()
            define i64 @entry() {
                %value = load i64, ptr @imported
                ret i64 %value
            }
        "#;

        for target in [
            NativeTarget::X86_64WindowsMsvc,
            NativeTarget::X86_64LinuxGnu,
            NativeTarget::X86_64MacOs,
        ] {
            let mangling = if target.object_format() == bray_target::ObjectFormat::MachO {
                "o"
            } else {
                "e"
            };

            let ir = format!(
                "target triple = \"{}\"\ntarget datalayout = \"e-m:{mangling}-p:64:64\"\n{ir}",
                target.as_str()
            );

            let bytes = bitcode(&ir);
            let observed = inspect_bitcode_unit_summary(&bytes, target).unwrap();
            let file = TemporaryFile::write("native-summary.bc", &bytes);

            let output = Command::new(
                std::path::Path::new(
                    crate::COMPILED_LLVM_PREFIX.expect("test LLVM toolchain must be provisioned"),
                )
                .join("bin")
                .join(if cfg!(windows) {
                    "llvm-nm.exe"
                } else {
                    "llvm-nm"
                }),
            )
            .args(["--extern-only", "--format=posix"])
            .arg(file.path())
            .output()
            .unwrap();

            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );

            let symbols = String::from_utf8(output.stdout).unwrap();
            let mut expected_provided = BTreeSet::new();
            let mut expected_references = BTreeSet::new();

            for line in symbols.lines() {
                let mut fields = line.split_whitespace();
                let name = fields.next().unwrap();
                let class = fields.next().unwrap();

                if matches!(class, "U" | "w" | "v") {
                    expected_references.insert(name);
                } else {
                    expected_provided.insert(name);
                }
            }

            assert_eq!(
                observed
                    .defined_symbols()
                    .map(|s| s.name().unwrap())
                    .collect::<BTreeSet<_>>(),
                expected_provided
            );

            assert_eq!(
                observed
                    .references()
                    .iter()
                    .map(|s| s.identity().name().unwrap())
                    .collect::<BTreeSet<_>>(),
                expected_references
            );

            assert!(matches!(observed, NativeUnitSummary::Opaque { .. }));

            assert!(
                observed
                    .references()
                    .iter()
                    .any(|r| r.binding() == NativeSymbolBinding::Weak
                        && r.presence() == NativeSymbolPresence::Optional)
            );
        }
    }

    #[test]
    fn code_data_tls_and_comdat_selection_are_exact() {
        let ir = r#"
            $shared = comdat any
            $larger = comdat largest
            @storage = weak_odr thread_local global i64 0, comdat($shared)
            @table = global ptr @callback
            declare void @callback()
            define weak_odr i32 @provider() comdat($larger) { ret i32 1 }
            define i32 @entry() { ret i32 2 }
        "#;

        for target in [
            NativeTarget::X86_64WindowsMsvc,
            NativeTarget::X86_64LinuxGnu,
            NativeTarget::X86_64MacOs,
        ] {
            let observed = summary(ir, target);

            let NativeUnitSummary::Exact {
                definitions,
                references,
                roots,
            } = observed
            else {
                panic!("understood code/data/TLS/COMDAT must be exact");
            };

            assert_eq!(definitions.len(), 4);
            assert_eq!(references.len(), 1);
            assert!(roots.is_empty());

            let name = target.object_symbol_name("provider");

            let provider = definitions
                .iter()
                .find(|d| d.symbol().identity().name() == Some(name.as_ref()))
                .unwrap();

            assert_eq!(provider.symbol().binding(), NativeSymbolBinding::Weak);

            assert!(
                matches!(provider.selection(), NativeDefinitionSelection::Comdat { group, rule: NativeComdatSelection::Largest, associative_with: None } if group.name() == Some("larger"))
            );
        }
    }

    #[test]
    fn lifecycle_roots_and_llvm_intrinsics_preserve_exact_selection() {
        let observed = summary(
            r#"
            @llvm.global_ctors = appending global [1 x {i32, ptr, ptr}] [{i32, ptr, ptr} {i32 65535, ptr @initialize, ptr null}]
            @llvm.global_dtors = appending global [1 x {i32, ptr, ptr}] [{i32, ptr, ptr} {i32 65535, ptr @finalize, ptr null}]
            declare void @llvm.trap()
            define internal void @initialize() { ret void }
            define internal void @finalize() { ret void }
            define void @entry() { call void @llvm.trap() unreachable }
        "#,
            NativeTarget::X86_64LinuxGnu,
        );

        let NativeUnitSummary::Exact {
            definitions,
            references,
            roots,
        } = observed
        else {
            panic!("lifecycle arrays must remain exact");
        };

        assert_eq!(definitions.len(), 1);
        assert!(references.is_empty());

        assert_eq!(
            roots.as_ref(),
            [NativeRoot::Initialization, NativeRoot::Finalization]
        );
    }

    #[test]
    fn unmapped_selection_retention_and_assembly_are_opaque() {
        for ir in [
            "define weak i32 @entry() { ret i32 0 }",
            "define dllexport i32 @entry() { ret i32 0 }",
            "declare dllimport i32 @entry()",
            "define linkonce i32 @entry() { ret i32 0 }",
            "@storage = common global i32 0",
            "define void @entry() section \".custom\" { ret void }",
            "define void @entry() { ret void }\n@llvm.used = appending global [1 x ptr] [ptr @entry], section \"llvm.metadata\"",
            "@base = global i32 0\n@redirect = hidden alias i32, ptr @base",
            "define ptr @resolver() { ret ptr null }\n@entry = hidden ifunc void (), ptr @resolver",
            "module asm \".globl extra\"\ndefine void @entry() { ret void }",
            "define void @entry() { call void asm sideeffect \"\", \"\"() ret void }",
            "$\"quoted group\" = comdat any\ndefine weak_odr void @entry() comdat($\"quoted group\") { ret void }",
            "!llvm.dependent-libraries = !{!0}\n!0 = !{!\"foreign\"}",
        ] {
            assert!(
                matches!(
                    summary(ir, NativeTarget::X86_64LinuxGnu),
                    NativeUnitSummary::Opaque { .. }
                ),
                "{ir}"
            );
        }
    }

    #[test]
    fn deferred_provider_precedence_preserves_its_public_reference() {
        let ir = r#"
            $implementation = comdat any
            define weak i32 @implementation() comdat { ret i32 1 }
            @live = global i32 3
            @table = global ptr @provider
            declare i32 @provider()
            !llvm.linker.options = !{!0}
            !0 = !{!"/alternatename:provider=implementation"}
        "#;

        let observed = summary(ir, NativeTarget::X86_64WindowsMsvc);

        let NativeUnitSummary::Exact {
            definitions,
            references,
            roots,
        } = observed
        else {
            panic!("local deferred provider must be exact");
        };

        assert!(roots.is_empty());

        let exact = NativeUnitSummary::Exact {
            definitions: definitions.clone(),
            references: references.clone(),
            roots: roots.clone(),
        };

        let archive = NativeUnitSummary::opaque_archive(
            exact.defined_symbols().cloned(),
            exact.references().iter().cloned(),
        );

        assert!(
            archive
                .references()
                .iter()
                .any(|reference| reference.identity().name() == Some("provider"))
        );

        assert_eq!(references[0].identity().name(), Some("provider"));

        assert!(
            definitions
                .iter()
                .any(|d| d.symbol().identity().name() == Some("provider")
                    && d.selection() == &NativeDefinitionSelection::Fallback)
        );

        assert!(matches!(
            summary(ir, NativeTarget::X86_64LinuxGnu),
            NativeUnitSummary::Opaque { .. }
        ));

        for option in [
            "/include:provider",
            "/alternatename:provider=foreign",
            "/alternatename:provider=redirect",
        ] {
            let ir = ir.replace("/alternatename:provider=implementation", option);

            assert!(matches!(
                summary(&ir, NativeTarget::X86_64WindowsMsvc),
                NativeUnitSummary::Opaque { .. }
            ));
        }

        let conflict = ir.replace(
            "!llvm.linker.options = !{!0}",
            "!llvm.linker.options = !{!0, !1}",
        ) + "\n!1 = !{!\"/alternatename:provider=live\"}";

        assert!(matches!(
            summary(&conflict, NativeTarget::X86_64WindowsMsvc),
            NativeUnitSummary::Opaque { .. }
        ));
    }

    #[test]
    fn quoted_and_explicitly_unmangled_symbols_keep_target_spelling() {
        let observed = summary(
            r#"
            define void @"a\20b"() { ret void }
            define void @"\01unmangled"() { ret void }
        "#,
            NativeTarget::X86_64MacOs,
        );

        assert_eq!(
            observed
                .defined_symbols()
                .map(|s| s.name().unwrap())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["_a b", "unmangled"])
        );
    }

    #[test]
    fn opaque_alias_inventory_preserves_physical_definitions_for_archive_assembly() {
        let observed = summary(
            r#"
            @base = global i32 0
            @"redirect\20value" = hidden alias i32, ptr @base
            @private_alias = private alias i32, ptr @base
            @table = global ptr @outside
            declare void @outside()
        "#,
            NativeTarget::X86_64MacOs,
        );

        assert!(matches!(observed, NativeUnitSummary::Opaque { .. }));

        assert_eq!(
            observed
                .defined_symbols()
                .map(|symbol| symbol.name().unwrap())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["_base", "_redirect value", "_table"])
        );

        let archive = NativeUnitSummary::opaque_archive(
            observed.defined_symbols().cloned(),
            observed.references().iter().cloned(),
        );

        assert_eq!(archive.references()[0].identity().name(), Some("_outside"));
    }

    #[test]
    fn module_assembly_closes_over_an_exact_provider_in_another_package() {
        use bray_native_artifact::{
            NativeArtifactIndex, NativeContentDigest, NativeUnit, NativeUnitKind,
            NativeUnitResolver,
        };

        for target in [
            NativeTarget::X86_64WindowsMsvc,
            NativeTarget::X86_64LinuxGnu,
            NativeTarget::X86_64MacOs,
        ] {
            let entry = target.object_symbol_name("asm_entry");
            let dependency = target.object_symbol_name("asm_dependency");

            let ir = format!(
                r#"
                target triple = "{}"
                module asm ".globl {entry}"
                module asm "{entry}:"
                module asm "jmp {dependency}"
                declare void @asm_entry()
                declare void @asm_dependency()
                define void @entry() {{
                    call void @asm_entry()
                    call void @asm_dependency()
                    ret void
                }}
                "#,
                target.as_str(),
            );

            let observed = summary(&ir, target);

            assert!(matches!(observed, NativeUnitSummary::Opaque { .. }));

            assert!(
                observed
                    .defined_symbols()
                    .any(|symbol| symbol.name() == Some(&entry))
            );

            assert_eq!(
                observed
                    .references()
                    .iter()
                    .map(|symbol| symbol.identity().name().unwrap())
                    .collect::<BTreeSet<_>>(),
                BTreeSet::from([dependency.as_ref()]),
            );

            let package = |id, observed| {
                let digest = NativeContentDigest::new([id; 32]);

                NativeArtifactIndex::try_new(
                    target,
                    digest,
                    [NativeUnit::new(
                        digest,
                        NativeUnitKind::Bitcode,
                        observed,
                        [],
                    )],
                    [],
                )
                .unwrap()
                .with_bitcode_toolchain("test-llvm")
            };

            let resolver = NativeUnitResolver::new([
                package(1, observed),
                package(
                    2,
                    summary("define void @asm_dependency() { ret void }", target),
                ),
                package(3, summary("define void @unused() { ret void }", target)),
            ]);

            let selected = resolver.select([]).unwrap();

            assert_eq!(
                selected
                    .units()
                    .iter()
                    .map(|unit| unit.artifact)
                    .collect::<BTreeSet<_>>(),
                BTreeSet::from([0, 1]),
            );
        }
    }

    #[test]
    fn invalid_external_bitcode_returns_the_llvm_cause() {
        assert!(
            matches!(inspect_bitcode_unit_summary(b"invalid", NativeTarget::X86_64LinuxGnu), Err(bray_codegen::CodegenFailure::BackendLibrary { report }) if !report.is_empty())
        );
    }
}
