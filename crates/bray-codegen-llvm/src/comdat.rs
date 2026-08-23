use inkwell::comdat::ComdatSelectionKind;
use inkwell::module::Module;
use inkwell::values::GlobalValue;

pub(crate) fn attach(
    module: &Module<'_>,
    value: GlobalValue<'_>,
    name: &str,
    object_format: bray_target::ObjectFormat,
) {
    let selection = match object_format {
        bray_target::ObjectFormat::Coff => Some(ComdatSelectionKind::ExactMatch),
        bray_target::ObjectFormat::Elf | bray_target::ObjectFormat::WebAssembly => {
            Some(ComdatSelectionKind::Any)
        }
        bray_target::ObjectFormat::MachO | bray_target::ObjectFormat::Xcoff => None,
    };

    if let Some(selection) = selection {
        let comdat = module.get_or_insert_comdat(name);

        comdat.set_selection_kind(selection);
        value.set_comdat(comdat);
    }
}
