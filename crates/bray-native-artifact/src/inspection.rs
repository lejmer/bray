use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use bray_base::NonEmptySharedStr;
use bray_symbols::{
    NativeSymbolBinding, NativeSymbolContract, NativeSymbolIdentity, NativeSymbolPresence,
};
use object::{
    ComdatKind, Object, ObjectComdat, ObjectSection, ObjectSymbol, SectionFlags, SectionKind,
    SymbolKind, SymbolSection,
};

use crate::{
    NativeComdatSelection, NativeDefinition, NativeDefinitionSelection, NativeRoot,
    NativeUnitSummary,
};

/// Summarizes an LLVM-inspected bitcode unit. Unknown selection or retention semantics
/// make the whole unit opaque instead of producing an incomplete exact graph.
pub fn scan_bitcode_unit_summary(
    symbols: &str,
    structure: &str,
    target: bray_target::NativeTarget,
) -> NativeUnitSummary {
    let mut definitions = BTreeSet::new();
    let references = scan_symbol_references(symbols);
    let mut weak_comdats = BTreeSet::new();

    for line in symbols
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if line.ends_with(':') {
            continue;
        }

        let mut fields = line.split_whitespace();

        let (Some(name), Some(class)) = (fields.next(), fields.next()) else {
            return NativeUnitSummary::opaque(references);
        };

        let Some(name) = NonEmptySharedStr::try_new(name) else {
            return NativeUnitSummary::opaque(references);
        };

        let symbol = NativeSymbolContract::required_name(name.clone());

        match class {
            "U" => {}
            "T" | "D" | "B" | "R" | "S" | "G" => {
                definitions.insert(NativeDefinition::new(
                    symbol,
                    NativeDefinitionSelection::Ordinary,
                ));
            }
            "W" => {
                let Some(ir_name) = target.codegen_symbol_name(name.as_str()) else {
                    return NativeUnitSummary::opaque(references);
                };

                let Some(selection) = weak_comdat_selection(ir_name, structure) else {
                    return NativeUnitSummary::opaque(references);
                };

                weak_comdats.insert(ir_name.to_owned());

                definitions.insert(NativeDefinition::new(
                    NativeSymbolContract::new(
                        NativeSymbolIdentity::Name(name),
                        None,
                        NativeSymbolBinding::Weak,
                        NativeSymbolPresence::Required,
                    ),
                    selection,
                ));
            }
            _ => return NativeUnitSummary::opaque(references),
        }
    }

    let Some(roots) = bitcode_roots(structure, &weak_comdats) else {
        return NativeUnitSummary::opaque(references);
    };

    exact_summary(definitions, references, roots)
}

fn weak_comdat_selection(name: &str, ir: &str) -> Option<NativeDefinitionSelection> {
    let function = format!("@{name}(");
    let global = format!("@{name} =");

    let definition = ir.lines().find(|line| {
        (line.starts_with("define ") && line.contains(&function))
            || (line.starts_with('@') && line.starts_with(&global))
    })?;

    if !definition.contains(" weak_odr ") && !definition.contains(" linkonce_odr ") {
        return None;
    }

    let group = if let Some((_, group)) = definition.split_once(" comdat($") {
        group.split_once(')')?.0
    } else if definition.contains(" comdat") {
        name
    } else {
        return None;
    };

    let declaration = format!("${group} = comdat ");

    let rule = ir
        .lines()
        .find_map(|line| line.strip_prefix(&declaration))?;

    let rule = match rule {
        "any" => NativeComdatSelection::Any,
        "exactmatch" => NativeComdatSelection::ExactMatch,
        "samesize" => NativeComdatSelection::SameSize,
        "largest" => NativeComdatSelection::Largest,
        "noduplicates" => NativeComdatSelection::NoDuplicates,
        _ => return None,
    };

    Some(NativeDefinitionSelection::Comdat {
        group: NativeSymbolIdentity::Name(NonEmptySharedStr::try_new(group)?),
        rule,
        associative_with: None,
    })
}

