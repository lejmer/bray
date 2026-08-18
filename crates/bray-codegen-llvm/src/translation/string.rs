use std::hash::Hasher as _;

use bray_base::{StableDigestHasher, lowercase_hex};
use inkwell::comdat::ComdatSelectionKind;
use inkwell::module::{Linkage, Module};
use inkwell::values::{BasicValueEnum, GlobalValue, UnnamedAddress};

pub(crate) fn publish_string_global<'context>(
    module: &Module<'context>,
    target: &bray_codegen::CodegenTarget,
    name: &str,
    initializer: BasicValueEnum<'context>,
) -> GlobalValue<'context> {
    let global = module
        .get_global(name)
        .unwrap_or_else(|| module.add_global(initializer.get_type(), None, name));

    global.set_constant(true);
    global.set_linkage(Linkage::LinkOnceODR);
    global.set_unnamed_address(UnnamedAddress::Global);
    global.set_initializer(&initializer);

    let selection = match target.machine().object_format() {
        bray_target::ObjectFormat::Coff => Some(ComdatSelectionKind::ExactMatch),
        bray_target::ObjectFormat::Elf | bray_target::ObjectFormat::WebAssembly => {
            Some(ComdatSelectionKind::Any)
        }
        bray_target::ObjectFormat::MachO | bray_target::ObjectFormat::Xcoff => None,
    };

    if let Some(selection) = selection {
        let comdat = module.get_or_insert_comdat(name);

        comdat.set_selection_kind(selection);
        global.set_comdat(comdat);
    }

    global
}

pub(crate) fn string_constant_name(text: &str) -> String {
    let mut hasher = StableDigestHasher::new();

    hasher.write(b"bray.string.utf8.without-terminator\0");
    hasher.write_u128(text.len() as u128);
    hasher.write(text.as_bytes());

    let digest = hasher.finalize();

    format!("bray.constant.string.{}", lowercase_hex(&digest))
}
