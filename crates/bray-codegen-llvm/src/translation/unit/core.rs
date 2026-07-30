use super::support::{llvm, pointer_value};
use crate::mapping::LlvmTypeMappings;
use bray_codegen::{
    CodegenFailure, CodegenInstance, CodegenParameterMapping, CodegenRequest, CodegenResultMapping,
};
use bray_ir::{
    MirBlockId, MirStorageId, MirStorageKind, MirTerminatorKind, MirUnit, MirValueId,
};
use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::StructType;
use inkwell::values::{BasicValueEnum, FunctionValue, PhiValue, PointerValue};
use inkwell::IntPredicate;
use std::collections::BTreeMap;

pub(crate) enum TranslationError {
    Cancelled,
    Failed(CodegenFailure),
}

pub(crate) fn translate_instances<'context, 'module, 'request>(
    context: &'context Context,
    module: &'module Module<'context>,
    request: CodegenRequest<'request>,
    types: &mut LlvmTypeMappings<'context, 'request>,
) -> Result<(), TranslationError> {
    for instance in request.unit().instances() {
        if request.cancellation().is_cancelled() {
            return Err(TranslationError::Cancelled);
        }

        if instance.protected_frame_identity().is_some() {
            super::super::frame::translate_protected_instance(
                context,
                module,
                request,
                instance,
                types,
            )
            .map_err(TranslationError::Failed)?;

            continue;
        }

        translate_instance(context, module, request, instance, types)
            .map_err(TranslationError::Failed)?;
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

pub(crate) struct UnitTranslator<'context, 'module, 'request, 'types> {
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
    pub(super) host_result: Option<BasicValueEnum<'context>>,
    pub(super) frame_context: Option<StructType<'context>>,
    pub(super) frame_dispatch: Option<BasicBlock<'context>>,
    pub(super) frame_progress: Option<BasicValueEnum<'context>>,
    pub(super) frame_cancellation: Option<BasicValueEnum<'context>>,
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
            host_result: None,
            frame_context: None,
            frame_dispatch: None,
            frame_progress: None,
            frame_cancellation: None,
        })
    }

    pub(crate) fn for_frame_resume(
        context: &'context Context,
        module: &'module Module<'context>,
        request: CodegenRequest<'request>,
        instance: &'request CodegenInstance,
        function: FunctionValue<'context>,
        frame_context: StructType<'context>,
        types: &'types mut LlvmTypeMappings<'context, 'request>,
    ) -> Result<Self, CodegenFailure> {
        let signature = request
            .mappings()
            .instance_symbol(instance.key())
            .map(bray_codegen::CodegenSymbolMapping::signature)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let unit = instance.mir();
        let dispatch = context.append_basic_block(function, "frame.dispatch");

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
            builder: context.create_builder(),
            blocks,
            phis: BTreeMap::new(),
            storages: BTreeMap::new(),
            values: BTreeMap::new(),
            host_result: None,
            frame_context: Some(frame_context),
            frame_dispatch: Some(dispatch),
            frame_progress: None,
            frame_cancellation: function.get_nth_param(1),
        })
    }

    pub(crate) fn translate(mut self) -> Result<(), CodegenFailure> {
        self.create_block_parameters()?;
        self.create_storages()?;
        self.bind_parameters()?;
        self.translate_frame_dispatch()?;

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

    fn translate_frame_dispatch(&mut self) -> Result<(), CodegenFailure> {
        let (Some(frame_context), Some(dispatch)) =
            (self.frame_context, self.frame_dispatch)
        else {
            return Ok(());
        };

        let descriptor = self
            .unit
            .frame_descriptor()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let context = self
            .function
            .get_first_param()
            .and_then(super::support::int_value)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        self.builder.position_at_end(dispatch);

        let pointer = llvm(self.builder.build_int_to_ptr(
            context,
            self.types.context().ptr_type(inkwell::AddressSpace::default()),
            "frame.context",
        ))?;

        let state_pointer = llvm(self.builder.build_struct_gep(
            frame_context,
            pointer,
            0,
            "frame.state.pointer",
        ))?;

        let state = llvm(self.builder.build_load(
            self.types.context().i32_type(),
            state_pointer,
            "frame.state",
        ))?
        .into_int_value();

        let cancellation = self
            .frame_cancellation
            .and_then(super::support::int_value)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let mut cases = Vec::with_capacity(descriptor.states().len());

        for facts in descriptor.states() {
            let entry = self.block(facts.entry())?;

            let cancellation_entry = self
                .unit
                .blocks()
                .iter()
                .find_map(|block| match block.terminator().kind() {
                    MirTerminatorKind::Suspend {
                        resume_state,
                        cancellation,
                        ..
                    } if *resume_state == facts.state() => Some(cancellation.edge()),
                    _ => None,
                });

            let entry = if let Some(cancellation_entry) = cancellation_entry {
                if !cancellation_entry.arguments().is_empty() {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                }

                let state_dispatch = self
                    .types
                    .context()
                    .append_basic_block(self.function, "frame.state.dispatch");

                self.builder.position_at_end(state_dispatch);

                let requested = llvm(self.builder.build_int_compare(
                    IntPredicate::NE,
                    cancellation,
                    cancellation.get_type().const_zero(),
                    "frame.cancellation.requested",
                ))?;

                llvm(self.builder.build_conditional_branch(
                    requested,
                    self.block(cancellation_entry.target())?,
                    entry,
                ))?;

                state_dispatch
            } else {
                entry
            };

            cases.push((
                self.types
                    .context()
                    .i32_type()
                    .const_int(u64::from(facts.state().raw()), false),
                entry,
            ));
        }

        let fallback = self.types.context().append_basic_block(
            self.function,
            "frame.invalid-state",
        );

        self.builder.position_at_end(dispatch);

        llvm(self.builder.build_switch(state, fallback, &cases))?;

        self.builder.position_at_end(fallback);

        let failure = crate::native::frame_progress_type(self.types.context())
            .const_named_struct(&[
                self.types.context().i32_type().const_int(4, false).into(),
                self.types.context().i32_type().const_zero().into(),
                self.types.context().i64_type().const_zero().into(),
            ]);

        llvm(self.builder.build_return(Some(&failure)))?;

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

        if let Some(frame_context) = self.frame_context {
            let context = self
                .function
                .get_first_param()
                .and_then(super::support::int_value)
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            let pointer = llvm(self.builder.build_int_to_ptr(
                context,
                self.types.context().ptr_type(inkwell::AddressSpace::default()),
                "frame.context",
            ))?;

            for (id, _) in self.unit.storages_with_ids() {
                let index = id
                    .slot()
                    .checked_add(2)
                    .and_then(|index| u32::try_from(index).ok())
                    .ok_or(CodegenFailure::ResourceExhausted)?;

                let storage = llvm(self.builder.build_struct_gep(
                    frame_context,
                    pointer,
                    index,
                    &format!("storage.{}", id.slot()),
                ))?;

                self.storages.insert(id, storage);
            }

            return Ok(());
        }

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
        if self.frame_context.is_some() {
            return Ok(());
        }

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