/// Reads a compiler-produced object in process, retaining exact selection only for
/// ordinary external symbols and sections with understood lifecycle behavior.
pub fn scan_object_unit_summary(bytes: &[u8]) -> Result<NativeUnitSummary, object::Error> {
    let file = object::File::parse(bytes)?;

    let (provided, mut references) = object_symbols(&file);

    references.retain(|reference| !provided.contains(reference.identity()));

    let Some(comdats) = object_comdats(&file) else {
        return Ok(NativeUnitSummary::opaque(references));
    };

    if file.kind() != object::ObjectKind::Relocatable {
        return Ok(NativeUnitSummary::opaque(references));
    }

    let mut roots = BTreeSet::new();

    for section in file.sections() {
        let Ok(name) = section.name() else {
            return Ok(NativeUnitSummary::opaque(references));
        };

        if matches!(section.kind(), SectionKind::Linker)
            || match section.flags() {
                SectionFlags::Coff { characteristics } => {
                    characteristics & object::pe::IMAGE_SCN_LNK_COMDAT != 0
                        && !comdats.contains_key(&section.index())
                }
                SectionFlags::Elf { sh_flags } => {
                    sh_flags & u64::from(object::elf::SHF_GROUP) != 0
                        && !comdats.contains_key(&section.index())
                }
                SectionFlags::MachO { flags } => {
                    flags
                        & (object::macho::S_ATTR_NO_DEAD_STRIP | object::macho::S_ATTR_LIVE_SUPPORT)
                        != 0
                }
                SectionFlags::None => false,
                _ => true,
            }
        {
            return Ok(NativeUnitSummary::opaque(references));
        }

        let lower = name.to_ascii_lowercase();

        if lower == ".drectve" {
            return Ok(NativeUnitSummary::opaque(references));
        }

        if lower == ".init"
            || lower.contains(".init_array")
            || lower.contains(".preinit_array")
            || lower.contains(".ctors")
            || lower.contains("__mod_init_func")
            || lower.contains(".crt$x") && !lower.contains(".crt$xt")
        {
            roots.insert(NativeRoot::Initialization);
        }

        if lower == ".fini"
            || lower.contains(".fini_array")
            || lower.contains(".dtors")
            || lower.contains("__mod_term_func")
            || lower.contains(".crt$xt")
        {
            roots.insert(NativeRoot::Finalization);
        }
    }

    let mut definitions = BTreeSet::new();

    for symbol in file.symbols() {
        if !symbol.is_global() && !symbol.is_weak() {
            continue;
        }

        let undefined = symbol.is_undefined();

        if symbol.is_weak() && undefined
            || !undefined && matches!(symbol.kind(), SymbolKind::Unknown)
            || !matches!(
                symbol.section(),
                SymbolSection::Undefined | SymbolSection::Section(_)
            )
            || matches!(symbol.flags(), object::SymbolFlags::CoffSection { selection, .. } if selection != 0)
        {
            return Ok(NativeUnitSummary::opaque(references));
        }

        let Ok(name) = symbol.name() else {
            return Ok(NativeUnitSummary::opaque(references));
        };

        let Some(name) = NonEmptySharedStr::try_new(name) else {
            return Ok(NativeUnitSummary::opaque(references));
        };

        if !undefined {
            // COMDAT identities are small shared strings. Definitions own their selection contract.
            let selection = symbol
                .section_index()
                .and_then(|section| comdats.get(&section))
                .cloned()
                .unwrap_or(NativeDefinitionSelection::Ordinary);

            if matches!(&selection, NativeDefinitionSelection::Comdat { associative_with: Some(parent), .. } if !provided.contains(parent))
            {
                return Ok(NativeUnitSummary::opaque(references));
            }

            let contract = NativeSymbolContract::new(
                NativeSymbolIdentity::Name(name),
                None,
                if symbol.is_weak() {
                    NativeSymbolBinding::Weak
                } else {
                    NativeSymbolBinding::Strong
                },
                NativeSymbolPresence::Required,
            );

            definitions.insert(NativeDefinition::new(contract, selection));
        }
    }

    Ok(exact_summary(definitions, references, roots))
}

fn object_comdats(
    file: &object::File<'_>,
) -> Option<HashMap<object::SectionIndex, NativeDefinitionSelection>> {
    let mut sections = HashMap::new();

    for comdat in file.comdats() {
        let rule = match comdat.kind() {
            ComdatKind::Any => NativeComdatSelection::Any,
            ComdatKind::NoDuplicates => NativeComdatSelection::NoDuplicates,
            ComdatKind::SameSize => NativeComdatSelection::SameSize,
            ComdatKind::ExactMatch => NativeComdatSelection::ExactMatch,
            ComdatKind::Largest => NativeComdatSelection::Largest,
            _ => return None,
        };

        let group = NativeSymbolIdentity::Name(NonEmptySharedStr::try_new(comdat.name().ok()?)?);

        let primary = file
            .symbol_by_index(comdat.symbol())
            .ok()?
            .section_index()?;

        for section in comdat.sections() {
            // Section contracts share the immutable group identity.
            sections.insert(
                section,
                NativeDefinitionSelection::Comdat {
                    group: group.clone(),
                    rule,
                    associative_with: (file.format() == object::BinaryFormat::Coff
                        && section != primary)
                        .then(|| group.clone()),
                },
            );
        }
    }

    Some(sections)
}

