use std::collections::BTreeSet;
use std::sync::Arc;

use bray_base::NonEmptySharedStr;
use bray_symbols::{NativeSymbolContract, NativeSymbolPresence};
use object::{Object, ObjectSection, ObjectSymbol, SectionFlags, SectionKind, SymbolKind, SymbolSection};

use crate::{NativeDefinition, NativeDefinitionSelection, NativeRoot, NativeUnitSummary};

/// Summarizes an LLVM-inspected bitcode unit. Unknown selection or retention semantics
/// make the whole unit opaque instead of producing an incomplete exact graph.
pub fn scan_bitcode_unit_summary(symbols: &str, structure: &str) -> NativeUnitSummary {
    let mut definitions = BTreeSet::new();
    let mut references = BTreeSet::new();

    for line in symbols.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if line.ends_with(':') {
            continue;
        }

        let mut fields = line.split_whitespace();

        let (Some(name), Some(class)) = (fields.next(), fields.next()) else {
            return NativeUnitSummary::Opaque;
        };

        let Some(name) = NonEmptySharedStr::try_new(name) else {
            return NativeUnitSummary::Opaque;
        };

        let symbol = NativeSymbolContract::required_name(name);

        match class {
            "U" => {
                references.insert(symbol);
            }
            "T" | "D" | "B" | "R" | "S" | "G" => {
                definitions.insert(NativeDefinition::new(
                    symbol,
                    NativeDefinitionSelection::Ordinary,
                ));
            }
            _ => return NativeUnitSummary::Opaque,
        }
    }

    let Some(roots) = bitcode_roots(structure) else {
        return NativeUnitSummary::Opaque;
    };

    exact_summary(definitions, references, roots)
}

