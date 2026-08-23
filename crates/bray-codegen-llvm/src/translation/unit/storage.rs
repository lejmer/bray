use bray_codegen::CodegenFailure;
use bray_ir::MirStorageId;
use inkwell::module::Linkage;
use inkwell::values::PointerValue;

use super::core::UnitTranslator;
use super::support::{llvm, pointer_value};

impl<'context> UnitTranslator<'context, '_, '_, '_> {
    pub(super) fn initialize_native_static_storage(
        &mut self,
        storage: MirStorageId,
        destination: PointerValue<'context>,
    ) -> Result<(), CodegenFailure> {
        let mapping = self
            .request
            .mappings()
            .native_static_storage(self.instance.key(), storage)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let global = match mapping.direction() {
            bray_symbols::ForeignCallableDirection::Import => {
                let pointee = self.types.map(mapping.pointee_type())?;

                let global = self
                    .module
                    .get_global(mapping.symbol().as_str())
                    .unwrap_or_else(|| {
                        self.module
                            .add_global(pointee, None, mapping.symbol().as_str())
                    });

                let optional = mapping.presence() == bray_symbols::NativeSymbolPresence::Optional;

                global.set_linkage(if optional {
                    Linkage::ExternalWeak
                } else {
                    Linkage::External
                });

                if mapping.duration() == bray_symbols::StaticStorageDuration::ExactThread {
                    global.set_thread_local(true);
                }

                global
            }
            bray_symbols::ForeignCallableDirection::Export => self
                .module
                .get_global(mapping.symbol().as_str())
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?,
        };

        llvm(
            self.builder
                .build_store(destination, global.as_pointer_value()),
        )?;

        Ok(())
    }

    pub(super) fn static_storage_pointer(
        &mut self,
        storage: MirStorageId,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        let mapping = self
            .request
            .mappings()
            .static_storage(self.instance.key(), storage)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let accessor = self
            .module
            .get_function(&mapping.accessor_name())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let pointer = llvm(self.builder.build_call(accessor, &[], "static.access"))?
            .try_as_basic_value()
            .basic()
            .and_then(pointer_value)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        Ok(pointer)
    }
}