/// Keeps known external references of a native object archive without claiming exact selection.
pub fn scan_object_archive_summary(bytes: &[u8]) -> Result<NativeUnitSummary, object::Error> {
    let archive = object::read::archive::ArchiveFile::parse(bytes)?;
    let mut provided = BTreeSet::new();
    let mut references = BTreeSet::new();

    for member in archive.members() {
        let data = member?.data(bytes)?;

        // Native archives may also contain short import records or unsupported members.
        if let Ok(file) = object::File::parse(data) {
            let (definitions, imports) = object_symbols(&file);

            provided.extend(definitions);
            references.extend(imports);
        }
    }

    references.retain(|reference| !provided.contains(reference.identity()));

    Ok(NativeUnitSummary::opaque(references))
}

fn object_symbols(
    file: &object::File<'_>,
) -> (
    BTreeSet<NativeSymbolIdentity>,
    BTreeSet<NativeSymbolContract>,
) {
    let mut provided = BTreeSet::new();
    let mut references = BTreeSet::new();

    for symbol in file
        .symbols()
        .filter(|symbol| symbol.is_global() || symbol.is_weak())
    {
        let Some(name) = symbol.name().ok().and_then(NonEmptySharedStr::try_new) else {
            continue;
        };

        let identity = NativeSymbolIdentity::Name(name);

        if symbol.is_undefined() {
            let (binding, presence) = if symbol.is_weak() {
                (NativeSymbolBinding::Weak, NativeSymbolPresence::Optional)
            } else {
                (NativeSymbolBinding::Strong, NativeSymbolPresence::Required)
            };

            references.insert(NativeSymbolContract::new(identity, None, binding, presence));
        } else {
            provided.insert(identity);
        }
    }

    (provided, references)
}

/// Keeps known external references from one LLVM symbol inventory, including archives.
pub fn scan_symbol_references(symbols: &str) -> BTreeSet<NativeSymbolContract> {
    let mut provided = BTreeSet::new();
    let mut references = BTreeSet::new();

    for line in symbols.lines() {
        let mut fields = line.split_whitespace();

        let (Some(name), Some(class)) = (fields.next(), fields.next()) else {
            continue;
        };

        let Some(name) = NonEmptySharedStr::try_new(name) else {
            continue;
        };

        let identity = NativeSymbolIdentity::Name(name);

        match class {
            "U" => {
                references.insert(NativeSymbolContract::new(
                    identity,
                    None,
                    NativeSymbolBinding::Strong,
                    NativeSymbolPresence::Required,
                ));
            }
            "w" | "v" => {
                references.insert(NativeSymbolContract::new(
                    identity,
                    None,
                    NativeSymbolBinding::Weak,
                    NativeSymbolPresence::Optional,
                ));
            }
            "T" | "D" | "B" | "R" | "S" | "G" | "W" | "V" | "I" | "i" | "u" => {
                provided.insert(identity);
            }
            _ => {}
        }
    }

    references.retain(|reference| !provided.contains(reference.identity()));

    references
}

fn exact_summary(
    definitions: BTreeSet<NativeDefinition>,
    references: BTreeSet<NativeSymbolContract>,
    roots: BTreeSet<NativeRoot>,
) -> NativeUnitSummary {
    if definitions.iter().any(|definition| {
        references.iter().any(|reference| {
            reference.identity() == definition.symbol().identity()
                && reference.presence() == NativeSymbolPresence::Required
        })
    }) {
        // A definition and undefined reference with one spelling need target-specific
        // resolution rules that this exact summary does not model.
        return NativeUnitSummary::opaque(references);
    }

    NativeUnitSummary::Exact {
        definitions: Arc::from(definitions.into_iter().collect::<Vec<_>>()),
        references: Arc::from(references.into_iter().collect::<Vec<_>>()),
        roots: Arc::from(roots.into_iter().collect::<Vec<_>>()),
    }
}

