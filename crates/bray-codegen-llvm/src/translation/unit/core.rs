use super::edge::{checked_call_operations, reachable_blocks};
use super::support::{llvm, physical_aggregate_element, pointer_value};
use crate::mapping::{LlvmDebugInfo, LlvmTypeMappings, apply_instance_optimization_attributes};
use crate::translation::frame::frame_storage_field_index;
use bray_codegen::{
    CodegenFailure, CodegenFieldLayout, CodegenInstance, CodegenLinkage, CodegenParameterMapping,
    CodegenRequest, CodegenResultMapping, CodegenSymbolMapping, CodegenTypeKind,
    CodegenTypeMapping,
};
use bray_ir::{
    MirBlockId, MirOperationId, MirPlace, MirStorageId, MirStorageKind, MirUnit, MirValueId,
};
use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::debug_info::DISubprogram;
use inkwell::module::Module;
use inkwell::types::{BasicType, StructType};
use inkwell::values::{BasicValueEnum, FunctionValue, PhiValue, PointerValue};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) enum TranslationError {
    Cancelled,
    Failed(CodegenFailure),
}

struct PreparedInstance<'context, 'request> {
    symbol: &'request CodegenSymbolMapping,
    function: FunctionValue<'context>,
    trampoline: Option<FunctionValue<'context>>,
    translate: bool,
}

pub(crate) fn translate_instances<'context, 'request>(
    context: &'context Context,
    module: &Module<'context>,
    request: CodegenRequest<'request>,
    types: &mut LlvmTypeMappings<'context, 'request>,
    debug: Option<&LlvmDebugInfo<'context>>,
) -> Result<(), TranslationError> {
    let mut prepared = Vec::with_capacity(request.unit().instances().len());

    for instance in request.unit().instances() {
        if request.cancellation().is_cancelled() {
            return Err(TranslationError::Cancelled);
        }

        types.select_instance(instance.key());

        if instance.protected_frame_identity().is_some() {
            prepared.push(None);

            continue;
        }

        let (symbol, function) =
            instance_function(module, request, instance).map_err(TranslationError::Failed)?;

        let translate = symbol.linkage() != CodegenLinkage::Import;

        let (function, trampoline) = if translate {
            super::callback::prepare(module, request, symbol, function, types)
                .map_err(TranslationError::Failed)?
        } else {
            (function, None)
        };

        prepared.push(Some(PreparedInstance {
            symbol,
            function,
            trampoline,
            translate,
        }));
    }

    for (instance, prepared) in request.unit().instances().iter().zip(prepared) {
        if request.cancellation().is_cancelled() {
            return Err(TranslationError::Cancelled);
        }

        types.select_instance(instance.key());

        if instance.protected_frame_identity().is_some() {
            let (_, function) =
                instance_function(module, request, instance).map_err(TranslationError::Failed)?;

            apply_instance_optimization_attributes(function, instance, types)
                .map_err(TranslationError::Failed)?;

            super::super::frame::translate_protected_instance(
                context, module, request, instance, types, debug,
            )
            .map_err(TranslationError::Failed)?;

            continue;
        }

        let PreparedInstance {
            symbol,
            function,
            trampoline,
            translate,
        } = prepared.ok_or(TranslationError::Failed(
            CodegenFailure::GeneratedModuleInvariant,
        ))?;

        if !translate {
            continue;
        }

        apply_instance_optimization_attributes(function, instance, types)
            .map_err(TranslationError::Failed)?;

        translate_instance(
            context, module, request, instance, symbol, function, trampoline, types, debug,
        )
        .map_err(TranslationError::Failed)?;
    }

    Ok(())
}

fn instance_function<'context, 'request>(
    module: &Module<'context>,
    request: CodegenRequest<'request>,
    instance: &CodegenInstance,
) -> Result<(&'request CodegenSymbolMapping, FunctionValue<'context>), CodegenFailure> {
    let symbol = request
        .mappings()
        .instance_symbol(instance.key())
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let function = module
        .get_function(symbol.name().as_str())
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    Ok((symbol, function))
}

