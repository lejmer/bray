use super::support::{llvm, physical_aggregate_element, pointer_value};
use crate::mapping::{LlvmDebugInfo, LlvmTypeMappings};
use crate::translation::frame::frame_storage_field_index;
use bray_codegen::{
    CodegenFailure, CodegenFieldLayout, CodegenInstance, CodegenParameterMapping, CodegenRequest,
    CodegenResultMapping, CodegenTypeKind, CodegenTypeMapping,
};
use bray_ir::{
    MirBlockId, MirPlace, MirStorageId, MirStorageKind, MirTerminatorKind, MirUnit, MirValueId,
};
use inkwell::IntPredicate;
use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::debug_info::DISubprogram;
use inkwell::module::Module;
use inkwell::types::StructType;
use inkwell::values::{BasicValueEnum, FunctionValue, PhiValue, PointerValue};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) enum TranslationError {
    Cancelled,
    Failed(CodegenFailure),
}

pub(crate) fn translate_instances<'context, 'request>(
    context: &'context Context,
    module: &Module<'context>,
    request: CodegenRequest<'request>,
    types: &mut LlvmTypeMappings<'context, 'request>,
    debug: Option<&LlvmDebugInfo<'context>>,
) -> Result<(), TranslationError> {
    for instance in request.unit().instances() {
        if request.cancellation().is_cancelled() {
            return Err(TranslationError::Cancelled);
        }

        types.select_instance(instance.key());

        if instance.protected_frame_identity().is_some() {
            super::super::frame::translate_protected_instance(
                context, module, request, instance, types, debug,
            )
            .map_err(TranslationError::Failed)?;

            continue;
        }

        translate_instance(context, module, request, instance, types, debug)
            .map_err(TranslationError::Failed)?;
    }

    Ok(())
}

