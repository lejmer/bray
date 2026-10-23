use super::core::UnitTranslator;
use bray_codegen::{CodegenCleanupRequirement, CodegenFailure};
use bray_ir::{
    MirGeneratedLifecycleRole, MirOperand, MirOperationId, MirPlace, MirRuntimeReference,
};
use bray_runtime_interface::RuntimeAbiRole;
use inkwell::values::{BasicValueEnum, PointerValue};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    fn outgoing_requirements(
        &self,
        operation: MirOperationId,
    ) -> &'request [CodegenCleanupRequirement] {
        self.request
            .mappings()
            .operation(self.instance.key(), operation)
            .and_then(bray_codegen::CodegenOperationMapping::outgoing_requirements)
            .expect("checked MIR translation requires outgoing requirements")
    }

    pub(super) fn admit_root_frame(
        &mut self,
        root: &bray_ir::BoundUnitKey,
        outcome: PointerValue<'context>,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        let descriptor = crate::mapping::cleanup::storage_descriptor(
            self.module,
            self.request.mappings(),
            self.request.unit().executable_root_instance(root),
        );

        let count = self.pointer_integer_type().const_int(1, false);

        let bundle = crate::mapping::cleanup::bundle(
            &self.builder,
            descriptor.as_pointer_value(),
            count,
            self.types,
        )?;

        self.invoke_native_runtime(
            MirRuntimeReference::new(
                RuntimeAbiRole::OutgoingAdmission,
                self.unit.target().runtime_abi(),
            ),
            &[bundle.into(), outcome.into()],
        )?;

        Ok(bundle)
    }

    pub(super) fn outgoing_capacity(&self, operation: MirOperationId) -> u32 {
        let kind = self
            .unit
            .operation(operation)
            .expect("outgoing operation belongs to the translated unit")
            .kind();

        if matches!(
            kind,
            bray_ir::MirOperationKind::AdmitOutgoing {
                owner: bray_ir::MirOutgoingOwner::FrameFactory(_),
                ..
            } | bray_ir::MirOperationKind::DischargeOutgoing {
                owner: bray_ir::MirOutgoingOwner::FrameFactory(_),
                ..
            }
        ) {
            return 1;
        }

        u32::try_from(self.outgoing_requirements(operation).len())
            .expect("outgoing requirement count must fit native ABI")
    }

    fn outgoing_descriptors(
        &mut self,
        operation: MirOperationId,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        crate::mapping::cleanup::descriptors(
            self.module,
            &self.builder,
            self.outgoing_requirements(operation),
            self.types,
        )
    }

    pub(super) fn translate_outgoing(
        &mut self,
        operation: MirOperationId,
        owner: &bray_ir::MirOutgoingOwner,
        runtime: MirRuntimeReference,
    ) -> Result<(), CodegenFailure> {
        let capacity = self.outgoing_capacity(operation);

        if capacity == 0 {
            return Ok(());
        }

        let count = self
            .pointer_integer_type()
            .const_int(u64::from(capacity), false);

        let descriptors = match owner {
            bray_ir::MirOutgoingOwner::FrameFactory(callee) => {
                let factory = super::support::pointer_value(self.operand(callee)?)
                    .expect("asynchronous callable values retain their immutable factory record");

                crate::mapping::frame::factory_field(
                    self.types.context(),
                    &self.builder,
                    factory,
                    1,
                )?
            }
            bray_ir::MirOutgoingOwner::Value(_) | bray_ir::MirOutgoingOwner::Invocation { .. } => {
                self.outgoing_descriptors(operation)?
            }
        };

        let bundle =
            crate::mapping::cleanup::bundle(&self.builder, descriptors, count, self.types)?;

        if runtime.role() == RuntimeAbiRole::OutgoingAdmission {
            let context = self.checked_call_panic_report_context()?;

            self.set_pending_call_context(context)?;

            self.invoke_runtime(runtime, &[bundle.into(), context.into()])?;
        } else {
            self.invoke_runtime(runtime, &[bundle.into()])?;
        }

        Ok(())
    }

    pub(super) fn activate_outgoing_call(
        &mut self,
        operation: MirOperationId,
    ) -> Result<Option<(BasicValueEnum<'context>, PointerValue<'context>)>, CodegenFailure> {
        if self.outgoing_capacity(operation) == 0 {
            return Ok(None);
        }

        let descriptors = self.outgoing_descriptors(operation)?;
        let context = self.types.context();
        let lease_type = crate::mapping::cleanup::lease_type(context, self.types.target());
        let lease = self.allocate_temporary(lease_type, "cleanup.lease")?;

        self.builder
            .build_store(lease, lease_type.const_zero())
            .map_err(CodegenFailure::backend_library)?;

        Ok(self
            .invoke_runtime(
                MirRuntimeReference::new(
                    RuntimeAbiRole::OutgoingActivation,
                    self.unit.target().runtime_abi(),
                ),
                &[descriptors.into(), lease.into()],
            )?
            .map(|record| Some((record, lease)))
            .expect("outgoing activation runtime must return its record"))
    }

    pub(super) fn retire_outgoing_call(
        &mut self,
        (record, lease): (BasicValueEnum<'context>, PointerValue<'context>),
    ) -> Result<(), CodegenFailure> {
        let context = self
            .pending_call_panic_report_context
            .expect("checked MIR translation requires an established mapping or value");

        self.invoke_runtime(
            MirRuntimeReference::new(
                RuntimeAbiRole::OutgoingRetirement,
                self.unit.target().runtime_abi(),
            ),
            &[record, lease.into(), context.into()],
        )?;

        Ok(())
    }

    pub(super) fn transfer_cleanup_incident(
        &mut self,
        operation: MirOperationId,
        incident: &MirPlace,
        destination: &MirPlace,
        runtime: MirRuntimeReference,
    ) -> Result<(), CodegenFailure> {
        let mapping = self
            .request
            .mappings()
            .operation(self.instance.key(), operation)
            .expect("typed incident transfer has its concrete operation mapping");

        let identity = mapping
            .incident_type_identity()
            .expect("typed incident transfer has its concrete payload identity");

        let [requirement] = self.outgoing_requirements(operation) else {
            panic!("typed incident transfer must retain one selected producer requirement");
        };

        let storage = crate::mapping::incident::claim_storage(
            self.module,
            &self.builder,
            requirement,
            self.types,
        )?;

        // The operation and move bookkeeping share the immutable storage path.
        let payload = self.operand(&MirOperand::Move(incident.clone()))?;

        self.store_value(storage.1, payload, incident.ty())?;

        let [helper] = mapping.helpers() else {
            panic!("typed incident transfer has one selected destruction helper");
        };

        let cleanup = helper.symbol().map(|key| {
            (
                self.request
                    .mappings()
                    .symbol(key)
                    .expect("typed incident destruction has its concrete symbol"),
                MirGeneratedLifecycleRole::from_reference(helper.reference())
                    .expect("incident destruction has a selected lifecycle role"),
            )
        });

        let destroy =
            crate::mapping::incident::declare_destroyer(self.module, cleanup, self.types)?;

        let reporter = crate::mapping::incident::declare_reporter(self.module, self.types)?;
        let source = self.native_source_anchor(operation)?.into_struct_value();

        let record = crate::mapping::incident::record_value(
            &self.builder,
            identity,
            source,
            storage,
            reporter,
            destroy,
            self.types,
        )?;

        let address = self.allocate_temporary(record.get_type(), "incident.transfer")?;

        self.builder
            .build_store(address, record)
            .map_err(CodegenFailure::backend_library)?;

        let destination = self.place(destination)?;

        let current = self
            .builder
            .build_load(
                crate::native::panic_report_type(self.types.context()),
                destination,
                "incident.collection",
            )
            .map_err(CodegenFailure::backend_library)?;

        let report = self
            .invoke_native_runtime(runtime, &[address.into(), current])?
            .expect("incident retention returns the owned report");

        self.builder
            .build_store(destination, report)
            .map_err(CodegenFailure::backend_library)?;

        Ok(())
    }
}