/// Reads a compiler-produced object in process, retaining exact selection only for
/// ordinary external symbols and sections with understood lifecycle behavior.
pub fn scan_object_unit_summary(bytes: &[u8]) -> Result<NativeUnitSummary, object::Error> {
    let file = object::File::parse(bytes)?;

    if file.kind() != object::ObjectKind::Relocatable || file.comdats().next().is_some() {
        return Ok(NativeUnitSummary::Opaque);
    }

    let mut roots = BTreeSet::new();

    for section in file.sections() {
        let Ok(name) = section.name() else {
            return Ok(NativeUnitSummary::Opaque);
        };

        if matches!(section.kind(), SectionKind::Tls | SectionKind::UninitializedTls | SectionKind::TlsVariables | SectionKind::Linker)
            || match section.flags() {
                SectionFlags::Coff { characteristics } => {
                    characteristics & object::pe::IMAGE_SCN_LNK_COMDAT != 0
                }
                SectionFlags::Elf { sh_flags } => {
                    sh_flags & u64::from(object::elf::SHF_GROUP | object::elf::SHF_TLS) != 0
                }
                SectionFlags::MachO { flags } => {
                    flags & (object::macho::S_ATTR_NO_DEAD_STRIP | object::macho::S_ATTR_LIVE_SUPPORT) != 0
                }
                SectionFlags::None => false,
                _ => true,
            }
        {
            return Ok(NativeUnitSummary::Opaque);
        }

        let lower = name.to_ascii_lowercase();

        if lower == ".drectve"
            || lower.contains("__thread_data")
            || lower.contains("__thread_bss")
            || lower == ".tdata"
            || lower == ".tbss"
        {
            return Ok(NativeUnitSummary::Opaque);
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
    let mut references = BTreeSet::new();

    for symbol in file.symbols() {
        if !symbol.is_global() && !symbol.is_weak() {
            continue;
        }

        let undefined = symbol.is_undefined();

        if symbol.is_weak()
            || matches!(symbol.kind(), SymbolKind::Tls)
            || !undefined && matches!(symbol.kind(), SymbolKind::Unknown)
            || !matches!(symbol.section(), SymbolSection::Undefined | SymbolSection::Section(_))
            || matches!(symbol.flags(), object::SymbolFlags::CoffSection { selection, .. } if selection != 0)
        {
            return Ok(NativeUnitSummary::Opaque);
        }

        let Ok(name) = symbol.name() else {
            return Ok(NativeUnitSummary::Opaque);
        };

        let Some(name) = NonEmptySharedStr::try_new(name) else {
            return Ok(NativeUnitSummary::Opaque);
        };

        let contract = NativeSymbolContract::required_name(name);

        if undefined {
            references.insert(contract);
        } else {
            definitions.insert(NativeDefinition::new(contract, NativeDefinitionSelection::Ordinary));
        }
    }

    Ok(exact_summary(definitions, references, roots))
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
        return NativeUnitSummary::Opaque;
    }

    NativeUnitSummary::Exact {
        definitions: Arc::from(definitions.into_iter().collect::<Vec<_>>()),
        references: Arc::from(references.into_iter().collect::<Vec<_>>()),
        roots: Arc::from(roots.into_iter().collect::<Vec<_>>()),
    }
}

fn bitcode_roots(ir: &str) -> Option<BTreeSet<NativeRoot>> {
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

        if line.starts_with("@llvm.")
            || line.starts_with('$')
            || line.split_whitespace().any(|token| matches!(token, "alias" | "ifunc"))
            || line.contains(" comdat")
            || line.contains(" extern_weak ")
            || line.contains(" weak ")
            || line.contains(" weak_odr ")
            || line.contains(" linkonce ")
            || line.contains(" linkonce_odr ")
            || line.contains(" thread_local ")
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
    use object::{Architecture, BinaryFormat, Endianness, SectionKind, SymbolFlags, SymbolKind, SymbolScope};

    use super::{scan_bitcode_unit_summary, scan_object_unit_summary};
    use crate::{NativeRoot, NativeUnitSummary};

    #[test]
    fn code_data_and_address_references_are_exact() {
        let symbols = "entry T 0 0\ncallback_table D 0 0\ncallback U\n";
        let summary = scan_bitcode_unit_summary(symbols, "define @entry {}");

        let NativeUnitSummary::Exact { definitions, references, roots } = summary else {
            panic!("ordinary bitcode must have an exact summary");
        };

        assert_eq!(definitions.len(), 2);
        assert_eq!(references.len(), 1);
        assert!(roots.is_empty());
    }

    #[test]
    fn object_symbols_and_lifecycle_sections_are_scanned_without_tools() {
        for (format, section) in [
            (BinaryFormat::Coff, ".CRT$XCU"),
            (BinaryFormat::Elf, ".init_array"),
        ] {
            let bytes = object_fixture(format, section);
            let summary = scan_object_unit_summary(&bytes).unwrap();

            assert!(matches!(summary, NativeUnitSummary::Exact { definitions, references, roots }
                if definitions.len() == 1 && references.len() == 1
                && roots.as_ref() == [NativeRoot::Initialization]));
        }

        let opaque = object_fixture(BinaryFormat::Coff, ".drectve");
        assert!(matches!(scan_object_unit_summary(&opaque).unwrap(), NativeUnitSummary::Opaque));
    }

    #[test]
    fn qualified_aliases_and_ifuncs_are_opaque() {
        for declaration in [
            "@alternate = dso_local alias i32, ptr @value",
            "@alternate = hidden unnamed_addr alias i32, ptr @value",
            "@dispatch = dso_local ifunc void (), ptr @resolver",
        ] {
            assert!(matches!(
                scan_bitcode_unit_summary("entry T 0 0", declaration),
                NativeUnitSummary::Opaque,
            ));
        }
    }

    #[test]
    fn initialization_is_a_root_and_unsupported_selection_is_opaque() {
        let summary = scan_bitcode_unit_summary(
            "entry T 0 0\n",
            "@llvm.global_ctors = appending global []\n",
        );

        assert!(matches!(summary, NativeUnitSummary::Exact { roots, .. } if roots.as_ref() == [NativeRoot::Initialization]));

        assert!(matches!(
            scan_bitcode_unit_summary("entry W 0 0", ""),
            NativeUnitSummary::Opaque
        ));
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

        let lifecycle = object.add_section(Vec::new(), lifecycle_section.as_bytes().to_vec(), SectionKind::Data);
        object.append_section_data(lifecycle, &[0; 8], 8);

        object.write().unwrap()
    }
}
