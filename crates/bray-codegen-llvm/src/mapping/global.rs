use inkwell::module::{Linkage, Module};
use inkwell::types::BasicType;
use inkwell::values::{BasicValueEnum, GlobalValue, UnnamedAddress};

pub(crate) fn declare_global<'context>(
    module: &Module<'context>,
    name: &str,
    ty: impl BasicType<'context>,
) -> GlobalValue<'context> {
    module
        .get_global(name)
        .unwrap_or_else(|| module.add_global(ty, None, name))
}

pub(crate) fn publish_immutable_global<'context>(
    module: &Module<'context>,
    target: &bray_codegen::CodegenTarget,
    name: &str,
    initializer: BasicValueEnum<'context>,
) -> GlobalValue<'context> {
    let global = declare_global(module, name, initializer.get_type());

    global.set_constant(true);
    global.set_linkage(Linkage::LinkOnceODR);
    global.set_unnamed_address(UnnamedAddress::Global);
    global.set_initializer(&initializer);

    crate::comdat::attach(module, global, name, target.machine().object_format());

    global
}

#[cfg(test)]
mod tests {
    use bray_codegen::test_support::codegen_request;
    use inkwell::context::Context;
    use inkwell::module::Linkage;
    use inkwell::values::UnnamedAddress;

    use super::publish_immutable_global;

    #[test]
    fn immutable_globals_share_identity_and_keep_static_storage() {
        let fixture = codegen_request();
        let context = Context::create();
        let module = context.create_module("immutable.globals");
        let initializer = context.i32_type().const_int(42, false).into();

        let first = publish_immutable_global(
            &module,
            fixture.request().target(),
            "shared.value",
            initializer,
        );

        let second = publish_immutable_global(
            &module,
            fixture.request().target(),
            "shared.value",
            initializer,
        );

        assert_eq!(first, second);
        assert_eq!(module.get_globals().count(), 1);
        assert_eq!(first.get_initializer(), Some(initializer));
        assert!(first.is_constant());
        assert_eq!(first.get_linkage(), Linkage::LinkOnceODR);
        assert_eq!(first.get_unnamed_address(), UnnamedAddress::Global);
        assert!(module.verify().is_ok());
    }
}
