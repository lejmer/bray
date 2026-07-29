use super::support::{llvm, pointer_value};
use crate::mapping::LlvmTypeMappings;
use bray_codegen::{
    CodegenFailure, CodegenInstance, CodegenParameterMapping, CodegenRequest, CodegenResultMapping,
};
use bray_ir::{MirBlockId, MirStorageId, MirStorageKind, MirUnit, MirValueId};
use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::values::{BasicValueEnum, FunctionValue, PhiValue, PointerValue};
use std::collections::BTreeMap;

pub(crate) fn translate_instances<'context, 'module, 'request>(
    context: &'context Context,
    module: &'module Module<'context>,
    request: CodegenRequest<'request>,
    types: &mut LlvmTypeMappings<'context, 'request>,
) -> Result<(), CodegenFailure> {
    for instance in request.unit().instances() {
        if request.cancellation().is_cancelled() {
            return Err(CodegenFailure::BackendLibrary);
        }

        translate_instance(context, module, request, instance, types)?;
    }

    Ok(())
}

fn translate_instance<'context, 'module, 'request>(
    context: &'context Context,
    module: &'module Module<'context>,
    request: CodegenRequest<'request>,
    instance: &'request CodegenInstance,
    types: &mut LlvmTypeMappings<'context, 'request>,
) -> Result<(), CodegenFailure> {
    let symbol = request
        .mappings()
        .instance_symbol(instance.key())
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let function = module
        .get_function(symbol.name().as_str())
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    UnitTranslator::new(
        context,
        module,
        request,
        instance,
        function,
        symbol.signature(),
        types,
    )?
    .translate()
}

pub(super) struct UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) module: &'module Module<'context>,
    pub(super) request: CodegenRequest<'request>,
    pub(super) instance: &'request CodegenInstance,
    pub(super) unit: &'request MirUnit,
    pub(super) function: FunctionValue<'context>,
    pub(super) signature: &'request bray_codegen::CodegenCallableSignature,
    pub(super) types: &'types mut LlvmTypeMappings<'context, 'request>,
    pub(super) builder: Builder<'context>,
    pub(super) blocks: BTreeMap<MirBlockId, BasicBlock<'context>>,
    pub(super) phis: BTreeMap<MirValueId, PhiValue<'context>>,
    pub(super) storages: BTreeMap<MirStorageId, PointerValue<'context>>,
    pub(super) values: BTreeMap<MirValueId, BasicValueEnum<'context>>,
}

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn new(
        context: &'context Context,
        module: &'module Module<'context>,
        request: CodegenRequest<'request>,
        instance: &'request CodegenInstance,
        function: FunctionValue<'context>,
        signature: &'request bray_codegen::CodegenCallableSignature,
        types: &'types mut LlvmTypeMappings<'context, 'request>,
    ) -> Result<Self, CodegenFailure> {
        let unit = instance.mir();
        let builder = context.create_builder();

        let blocks = unit
            .blocks_with_ids()
            .enumerate()
            .map(|(index, (id, _))| {
                let block = context.append_basic_block(function, &format!("block.{index}"));

                (id, block)
            })
            .collect();

        Ok(Self {
            module,
            request,
            instance,
            unit,
            function,
            signature,
            types,
            builder,
            blocks,
            phis: BTreeMap::new(),
            storages: BTreeMap::new(),
            values: BTreeMap::new(),
        })
    }

    pub(super) fn translate(mut self) -> Result<(), CodegenFailure> {
        self.create_block_parameters()?;
        self.create_storages()?;
        self.bind_parameters()?;

        for (id, block) in self.unit.blocks_with_ids() {
            let llvm_block = self.block(id)?;

            self.builder.position_at_end(llvm_block);

            for operation_id in block.operations() {
                let operation = self
                    .unit
                    .operation(*operation_id)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                self.translate_operation(*operation_id, operation)?;
            }

            self.translate_terminator(id, block.terminator().kind())?;
        }

        Ok(())
    }

    pub(super) fn create_block_parameters(&mut self) -> Result<(), CodegenFailure> {
        for (id, block) in self.unit.blocks_with_ids() {
            let llvm_block = self.block(id)?;

            self.builder.position_at_end(llvm_block);

            for parameter in block.parameters() {
                let ty = self.value_type(*parameter)?;

                let phi = llvm(
                    self.builder
                        .build_phi(ty, &format!("value.{}", parameter.slot())),
                )?;

                self.phis.insert(*parameter, phi);
                self.values.insert(*parameter, phi.as_basic_value());
            }
        }

        Ok(())
    }

    pub(super) fn create_storages(&mut self) -> Result<(), CodegenFailure> {
        let entry = self.block(self.unit.entry())?;

        self.builder.position_at_end(entry);

        for (id, storage) in self.unit.storages_with_ids() {
            let ty = self.types.map(storage.ty())?;

            let pointer = llvm(
                self.builder
                    .build_alloca(ty, &format!("storage.{}", id.slot())),
            )?;

            self.storages.insert(id, pointer);
        }

        Ok(())
    }

    pub(super) fn bind_parameters(&mut self) -> Result<(), CodegenFailure> {
        let mut llvm_index = usize::from(matches!(
            self.signature.result(),
            CodegenResultMapping::Indirect { .. }
        ));

        let parameter_storages = self
            .unit
            .storages_with_ids()
            .filter(|(_, storage)| storage.kind() == MirStorageKind::Parameter);

        for ((storage, _), mapping) in parameter_storages.zip(self.signature.parameters()) {
            let destination = self.storage(storage)?;

            match mapping {
                CodegenParameterMapping::Ignore => {}
                CodegenParameterMapping::Direct { .. } => {
                    let value = self
                        .function
                        .get_nth_param(
                            u32::try_from(llvm_index)
                                .map_err(|_| CodegenFailure::ResourceExhausted)?,
                        )
                        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                    llvm(self.builder.build_store(destination, value))?;
                    llvm_index += 1;
                }
                CodegenParameterMapping::Indirect { pointee, .. } => {
                    let source = self
                        .function
                        .get_nth_param(
                            u32::try_from(llvm_index)
                                .map_err(|_| CodegenFailure::ResourceExhausted)?,
                        )
                        .and_then(pointer_value)
                        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                    let value = llvm(self.builder.build_load(
                        self.types.map(*pointee)?,
                        source,
                        "parameter.indirect",
                    ))?;

                    llvm(self.builder.build_store(destination, value))?;
                    llvm_index += 1;
                }
            }
        }

        Ok(())
    }
}
