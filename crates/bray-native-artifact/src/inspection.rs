use std::collections::{BTreeMap, BTreeSet, HashMap};
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

/// Decodes one COFF deferred fallback-provider option.
pub fn alternate_name(option: &str) -> Option<(NonEmptySharedStr, NonEmptySharedStr)> {
    let (name, implementation) = option.strip_prefix("/alternatename:")?.split_once('=')?;

    Some((
        NonEmptySharedStr::try_new(name)?,
        NonEmptySharedStr::try_new(implementation)?,
    ))
}

fn add_fallback_definitions(
    aliases: Vec<(NonEmptySharedStr, NonEmptySharedStr)>,
    definitions: &mut BTreeSet<NativeDefinition>,
) -> bool {
    let mut unique = BTreeMap::new();

    for (name, implementation) in aliases {
        match unique.entry(name) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(implementation);
            }
            std::collections::btree_map::Entry::Occupied(entry)
                if entry.get() != &implementation =>
            {
                return false;
            }
            std::collections::btree_map::Entry::Occupied(_) => {}
        }
    }

    if unique.values().any(|implementation| {
        !definitions.iter().any(|definition| {
            definition.symbol().identity().name() == Some(implementation.as_str())
        })
    }) {
        // An alternate name may lead outside the unit or through another alias.
        // Preserve every reference when that foreign resolution contract is unknown.
        return false;
    }

    for (name, _) in unique {
        if definitions
            .iter()
            .any(|definition| definition.symbol().identity().name() == Some(name.as_str()))
        {
            continue;
        }

        definitions.insert(NativeDefinition::new(
            NativeSymbolContract::new(
                NativeSymbolIdentity::Name(name),
                None,
                NativeSymbolBinding::Weak,
                NativeSymbolPresence::Required,
            ),
            NativeDefinitionSelection::Fallback,
        ));
    }

    true
}

/// Reads a compiler-produced object in process, retaining exact selection only for
/// ordinary external symbols and sections with understood lifecycle behavior.
pub fn scan_object_unit_summary(bytes: &[u8]) -> Result<NativeUnitSummary, object::Error> {
    let file = object::File::parse(bytes)?;

    let (provided, mut references) = object_symbols(&file);

    references.retain(|reference| !provided.contains(reference.identity()));

    let Some(comdats) = object_comdats(&file) else {
        return Ok(NativeUnitSummary::opaque_with_providers(
            provided.iter().cloned(),
            references,
        ));
    };

    if file.kind() != object::ObjectKind::Relocatable {
        return Ok(NativeUnitSummary::opaque_with_providers(
            provided.iter().cloned(),
            references,
        ));
    }

    let mut roots = BTreeSet::new();
    let mut aliases = Vec::new();

    for section in file.sections() {
        let Ok(name) = section.name() else {
            return Ok(NativeUnitSummary::opaque_with_providers(
                provided.iter().cloned(),
                references,
            ));
        };

        if name == ".drectve" && file.format() == object::BinaryFormat::Coff {
            let Some(options) = section
                .data()
                .ok()
                .and_then(|bytes| std::str::from_utf8(bytes).ok())
            else {
                return Ok(NativeUnitSummary::opaque_with_providers(
                    provided.iter().cloned(),
                    references,
                ));
            };

            for option in options.split_whitespace() {
                let Some(alias) = alternate_name(option) else {
                    return Ok(NativeUnitSummary::opaque_with_providers(
                        provided.iter().cloned(),
                        references,
                    ));
                };

                aliases.push(alias);
            }

            continue;
        }

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
            return Ok(NativeUnitSummary::opaque_with_providers(
                provided.iter().cloned(),
                references,
            ));
        }

        let lower = name.to_ascii_lowercase();

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
            return Ok(NativeUnitSummary::opaque_with_providers(
                provided.iter().cloned(),
                references,
            ));
        }

        let Ok(name) = symbol.name() else {
            return Ok(NativeUnitSummary::opaque_with_providers(
                provided.iter().cloned(),
                references,
            ));
        };

        let Some(name) = NonEmptySharedStr::try_new(name) else {
            return Ok(NativeUnitSummary::opaque_with_providers(
                provided.iter().cloned(),
                references,
            ));
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
                return Ok(NativeUnitSummary::opaque_with_providers(
                    provided.iter().cloned(),
                    references,
                ));
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

    Ok(summarize_native_unit(
        definitions,
        references,
        roots,
        aliases,
    ))
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

    Ok(NativeUnitSummary::opaque_archive(provided, references))
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
#[cfg(test)]
fn scan_symbol_references(symbols: &str) -> BTreeSet<NativeSymbolContract> {
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

/// Builds an exact summary only when fallback providers and symbol overlap are understood.
/// Unknown or conflicting deferred providers retain all known references conservatively.
pub fn summarize_native_unit(
    mut definitions: BTreeSet<NativeDefinition>,
    references: BTreeSet<NativeSymbolContract>,
    roots: BTreeSet<NativeRoot>,
    aliases: Vec<(NonEmptySharedStr, NonEmptySharedStr)>,
) -> NativeUnitSummary {
    if !add_fallback_definitions(aliases, &mut definitions)
        || definitions.iter().any(|definition| {
            definition.selection() != &NativeDefinitionSelection::Fallback
                && references.iter().any(|reference| {
                    reference.identity() == definition.symbol().identity()
                        && reference.presence() == NativeSymbolPresence::Required
                })
        })
    {
        // Only deferred fallbacks may also demand their public provider within the unit.
        // Other definition/reference overlaps need selection rules this summary does not model.
        return NativeUnitSummary::opaque_with_providers(
            definitions
                .iter()
                .filter(|d| d.selection() != &NativeDefinitionSelection::Fallback)
                .map(|d| d.symbol().identity().clone()),
            references,
        );
    }

    NativeUnitSummary::Exact {
        definitions: Arc::from(definitions.into_iter().collect::<Vec<_>>()),
        references: Arc::from(references.into_iter().collect::<Vec<_>>()),
        roots: Arc::from(roots.into_iter().collect::<Vec<_>>()),
    }
}

#[cfg(test)]
mod tests {
    use object::write::{Object, StandardSection, Symbol, SymbolSection};
    use object::{
        Architecture, BinaryFormat, Endianness, SectionKind, SymbolFlags, SymbolKind, SymbolScope,
    };

    use super::scan_object_unit_summary;
    use crate::{NativeComdatSelection, NativeDefinitionSelection, NativeRoot, NativeUnitSummary};

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