fn translate_instance<'context, 'request>(
    context: &'context Context,
    module: &Module<'context>,
    request: CodegenRequest<'request>,
    instance: &'request CodegenInstance,
    types: &mut LlvmTypeMappings<'context, 'request>,
    debug: Option<&LlvmDebugInfo<'context>>,
) -> Result<(), CodegenFailure> {
    let symbol = request
        .mappings()
        .instance_symbol(instance.key())
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let function = module
        .get_function(symbol.name().as_str())
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let source = instance
        .mir()
        .blocks()
        .first()
        .map(bray_ir::MirBlock::source)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let debug_scope = debug.and_then(|debug| {
        debug.attach_function(
            function,
            symbol.name().as_str(),
            symbol.name().as_str(),
            source,
        )
    });

    UnitTranslator::new(
        context,
        module,
        request,
        instance,
        function,
        symbol.signature(),
        types,
        debug,
        debug_scope,
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
    pub(super) debug: Option<&'types LlvmDebugInfo<'context>>,
    pub(super) debug_scope: Option<DISubprogram<'context>>,
    pub(super) blocks: BTreeMap<MirBlockId, BasicBlock<'context>>,
    pub(super) reachable_blocks: BTreeSet<MirBlockId>,
    pub(super) phis: BTreeMap<MirValueId, PhiValue<'context>>,
    pub(super) storages: BTreeMap<MirStorageId, PointerValue<'context>>,
    pub(super) values: BTreeMap<MirValueId, BasicValueEnum<'context>>,
    pub(super) pending_moves: Vec<MirPlace>,
    pub(super) host_root: Option<BasicValueEnum<'context>>,
    pub(super) host_result: Option<BasicValueEnum<'context>>,
    pub(super) host_status: Option<inkwell::values::IntValue<'context>>,
    pub(super) host_selection_continuation: Option<BasicBlock<'context>>,
    pub(super) host_selection_shutdown: Option<BasicBlock<'context>>,
    pub(super) host_selection_statuses:
        Vec<(inkwell::values::IntValue<'context>, BasicBlock<'context>)>,
    pub(super) frame_context: Option<StructType<'context>>,
    pub(super) frame_dispatch: Option<BasicBlock<'context>>,
    pub(super) frame_progress: Option<BasicValueEnum<'context>>,
}

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn type_mapping(
        &self,
        ty: bray_symbols::TypeId,
    ) -> Option<&'request CodegenTypeMapping> {
        self.request.mappings().instance_ty(self.instance.key(), ty)
    }

    pub(super) fn mapped_type_size(&self, ty: bray_symbols::TypeId) -> Result<u64, CodegenFailure> {
        self.type_mapping(ty)
            .and_then(|mapping| mapping.layout().map(|layout| layout.size()))
            .ok_or(CodegenFailure::GeneratedModuleInvariant)
    }

    pub(super) fn aggregate_element(
        &self,
        fields: &[CodegenFieldLayout],
        semantic_index: usize,
    ) -> Result<u32, CodegenFailure> {
        physical_aggregate_element(fields, semantic_index, |field| {
            self.mapped_type_size(field.ty())
        })
    }

    pub(super) fn aggregate_value_element(
        &self,
        fields: &[CodegenFieldLayout],
        semantic_index: usize,
    ) -> Result<usize, CodegenFailure> {
        usize::try_from(self.aggregate_element(fields, semantic_index)?)
            .map_err(|_| CodegenFailure::ResourceExhausted)
    }

    pub(super) fn pointer_field_index(
        &self,
        fields: &[CodegenFieldLayout],
    ) -> Result<usize, CodegenFailure> {
        fields
            .iter()
            .position(|field| {
                self.type_mapping(field.ty()).is_some_and(|mapping| {
                    matches!(mapping.kind(), CodegenTypeKind::Pointer { .. })
                })
            })
            .ok_or(CodegenFailure::GeneratedModuleInvariant)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "translator construction keeps independent LLVM, MIR, type, and debug inputs explicit"
    )]
    pub(super) fn new(
        context: &'context Context,
        module: &'module Module<'context>,
        request: CodegenRequest<'request>,
        instance: &'request CodegenInstance,
        function: FunctionValue<'context>,
        signature: &'request bray_codegen::CodegenCallableSignature,
        types: &'types mut LlvmTypeMappings<'context, 'request>,
        debug: Option<&'types LlvmDebugInfo<'context>>,
        debug_scope: Option<DISubprogram<'context>>,
    ) -> Result<Self, CodegenFailure> {
        let unit = instance.mir();
        let builder = context.create_builder();

        let blocks = create_blocks(context, function, unit);

        let reachable_blocks = reachable_blocks(unit);

        Ok(Self {
            module,
            request,
            instance,
            unit,
            function,
            signature,
            types,
            builder,
            debug,
            debug_scope,
            blocks,
            reachable_blocks,
            phis: BTreeMap::new(),
            storages: BTreeMap::new(),
            values: BTreeMap::new(),
            pending_moves: Vec::new(),
            host_root: None,
            host_result: None,
            host_status: None,
            host_selection_continuation: None,
            host_selection_shutdown: None,
            host_selection_statuses: Vec::new(),
            frame_context: None,
            frame_dispatch: None,
            frame_progress: None,
        })
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "frame translator construction keeps independent LLVM, MIR, type, and debug inputs explicit"
    )]
    pub(crate) fn for_frame_resume(
        context: &'context Context,
        module: &'module Module<'context>,
        request: CodegenRequest<'request>,
        instance: &'request CodegenInstance,
        function: FunctionValue<'context>,
        frame_context: StructType<'context>,
        types: &'types mut LlvmTypeMappings<'context, 'request>,
        debug: Option<&'types LlvmDebugInfo<'context>>,
        debug_scope: Option<DISubprogram<'context>>,
    ) -> Result<Self, CodegenFailure> {
        let signature = request
            .mappings()
            .instance_symbol(instance.key())
            .map(bray_codegen::CodegenSymbolMapping::signature)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let unit = instance.mir();
        let dispatch = context.append_basic_block(function, "frame.dispatch");

        let blocks = create_blocks(context, function, unit);

        let reachable_blocks = reachable_blocks(unit);

        Ok(Self {
            module,
            request,
            instance,
            unit,
            function,
            signature,
            types,
            builder: context.create_builder(),
            debug,
            debug_scope,
            blocks,
            reachable_blocks,
            phis: BTreeMap::new(),
            storages: BTreeMap::new(),
            values: BTreeMap::new(),
            pending_moves: Vec::new(),
            host_root: None,
            host_result: None,
            host_status: None,
            host_selection_continuation: None,
            host_selection_shutdown: None,
            host_selection_statuses: Vec::new(),
            frame_context: Some(frame_context),
            frame_dispatch: Some(dispatch),
            frame_progress: None,
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

            if !self.reachable_blocks.contains(&id) {
                llvm(self.builder.build_unreachable())?;

                continue;
            }

            for operation_id in block.operations() {
                let operation = self
                    .unit
                    .operation(*operation_id)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                self.set_debug_location(operation.source());

                self.translate_operation(*operation_id, operation)?;
            }

            self.set_debug_location(block.terminator().source());
            self.translate_terminator(id, block.terminator().kind())?;
        }

        Ok(())
    }

    fn set_debug_location(&self, source: &bray_ir::MirSourceAnchor) {
        match (self.debug, self.debug_scope) {
            (Some(debug), Some(scope)) => debug.set_location(&self.builder, scope, source),
            (None, None) => {}
            (Some(_), None) | (None, Some(_)) => self.builder.unset_current_debug_location(),
        }
    }

    fn translate_frame_dispatch(&mut self) -> Result<(), CodegenFailure> {
        let (Some(frame_context), Some(dispatch)) = (self.frame_context, self.frame_dispatch)
        else {
            return Ok(());
        };

        let descriptor = self
            .unit
            .frame_descriptor()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let context = self.frame_context_argument()?;

        self.builder.position_at_end(dispatch);

        let pointer = llvm(
            self.builder.build_int_to_ptr(
                context,
                self.types
                    .context()
                    .ptr_type(inkwell::AddressSpace::default()),
                "frame.context",
            ),
        )?;

        let state_pointer =
            llvm(
                self.builder
                    .build_struct_gep(frame_context, pointer, 0, "frame.state.pointer"),
            )?;

        let state = llvm(self.builder.build_load(
            self.types.context().i32_type(),
            state_pointer,
            "frame.state",
        ))?
        .into_int_value();

        let cancellation_pointer = llvm(self.builder.build_struct_gep(
            frame_context,
            pointer,
            2,
            "frame.cancellation.pointer",
        ))?;

        let cancellation = llvm(self.builder.build_load(
            self.types.context().i8_type(),
            cancellation_pointer,
            "frame.cancellation",
        ))?
        .into_int_value();

        let mut cases = Vec::with_capacity(descriptor.states().len());

        for facts in descriptor.states() {
            let entry = self.block(facts.entry())?;

            let cancellation_entry =
                self.unit
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

        let fallback = self
            .types
            .context()
            .append_basic_block(self.function, "frame.invalid-state");

        self.builder.position_at_end(dispatch);

        llvm(self.builder.build_switch(state, fallback, &cases))?;

        self.builder.position_at_end(fallback);

        let failure =
            crate::native::frame_progress_type(self.types.context()).const_named_struct(&[
                self.types.context().i32_type().const_int(4, false).into(),
                self.types.context().i32_type().const_zero().into(),
                self.types.context().i64_type().const_zero().into(),
            ]);

        self.return_frame_progress(failure.into())?;

        Ok(())
    }

    pub(super) fn return_frame_progress(
        &self,
        progress: BasicValueEnum<'context>,
    ) -> Result<(), CodegenFailure> {
        crate::native::return_frame_result(
            &self.builder,
            self.function,
            self.request.target(),
            bray_runtime_interface::ProtectedFrameOperation::Resume,
            progress,
        )
    }

    pub(super) fn frame_context_argument(
        &self,
    ) -> Result<inkwell::values::IntValue<'context>, CodegenFailure> {
        self.function
            .get_nth_param(crate::native::frame_parameter_index(
                self.request.target(),
                bray_runtime_interface::ProtectedFrameOperation::Resume,
                0,
            ))
            .and_then(super::support::int_value)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)
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
        if let Some(frame_context) = self.frame_context {
            let dispatch = self
                .frame_dispatch
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            self.builder.position_at_end(dispatch);

            let context = self.frame_context_argument()?;

            let pointer = llvm(
                self.builder.build_int_to_ptr(
                    context,
                    self.types
                        .context()
                        .ptr_type(inkwell::AddressSpace::default()),
                    "frame.context",
                ),
            )?;

            for (id, _) in self.unit.storages_with_ids() {
                let storage = llvm(self.builder.build_struct_gep(
                    frame_context,
                    pointer,
                    frame_storage_field_index(id)?,
                    &format!("storage.{}", id.slot()),
                ))?;

                self.storages.insert(id, storage);
            }

            return Ok(());
        }

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
            .filter_map(|(id, storage)| match storage.kind() {
                MirStorageKind::Parameter(position) => Some((position, id)),
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();

        for (position, mapping) in self.signature.parameters().iter().enumerate() {
            let storage = u32::try_from(position)
                .ok()
                .and_then(|position| parameter_storages.get(&position).copied());

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

                    if let Some(storage) = storage {
                        llvm(self.builder.build_store(self.storage(storage)?, value))?;
                    }

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

                    if let Some(storage) = storage {
                        llvm(self.builder.build_store(self.storage(storage)?, value))?;
                    }

                    llvm_index += 1;
                }
            }
        }

        Ok(())
    }
}

fn create_blocks<'context>(
    context: &'context Context,
    function: FunctionValue<'context>,
    unit: &MirUnit,
) -> BTreeMap<MirBlockId, BasicBlock<'context>> {
    unit.blocks_with_ids()
        .enumerate()
        .map(|(index, (id, _))| {
            let block = context.append_basic_block(function, &format!("block.{index}"));

            (id, block)
        })
        .collect()
}

fn reachable_blocks(unit: &MirUnit) -> BTreeSet<MirBlockId> {
    let mut reachable = BTreeSet::new();
    let mut pending = vec![unit.entry()];

    while let Some(block) = pending.pop() {
        if !reachable.insert(block) {
            continue;
        }

        let Some(block) = unit.block(block) else {
            continue;
        };

        block
            .terminator()
            .kind()
            .for_each_successor(|successor| pending.push(successor));
    }

    reachable
}