fn translate_instance<'context, 'request>(
    context: &'context Context,
    module: &Module<'context>,
    request: CodegenRequest<'request>,
    instance: &'request CodegenInstance,
    symbol: &'request CodegenSymbolMapping,
    function: FunctionValue<'context>,
    trampoline: Option<FunctionValue<'context>>,
    types: &mut LlvmTypeMappings<'context, 'request>,
    debug: Option<&LlvmDebugInfo<'context>>,
) -> Result<(), CodegenFailure> {
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
    .translate()?;

    if let Some(trampoline) = trampoline {
        super::callback::translate(
            context, module, request, symbol, function, trampoline, types,
        )?;
    }

    Ok(())
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
    pub(super) checked_call_operations: BTreeSet<MirOperationId>,
    pub(super) phis: BTreeMap<MirValueId, PhiValue<'context>>,
    pub(super) storages: BTreeMap<MirStorageId, PointerValue<'context>>,
    pub(super) values: BTreeMap<MirValueId, BasicValueEnum<'context>>,
    pub(super) run_result_transfers:
        BTreeMap<bray_symbols::TypeId, (bray_ir::MirRunResultVariants, FunctionValue<'context>)>,
    pub(super) pending_moves: Vec<MirPlace>,
    pub(super) panic_report_context: Option<PointerValue<'context>>,
    pub(super) pending_call_panic_report_context: Option<PointerValue<'context>>,
    pub(super) host_root: Option<BasicValueEnum<'context>>,
    pub(super) host_result: Option<BasicValueEnum<'context>>,
    pub(super) host_returned_value: Option<super::effect::HostReturnedValue<'context>>,
    pub(super) host_status: Option<inkwell::values::IntValue<'context>>,
    pub(super) performance_loop: Option<PerformanceLoop<'context>>,
    pub(super) host_selection_continuation: Option<BasicBlock<'context>>,
    pub(super) host_selection_shutdown: Option<BasicBlock<'context>>,
    pub(super) host_selection_statuses:
        Vec<(inkwell::values::IntValue<'context>, BasicBlock<'context>)>,
    pub(super) frame_context: Option<StructType<'context>>,
    pub(super) frame_dispatch: Option<BasicBlock<'context>>,
    pub(super) frame_progress: Option<BasicValueEnum<'context>>,
}

pub(super) struct PerformanceLoop<'context> {
    pub(super) header: BasicBlock<'context>,
    pub(super) iteration: PhiValue<'context>,
    pub(super) status: PhiValue<'context>,
    pub(super) inner_iterations: std::num::NonZeroU64,
}

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn allocate_temporary(
        &self,
        ty: impl BasicType<'context>,
        name: &str,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        crate::translation::allocate_temporary(self.types.context(), &self.builder, ty, name)
    }

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
        crate::conversion::resource_limit(
            self.aggregate_element(fields, semantic_index)?,
            "aggregate_element_index",
        )
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
        let checked_call_operations = checked_call_operations(unit);

        let panic_report_context =
            super::panic::incoming_panic_report_context(function, signature)?;

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
            checked_call_operations,
            phis: BTreeMap::new(),
            storages: BTreeMap::new(),
            values: BTreeMap::new(),
            run_result_transfers: BTreeMap::new(),
            pending_moves: Vec::new(),
            panic_report_context,
            pending_call_panic_report_context: None,
            host_root: None,
            host_result: None,
            host_returned_value: None,
            host_status: None,
            performance_loop: None,
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
            .map(CodegenSymbolMapping::signature)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let unit = instance.mir();
        let dispatch = context.append_basic_block(function, "frame.dispatch");

        let blocks = create_blocks(context, function, unit);

        let reachable_blocks = reachable_blocks(unit);
        let checked_call_operations = checked_call_operations(unit);

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
            checked_call_operations,
            phis: BTreeMap::new(),
            storages: BTreeMap::new(),
            values: BTreeMap::new(),
            pending_moves: Vec::new(),
            panic_report_context: None,
            run_result_transfers: BTreeMap::new(),
            pending_call_panic_report_context: None,
            host_root: None,
            host_result: None,
            host_returned_value: None,
            host_status: None,
            performance_loop: None,
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

                self.translate_operation(*operation_id, operation).map_err(
                    |failure| match failure {
                        CodegenFailure::GeneratedModuleInvariant => {
                            CodegenFailure::generated_module_invariant((
                                operation.source(),
                                operation_id,
                                operation.kind(),
                            ))
                        }
                        failure => failure,
                    },
                )?;
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

            for (id, storage) in self.unit.storages_with_ids() {
                if matches!(storage.kind(), MirStorageKind::Static(_)) {
                    continue;
                }

                let storage = llvm(self.builder.build_struct_gep(
                    frame_context,
                    pointer,
                    frame_storage_field_index(id)?,
                    &format!("storage.{}", id.slot()),
                ))?;

                self.storages.insert(id, storage);

                if matches!(
                    self.unit.storage(id).map(bray_ir::MirStorage::kind),
                    Some(MirStorageKind::NativeStatic(_))
                ) {
                    self.initialize_native_static_storage(id, storage)?;
                }
            }

            return Ok(());
        }

        let entry = self.block(self.unit.entry())?;

        self.builder.position_at_end(entry);

        for (id, storage) in self.unit.storages_with_ids() {
            if matches!(storage.kind(), MirStorageKind::Static(_)) {
                continue;
            }

            let ty = self.types.map(storage.ty())?;

            let pointer = llvm(
                self.builder
                    .build_alloca(ty, &format!("storage.{}", id.slot())),
            )?;

            self.storages.insert(id, pointer);

            if matches!(storage.kind(), MirStorageKind::NativeStatic(_)) {
                self.initialize_native_static_storage(id, pointer)?;
            }
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
                        .get_nth_param(crate::conversion::resource_limit(
                            llvm_index,
                            "aggregate_element_index",
                        )?)
                        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                    if let Some(storage) = storage {
                        let destination = self.storage(storage)?;

                        llvm(self.builder.build_store(destination, value))?;
                    }

                    llvm_index += 1;
                }
                CodegenParameterMapping::Indirect { pointee, .. } => {
                    let source = self
                        .function
                        .get_nth_param(crate::conversion::resource_limit(
                            llvm_index,
                            "aggregate_element_index",
                        )?)
                        .and_then(pointer_value)
                        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                    let value = llvm(self.builder.build_load(
                        self.types.map(*pointee)?,
                        source,
                        "parameter.indirect",
                    ))?;

                    if let Some(storage) = storage {
                        let destination = self.storage(storage)?;

                        llvm(self.builder.build_store(destination, value))?;
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
    std::iter::once(unit.entry())
        .chain(
            unit.blocks_with_ids()
                .map(|(id, _)| id)
                .filter(|id| *id != unit.entry()),
        )
        .map(|id| {
            let block = context.append_basic_block(function, &format!("block.{}", id.slot()));

            (id, block)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use bray_ir::{
        MirBlockKind, MirEdge, MirSourceAnchor, MirTerminatorKind, MirUnitBuilder, MirUnitKind,
    };
    use inkwell::context::Context;

    #[test]
    fn native_block_order_starts_with_the_selected_mir_entry() {
        let bound = bray_testing::test_bound_unit(42);
        let source = MirSourceAnchor::from(bound.key().source());

        let mut mir = MirUnitBuilder::for_bound(
            bound.identity(),
            MirUnitKind::Synchronous,
            bray_testing::test_mir_target(),
        );

        let body = mir
            .push_block(source.clone(), MirBlockKind::Ordinary)
            .unwrap();

        let entry = mir
            .push_block(source.clone(), MirBlockKind::Ordinary)
            .unwrap();

        mir.set_terminator(body, source.clone(), MirTerminatorKind::Return(None))
            .unwrap();

        mir.set_terminator(
            entry,
            source,
            MirTerminatorKind::Goto(MirEdge::new(body, [])),
        )
        .unwrap();

        let mir = mir.finish(entry).unwrap();
        let context = Context::create();
        let module = context.create_module("reordered-entry");

        let function =
            module.add_function("destroy", context.void_type().fn_type(&[], false), None);

        let blocks = super::create_blocks(&context, function, &mir);
        let builder = context.create_builder();

        assert_eq!(function.get_first_basic_block(), Some(blocks[&entry]));
        assert_eq!(blocks[&body].get_name().to_str().unwrap(), "block.0");
        assert_eq!(blocks[&entry].get_name().to_str().unwrap(), "block.1");

        builder.position_at_end(blocks[&entry]);
        builder.build_unconditional_branch(blocks[&body]).unwrap();
        builder.position_at_end(blocks[&body]);
        builder.build_return(None).unwrap();

        module.verify().unwrap();
    }
}