fn bitcode_roots(ir: &str, weak_comdats: &BTreeSet<String>) -> Option<BTreeSet<NativeRoot>> {
    let mut roots = BTreeSet::new();

    for line in ir.lines().map(str::trim) {
        if line.starts_with(';') || line.starts_with('^') {
            continue;
        }

        if line.starts_with("@llvm.global_ctors =") {
            roots.insert(NativeRoot::Initialization);
            continue;
        }

        if line.starts_with("@llvm.global_dtors =") {
            roots.insert(NativeRoot::Finalization);
            continue;
        }

        if line.starts_with('$') {
            if !line.contains(" = comdat ") {
                return None;
            }

            continue;
        }

        if line.contains(" comdat")
            || line.contains(" weak_odr ")
            || line.contains(" linkonce_odr ")
        {
            let known = weak_comdats.iter().any(|name| {
                (line.starts_with("define ") && line.contains(&format!("@{name}(")))
                    || line.starts_with(&format!("@{name} ="))
            });

            if !known {
                return None;
            }
        }

        if line.starts_with("@llvm.")
            || line
                .split_whitespace()
                .any(|token| matches!(token, "alias" | "ifunc"))
            || line.contains(" extern_weak ")
            || line.contains(" weak ")
            || line.contains(" linkonce ")
            || line.contains(" section ")
            || line.contains(" asm ")
            || line.contains(" appending global ")
            || line.contains("llvm.linker.options")
            || line.contains("llvm.dependent-libraries")
        {
            return None;
        }
    }

    Some(roots)
}

#[cfg(test)]
mod tests {
    use object::write::{Object, StandardSection, Symbol, SymbolSection};
    use object::{
        Architecture, BinaryFormat, Endianness, SectionKind, SymbolFlags, SymbolKind, SymbolScope,
    };

    use super::{scan_bitcode_unit_summary, scan_object_unit_summary};
    use crate::{NativeComdatSelection, NativeDefinitionSelection, NativeRoot, NativeUnitSummary};
    use bray_symbols::NativeSymbolBinding;

    #[test]
    fn code_data_and_address_references_are_exact() {
        let symbols = "entry T 0 0\ncallback_table D 0 0\ncallback U\n";

        let summary = scan_bitcode_unit_summary(
            symbols,
            "define @entry {}",
            bray_target::NativeTarget::X86_64LinuxGnu,
        );

        let NativeUnitSummary::Exact {
            definitions,
            references,
            roots,
        } = summary
        else {
            panic!("ordinary bitcode must have an exact summary");
        };

        assert_eq!(definitions.len(), 2);
        assert_eq!(references.len(), 1);
        assert!(roots.is_empty());
    }

    #[test]
    fn weak_comdat_bitcode_is_selected_by_symbol() {
        let ir = "$provider = comdat exactmatch\ndefine weak_odr void @provider() comdat {\n  ret void\n}\n";

        let summary = scan_bitcode_unit_summary(
            "provider W 0 0\n",
            ir,
            bray_target::NativeTarget::X86_64LinuxGnu,
        );

        let NativeUnitSummary::Exact {
            definitions,
            references,
            roots,
        } = summary
        else {
            panic!("supported weak COMDAT must have an exact summary");
        };

        assert_eq!(definitions.len(), 1);
        assert_eq!(definitions[0].symbol().binding(), NativeSymbolBinding::Weak);

        assert!(matches!(
            definitions[0].selection(),
            NativeDefinitionSelection::Comdat {
                rule: NativeComdatSelection::ExactMatch,
                associative_with: None,
                ..
            },
        ));

        assert!(references.is_empty());
        assert!(roots.is_empty());

        assert!(matches!(
            scan_bitcode_unit_summary(
                "provider W 0 0\n",
                "$provider = comdat exactmatch\ndefine weak_odr void @provider() section \".custom\" comdat {\n  ret void\n}\n",
                bray_target::NativeTarget::X86_64LinuxGnu
            ),
            NativeUnitSummary::Opaque { .. },
        ));
    }

