use std::collections::BTreeSet;
use std::sync::Arc;

use bray_base::NonEmptySharedStr;
use bray_symbols::{NativeSymbolContract, NativeSymbolPresence};

use crate::{NativeDefinition, NativeDefinitionSelection, NativeRoot, NativeUnitKind, NativeUnitSummary};

/// Summarizes an LLVM-inspected physical unit. Unknown selection or retention semantics
/// make the whole unit opaque instead of producing an incomplete exact graph.
pub fn scan_native_unit_summary(
    kind: NativeUnitKind,
    symbols: &str,
    structure: &str,
) -> NativeUnitSummary {
    if kind == NativeUnitKind::OpaqueArchive {
        return NativeUnitSummary::Opaque;
    }

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

    let roots = match kind {
        NativeUnitKind::Bitcode => bitcode_roots(structure),
        NativeUnitKind::Object => object_roots(structure),
        NativeUnitKind::OpaqueArchive => unreachable!("opaque archives returned before inspection"),
    };

    let Some(roots) = roots else {
        return NativeUnitSummary::Opaque;
    };

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

fn object_roots(inventory: &str) -> Option<BTreeSet<NativeRoot>> {
    let mut roots = BTreeSet::new();

    for line in inventory.lines().map(str::trim) {
        let lower = line.to_ascii_lowercase();

        if lower.contains("comdat")
            || lower.contains("weakexternal")
            || lower.contains("alias")
            || lower.contains("sht_group")
            || lower.contains(".drectve")
            || lower.contains("no_dead_strip")
            || lower.contains("live_support")
            || lower.contains("shf_tls")
            || lower.contains("thread_local")
            || lower.contains(".tdata")
            || lower.contains(".tbss")
            || lower.contains("__thread_data")
            || lower.contains("__thread_bss")
            || lower.contains("n_indr")
            || (lower.starts_with("selection:") && !lower.ends_with("(0x0)"))
        {
            return None;
        }

        let Some(section) = lower
            .strip_prefix("name:")
            .or_else(|| lower.strip_prefix("section:"))
            .and_then(|value| value.split_whitespace().next())
        else {
            continue;
        };

        if section == ".init"
            || section.contains(".init_array")
            || section.contains(".preinit_array")
            || section.contains(".ctors")
            || section.contains("__mod_init_func")
            || section.contains(".crt$x") && !section.contains(".crt$xt")
        {
            roots.insert(NativeRoot::Initialization);
        }

        if section == ".fini"
            || section.contains(".fini_array")
            || section.contains(".dtors")
            || section.contains("__mod_term_func")
            || section.contains(".crt$xt")
        {
            roots.insert(NativeRoot::Finalization);
        }
    }

    Some(roots)
}

#[cfg(test)]
mod tests {
    use super::scan_native_unit_summary;
    use crate::{NativeRoot, NativeUnitKind, NativeUnitSummary};

    #[test]
    fn code_data_and_address_references_are_exact() {
        let symbols = "entry T 0 0\ncallback_table D 0 0\ncallback U\n";
        let summary = scan_native_unit_summary(NativeUnitKind::Bitcode, symbols, "define @entry {}");

        let NativeUnitSummary::Exact { definitions, references, roots } = summary else {
            panic!("ordinary bitcode must have an exact summary");
        };

        assert_eq!(definitions.len(), 2);
        assert_eq!(references.len(), 1);
        assert!(roots.is_empty());
    }

    #[test]
    fn real_object_format_inventories_preserve_address_references_and_initializers() {
        let cases = [
            ("callback U 0 0\ncallback_table D 0 0\nentry T 0 0\n", "Name: .CRT$XCU (2E 43 52 54 24 58 43 55)"),
            ("callback U 0 0\ncallback_table D 0 8\nentry T 0 1d\n", "Name: .init_array (12)"),
            ("_callback U 0 0\n_callback_table D 38 0\n_entry T 0 0\n", "Name: __mod_init_func (5F 5F 6D 6F 64)"),
        ];

        for (symbols, inventory) in cases {
            let summary = scan_native_unit_summary(NativeUnitKind::Object, symbols, inventory);

            assert!(matches!(summary, NativeUnitSummary::Exact { definitions, references, roots }
                if definitions.len() == 2 && references.len() == 1
                && roots.as_ref() == [NativeRoot::Initialization]));
        }

        assert!(matches!(
            scan_native_unit_summary(NativeUnitKind::Object, "entry T 0 0", "Name: .drectve"),
            NativeUnitSummary::Opaque,
        ));

        assert!(matches!(
            scan_native_unit_summary(NativeUnitKind::Object, "entry T 0 0", "Name: .CRT$XTU"),
            NativeUnitSummary::Exact { roots, .. } if roots.as_ref() == [NativeRoot::Finalization],
        ));

        for (inventory, root) in [
            ("Name: .init (2E 69 6E 69 74)", NativeRoot::Initialization),
            ("Section: .fini (2E 66 69 6E 69)", NativeRoot::Finalization),
        ] {
            assert!(matches!(
                scan_native_unit_summary(NativeUnitKind::Object, "entry T 0 0", inventory),
                NativeUnitSummary::Exact { roots, .. } if roots.as_ref() == [root],
            ));
        }
    }

    #[test]
    fn qualified_aliases_and_ifuncs_are_opaque() {
        for declaration in [
            "@alternate = dso_local alias i32, ptr @value",
            "@alternate = hidden unnamed_addr alias i32, ptr @value",
            "@dispatch = dso_local ifunc void (), ptr @resolver",
        ] {
            assert!(matches!(
                scan_native_unit_summary(NativeUnitKind::Bitcode, "entry T 0 0", declaration),
                NativeUnitSummary::Opaque,
            ));
        }
    }

    #[test]
    fn initialization_is_a_root_and_unsupported_selection_is_opaque() {
        let summary = scan_native_unit_summary(
            NativeUnitKind::Bitcode,
            "entry T 0 0\n",
            "@llvm.global_ctors = appending global []\n",
        );

        assert!(matches!(summary, NativeUnitSummary::Exact { roots, .. } if roots.as_ref() == [NativeRoot::Initialization]));

        assert!(matches!(
            scan_native_unit_summary(NativeUnitKind::Bitcode, "entry W 0 0", ""),
            NativeUnitSummary::Opaque
        ));

        assert!(matches!(
            scan_native_unit_summary(NativeUnitKind::Object, "entry T 0 0", "Selection: Any (0x2)"),
            NativeUnitSummary::Opaque
        ));
    }
}