    #[test]
    fn thread_local_bitcode_uses_symbol_demand_without_a_lifecycle_root() {
        let ir = "$state = comdat any\n@state = weak_odr thread_local global i64 0, comdat\n@imported = external thread_local global i64\n";

        let NativeUnitSummary::Exact {
            definitions,
            references,
            roots,
        } = scan_bitcode_unit_summary(
            "state W 0 0\nimported U\n",
            ir,
            bray_target::NativeTarget::X86_64LinuxGnu,
        )
        else {
            panic!("thread-local allocation alone must not require conservative retention");
        };

        assert_eq!(definitions.len(), 1);
        assert_eq!(definitions[0].symbol().identity().name(), Some("state"));
        assert_eq!(references.len(), 1);
        assert_eq!(references[0].identity().name(), Some("imported"));
        assert!(roots.is_empty());
    }

    #[test]
    fn decorated_bitcode_symbols_use_llvm_names_for_comdat_lookup() {
        let ir =
            "$provider = comdat any\ndefine weak_odr void @provider() comdat {\n ret void\n}\n";

        let summary = scan_bitcode_unit_summary(
            "_provider W 0 0\n",
            ir,
            bray_target::NativeTarget::X86_64MacOs,
        );

        let NativeUnitSummary::Exact { definitions, .. } = summary else {
            panic!("decorated symbol must retain exact COMDAT selection")
        };

        assert_eq!(definitions[0].symbol().identity().name(), Some("_provider"));
    }

    #[test]
    fn object_symbols_and_lifecycle_sections_are_scanned_without_tools() {
        for (format, section) in [
            (BinaryFormat::Coff, ".CRT$XCU"),
            (BinaryFormat::Elf, ".init_array"),
        ] {
            let bytes = object_fixture(format, section);
            let summary = scan_object_unit_summary(&bytes).unwrap();

            assert!(
                matches!(summary, NativeUnitSummary::Exact { definitions, references, roots }
                if definitions.len() == 1 && references.len() == 1
                && roots.as_ref() == [NativeRoot::Initialization])
            );
        }

        let opaque = object_fixture(BinaryFormat::Coff, ".drectve");

        let summary = scan_object_unit_summary(&opaque).unwrap();

        assert!(matches!(summary, NativeUnitSummary::Opaque { .. }));
        assert_eq!(summary.references().len(), 1);
        assert_eq!(summary.references()[0].identity().name(), Some("callback"));
    }

    #[test]
    fn archive_symbols_keep_only_external_dependencies_and_optional_presence() {
        let references = super::scan_symbol_references(
            "one.o:\nlocal U\nrequired U\noptional w\ntwo.o:\nlocal T 0 4\n",
        );

        assert_eq!(references.len(), 2);

        assert!(
            references
                .iter()
                .any(|reference| reference.identity().name() == Some("required")
                    && reference.presence() == bray_symbols::NativeSymbolPresence::Required)
        );

        assert!(
            references
                .iter()
                .any(|reference| reference.identity().name() == Some("optional")
                    && reference.presence() == bray_symbols::NativeSymbolPresence::Optional)
        );
    }

    #[test]
    fn qualified_aliases_and_ifuncs_are_opaque() {
        for declaration in [
            "@alternate = dso_local alias i32, ptr @value",
            "@alternate = hidden unnamed_addr alias i32, ptr @value",
            "@dispatch = dso_local ifunc void (), ptr @resolver",
        ] {
            assert!(matches!(
                scan_bitcode_unit_summary(
                    "entry T 0 0",
                    declaration,
                    bray_target::NativeTarget::X86_64LinuxGnu
                ),
                NativeUnitSummary::Opaque { .. },
            ));
        }
    }

    #[test]
    fn initialization_is_a_root_and_unsupported_selection_is_opaque() {
        let summary = scan_bitcode_unit_summary(
            "entry T 0 0\n",
            "@llvm.global_ctors = appending global []\n",
            bray_target::NativeTarget::X86_64LinuxGnu,
        );

        assert!(
            matches!(summary, NativeUnitSummary::Exact { roots, .. } if roots.as_ref() == [NativeRoot::Initialization])
        );

        assert!(matches!(
            scan_bitcode_unit_summary("entry W 0 0", "", bray_target::NativeTarget::X86_64LinuxGnu),
            NativeUnitSummary::Opaque { .. }
        ));
    }

    #[test]
    fn object_comdat_rules_and_associated_definitions_use_the_shared_selection_contract() {
        for (kind, rule) in [
            (object::ComdatKind::Any, NativeComdatSelection::Any),
            (
                object::ComdatKind::SameSize,
                NativeComdatSelection::SameSize,
            ),
            (
                object::ComdatKind::ExactMatch,
                NativeComdatSelection::ExactMatch,
            ),
            (object::ComdatKind::Largest, NativeComdatSelection::Largest),
            (
                object::ComdatKind::NoDuplicates,
                NativeComdatSelection::NoDuplicates,
            ),
        ] {
            let mut object =
                Object::new(BinaryFormat::Coff, Architecture::X86_64, Endianness::Little);

            let text = object.section_id(StandardSection::Text);
            object.append_section_data(text, &[0xc3], 1);
            object.section_symbol(text);

            let entry = object.add_symbol(Symbol {
                name: b"entry".to_vec(),
                value: 0,
                size: 1,
                kind: SymbolKind::Text,
                scope: SymbolScope::Linkage,
                weak: false,
                section: SymbolSection::Section(text),
                flags: SymbolFlags::None,
            });

            let data = object.section_id(StandardSection::Data);
            object.append_section_data(data, &[0; 8], 8);
            object.section_symbol(data);

            object.add_symbol(Symbol {
                name: b"associated".to_vec(),
                value: 0,
                size: 8,
                kind: SymbolKind::Data,
                scope: SymbolScope::Linkage,
                weak: false,
                section: SymbolSection::Section(data),
                flags: SymbolFlags::None,
            });

            object.add_comdat(object::write::Comdat {
                kind,
                symbol: entry,
                sections: vec![text, data],
            });

            let summary = scan_object_unit_summary(&object.write().unwrap()).unwrap();

            let NativeUnitSummary::Exact {
                definitions, roots, ..
            } = summary
            else {
                panic!("supported object COMDAT must be exact")
            };

            assert!(roots.is_empty());
            assert_eq!(definitions.len(), 2);

            for definition in definitions.iter() {
                let NativeDefinitionSelection::Comdat {
                    rule: actual,
                    associative_with,
                    ..
                } = definition.selection()
                else {
                    panic!("COMDAT selection must survive object inspection")
                };

                assert_eq!(*actual, rule);

                assert_eq!(
                    associative_with.as_ref().and_then(|symbol| symbol.name()),
                    (definition.symbol().identity().name() == Some("associated"))
                        .then_some("entry")
                );
            }
        }
    }

    #[test]
    fn ordinary_thread_local_object_storage_has_no_lifecycle_root() {
        for format in [BinaryFormat::Coff, BinaryFormat::Elf] {
            let mut object = Object::new(format, Architecture::X86_64, Endianness::Little);
            let data = object.section_id(StandardSection::Tls);
            object.append_section_data(data, &[0; 8], 8);

            object.add_symbol(Symbol {
                name: b"thread_storage".to_vec(),
                value: 0,
                size: 8,
                kind: SymbolKind::Tls,
                scope: SymbolScope::Linkage,
                weak: false,
                section: SymbolSection::Section(data),
                flags: SymbolFlags::None,
            });

            let summary = scan_object_unit_summary(&object.write().unwrap()).unwrap();

            let NativeUnitSummary::Exact {
                definitions, roots, ..
            } = summary
            else {
                panic!("ordinary TLS storage must be exact")
            };

            assert_eq!(definitions.len(), 1);
            assert!(roots.is_empty());
        }
    }

    fn object_fixture(format: BinaryFormat, lifecycle_section: &str) -> Vec<u8> {
        let mut object = Object::new(format, Architecture::X86_64, Endianness::Little);
        let text = object.section_id(StandardSection::Text);
        object.append_section_data(text, &[0xc3], 1);

        object.add_symbol(Symbol {
            name: b"entry".to_vec(),
            value: 0,
            size: 1,
            kind: SymbolKind::Text,
            scope: SymbolScope::Linkage,
            weak: false,
            section: SymbolSection::Section(text),
            flags: SymbolFlags::None,
        });

        object.add_symbol(Symbol {
            name: b"callback".to_vec(),
            value: 0,
            size: 0,
            kind: SymbolKind::Text,
            scope: SymbolScope::Linkage,
            weak: false,
            section: SymbolSection::Undefined,
            flags: SymbolFlags::None,
        });

        let lifecycle = object.add_section(
            Vec::new(),
            lifecycle_section.as_bytes().to_vec(),
            SectionKind::Data,
        );

        object.append_section_data(lifecycle, &[0; 8], 8);

        object.write().unwrap()
    }
}
