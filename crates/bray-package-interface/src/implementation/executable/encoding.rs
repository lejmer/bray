// rust-style: allow(module-too-large, reason = "the executable MIR wire encoder keeps one exhaustive operation and terminator mapping")

use std::sync::Arc;

use bray_bound_tree::{
    BoundCallResult, BoundUnitKey, CheckedMemoryOperationKind, ConstructionInputId,
    ConstructionTarget, ConversionTarget, InlineAssemblyContract, InlineAssemblyOperandKind,
    PatternOperation, PatternProjection, SelectedConversion,
};
use bray_ir::{
    MirAggregateKind, MirBinaryOperator, MirBlockKind, MirCall, MirCallArgument, MirCallTarget,
    MirCleanupEdge, MirCleanupPhase, MirConstructionInput, MirEdge, MirGeneratorKind,
    MirGeneratorOperation, MirImmediateValue, MirMemoryOperation, MirNumericConversionKind,
    MirOperand, MirOperationKind, MirPanicCause, MirPatternPredicate, MirPlace, MirProjectionKind,
    MirRuntimeReference, MirStorageKind, MirStoreKind, MirSwitchCase, MirTerminatorKind,
    MirTextOperation, MirTextOperationKind, MirUnaryOperator, MirUnit, MirUnitKind, MirValueOrigin,
};
use bray_symbols::{
    AnySymbolId, CallableInstanceData, CallablePhaseBehavior, CallablePhaseBehaviors,
    ConstantTermId, ConstantValueId, DependencyContractTemplateId, GenericSubstitutionId,
    ImplementationInstanceId, LifecycleObligationKind, TraitApplicationId, TypeId,
};

use crate::semantic::write_symbol_reference;
use crate::wire::WireEncoder;
use crate::{
    InterfaceConstantTermId, InterfaceConstantValueId, InterfaceGenericSubstitutionId,
    InterfaceImplementationInstanceId, InterfaceSymbolReference, InterfaceTraitApplicationId,
    InterfaceTypeId,
};

use super::support::{FORMAT_VERSION, write_bool, write_count, write_optional};

/// Maps compilation-local semantic identities into one package interface's tables.
pub trait ExecutableTemplateEncodeContext {
    /// Error reported while completing a required interface semantic record.
    type Error;

    /// Maps one semantic type.
    fn type_id(&mut self, id: TypeId) -> Result<InterfaceTypeId, Self::Error>;

    /// Maps one canonical constant value.
    fn constant_value_id(
        &mut self,
        id: ConstantValueId,
    ) -> Result<InterfaceConstantValueId, Self::Error>;

    /// Maps one open or closed constant term.
    fn constant_term_id(
        &mut self,
        id: ConstantTermId,
    ) -> Result<InterfaceConstantTermId, Self::Error>;

    /// Maps one generic substitution.
    fn substitution_id(
        &mut self,
        id: GenericSubstitutionId,
    ) -> Result<InterfaceGenericSubstitutionId, Self::Error>;

    /// Maps one applied trait.
    fn trait_application_id(
        &mut self,
        id: TraitApplicationId,
    ) -> Result<InterfaceTraitApplicationId, Self::Error>;

    /// Maps one selected implementation instance.
    fn implementation_instance_id(
        &mut self,
        id: ImplementationInstanceId,
    ) -> Result<InterfaceImplementationInstanceId, Self::Error>;

    /// Maps one portable dependency contract.
    fn dependency_contract_id(
        &mut self,
        id: DependencyContractTemplateId,
    ) -> Result<crate::InterfaceDependencyContractId, Self::Error>;

    /// Maps one declaration identity.
    fn symbol_reference(
        &mut self,
        id: AnySymbolId,
    ) -> Result<InterfaceSymbolReference, Self::Error>;

    /// Maps one source-owned nested executable unit into its artifact-local identity.
    fn nested_executable_id(
        &mut self,
        key: &BoundUnitKey,
    ) -> Result<bray_ir::MirExecutableTemplateId, Self::Error>;
}

/// Failure while encoding one checked executable template.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExecutableTemplateEncodeError<E> {
    /// Completing one referenced semantic record failed.
    Semantic(E),
    /// Compiler-generated product-host MIR cannot be exported as a callable template.
    InvalidUnitKind,
}

/// Encodes one checked generic MIR unit using package-interface semantic references.
pub fn encode_executable_template<C: ExecutableTemplateEncodeContext>(
    unit: &MirUnit,
    context: &mut C,
) -> Result<Arc<[u8]>, ExecutableTemplateEncodeError<C::Error>> {
    let mut encoder = Encoder {
        wire: WireEncoder::new(),
        context,
    };

    encoder.wire.write_u32(FORMAT_VERSION);
    encoder.target(unit.target());
    encoder.unit_kind(unit.kind())?;
    encoder.wire.write_u32(unit.entry().slot());

    write_count(&mut encoder.wire, unit.blocks().len());

    for block in unit.blocks() {
        encoder.block_kind(block.kind());
        write_count(&mut encoder.wire, block.parameters().len());

        for parameter in block.parameters() {
            encoder.wire.write_u32(parameter.slot());
        }

        write_count(&mut encoder.wire, block.operations().len());

        for operation in block.operations() {
            encoder.wire.write_u32(operation.slot());
        }
    }

    write_count(&mut encoder.wire, unit.storages().len());

    for storage in unit.storages() {
        encoder.storage_kind(storage.kind())?;
        encoder.ty(storage.ty())?;
    }

    write_count(&mut encoder.wire, unit.values().len());

    for value in unit.values() {
        encoder.ty(value.ty())?;

        match value.origin() {
            MirValueOrigin::BlockParameter(block) => {
                encoder.wire.write_u32(0);
                encoder.wire.write_u32(block.slot());
            }
            MirValueOrigin::Operation(operation) => {
                encoder.wire.write_u32(1);
                encoder.wire.write_u32(operation.slot());
            }
        }
    }

    write_count(&mut encoder.wire, unit.operations().len());

    for operation in unit.operations() {
        write_optional(&mut encoder.wire, operation.result(), |wire, result| {
            wire.write_u32(result.slot());
        });

        encoder.operation(operation.kind())?;
    }

    for block in unit.blocks() {
        encoder.terminator(block.terminator().kind())?;
    }

    encoder.frame_descriptor(unit)?;

    Ok(Arc::from(encoder.wire.into_bytes()))
}

#[cfg(test)]
pub(super) fn encode_projection_for_test<C: ExecutableTemplateEncodeContext>(
    projection: &MirProjectionKind,
    context: &mut C,
) -> Result<Vec<u8>, ExecutableTemplateEncodeError<C::Error>> {
    let mut encoder = Encoder {
        wire: WireEncoder::new(),
        context,
    };

    encoder.projection_kind(projection)?;

    Ok(encoder.wire.into_bytes())
}

/// Encodes one validated MIR unit under its complete specialization identity.
pub fn encode_pre_specialized_mir<C: ExecutableTemplateEncodeContext>(
    key: crate::PackageImplementationSpecializationKey,
    unit: &MirUnit,
    context: &mut C,
) -> Result<crate::InterfacePreSpecializedMir, ExecutableTemplateEncodeError<C::Error>> {
    let payload = encode_executable_template(unit, context)?;

    Ok(
        crate::InterfacePreSpecializedMir::new(key, crate::CURRENT_MIR_SCHEMA_REVISION, payload)
            .unwrap_or_else(|| unreachable!("the executable MIR codec always emits a header")),
    )
}

struct Encoder<'context, C> {
    wire: WireEncoder,
    context: &'context mut C,
}

impl<C: ExecutableTemplateEncodeContext> Encoder<'_, C> {
    fn target(&mut self, target: &bray_ir::MirTargetContract) {
        self.wire.write_bytes(&target.compatibility_digest());
    }

    fn semantic<T>(
        result: Result<T, C::Error>,
    ) -> Result<T, ExecutableTemplateEncodeError<C::Error>> {
        result.map_err(ExecutableTemplateEncodeError::Semantic)
    }

    fn ty(&mut self, id: TypeId) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        let id = Self::semantic(self.context.type_id(id))?;

        self.wire.write_u32(id.raw());

        Ok(())
    }

    fn constant_value(
        &mut self,
        id: ConstantValueId,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        let id = Self::semantic(self.context.constant_value_id(id))?;

        self.wire.write_u32(id.raw());

        Ok(())
    }

    fn constant_term(
        &mut self,
        id: ConstantTermId,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        let id = Self::semantic(self.context.constant_term_id(id))?;

        self.wire.write_u32(id.raw());

        Ok(())
    }

    fn substitution(
        &mut self,
        id: GenericSubstitutionId,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        let id = Self::semantic(self.context.substitution_id(id))?;

        self.wire.write_u32(id.raw());

        Ok(())
    }

    fn trait_application(
        &mut self,
        id: TraitApplicationId,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        let id = Self::semantic(self.context.trait_application_id(id))?;

        self.wire.write_u32(id.raw());

        Ok(())
    }

    fn trait_dispatch(
        &mut self,
        dispatch: bray_symbols::TraitConstraintDispatch,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        match dispatch {
            bray_symbols::TraitConstraintDispatch::Constraint { owner, ordinal } => {
                self.wire.write_u32(0);
                self.symbol(owner.symbol())?;
                self.wire.write_u32(ordinal.raw());
            }
            bray_symbols::TraitConstraintDispatch::TraitDefault(requirement) => {
                self.wire.write_u32(1);
                self.implementation_requirement(requirement)?;
            }
        }

        Ok(())
    }

    fn implementation(
        &mut self,
        id: ImplementationInstanceId,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        let id = Self::semantic(self.context.implementation_instance_id(id))?;

        self.wire.write_u32(id.raw());

        Ok(())
    }

    fn symbol(&mut self, id: AnySymbolId) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        let reference = Self::semantic(self.context.symbol_reference(id))?;

        write_symbol_reference(&mut self.wire, &reference);

        Ok(())
    }

    fn unit_kind(
        &mut self,
        kind: &MirUnitKind,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        match kind {
            MirUnitKind::Synchronous => self.wire.write_u32(0),
            MirUnitKind::ProtectedAsyncFrame(frame) => {
                self.wire.write_u32(1);
                self.wire.write_bytes(&frame.digest());
            }
            MirUnitKind::ExecutableHost(_) | MirUnitKind::GeneratedLifecycle(_) => {
                return Err(ExecutableTemplateEncodeError::InvalidUnitKind);
            }
        }

        Ok(())
    }

    fn block_kind(&mut self, kind: MirBlockKind) {
        self.wire.write_u32(match kind {
            MirBlockKind::Ordinary => 0,
            MirBlockKind::CleanupBroadcast => 1,
            MirBlockKind::LifecycleResolution => 2,
        });
    }

    fn storage_kind(
        &mut self,
        kind: &MirStorageKind,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        let (tag, ordinal) = match kind {
            MirStorageKind::Parameter(ordinal) => (0, Some(*ordinal)),
            MirStorageKind::Local => (1, None),
            MirStorageKind::Temporary => (2, None),
            MirStorageKind::Return => (3, None),
            MirStorageKind::InactiveFrame => (4, None),
            MirStorageKind::CurrentFrame => (5, None),
            MirStorageKind::CurrentTask => (6, None),
            MirStorageKind::ChildTask => (7, None),
            MirStorageKind::Static(_) => (8, None),
            MirStorageKind::NativeStatic(_) => (9, None),
        };

        self.wire.write_u32(tag);

        if let Some(ordinal) = ordinal {
            self.wire.write_u32(ordinal);
        }

        if let MirStorageKind::Static(reference) | MirStorageKind::NativeStatic(reference) = kind {
            self.symbol(reference.template().declaration().into())?;
            self.substitution(reference.substitution())?;

            let witnesses: &[ImplementationInstanceId] = match &reference {
                bray_symbols::StaticReferenceSelection::Open {
                    selected_witnesses, ..
                } => selected_witnesses,
                bray_symbols::StaticReferenceSelection::Closed(instance) => {
                    instance.selected_witnesses()
                }
            };

            write_count(&mut self.wire, witnesses.len());

            for witness in witnesses {
                self.implementation(*witness)?;
            }
        }

        Ok(())
    }

    fn operation(
        &mut self,
        operation: &MirOperationKind,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        match operation {
            MirOperationKind::AnonymousCallable(reference) => {
                let bray_ir::MirAnonymousCallableReference::Bound(unit) = reference else {
                    return Err(ExecutableTemplateEncodeError::InvalidUnitKind);
                };

                let identity = Self::semantic(self.context.nested_executable_id(unit))?;

                self.wire.write_u32(20);
                self.wire.write_u32(identity.raw());
            }
            MirOperationKind::DeclaredCallable(callable) => {
                self.wire.write_u32(19);
                self.callable_instance(callable.instance())?;
                self.callable_abi(callable.abi());
            }
            MirOperationKind::Store {
                kind,
                destination,
                value,
            } => {
                self.wire.write_u32(1);
                self.store_kind(*kind);
                self.place(destination)?;
                self.operand(value)?;
            }
            MirOperationKind::Borrow { kind, place } => {
                self.wire.write_u32(2);
                self.borrow_kind(*kind);
                self.place(place)?;
            }
            MirOperationKind::Unary { operator, operand } => {
                self.wire.write_u32(3);
                self.unary_operator(*operator);
                self.operand(operand)?;
            }
            MirOperationKind::Binary {
                operator,
                left,
                right,
            } => {
                self.wire.write_u32(4);
                self.binary_operator(*operator);
                self.operand(left)?;
                self.operand(right)?;
            }
            MirOperationKind::Aggregate(aggregate) => {
                self.wire.write_u32(5);
                self.aggregate_kind(aggregate.kind());
                self.operands(aggregate.operands())?;
            }
            MirOperationKind::Construct(construction) => {
                self.wire.write_u32(6);
                self.construction_target(construction.target())?;
                write_count(&mut self.wire, construction.inputs().len());

                for input in construction.inputs() {
                    self.construction_input(input)?;
                }
            }
            MirOperationKind::Convert {
                operand,
                conversion,
            } => {
                self.wire.write_u32(7);
                self.operand(operand)?;
                self.conversion(conversion)?;
            }
            MirOperationKind::NumericConversion { kind, operand } => {
                self.wire.write_u32(8);
                self.numeric_conversion_kind(*kind);
                self.operand(operand)?;
            }
            MirOperationKind::PatternProjection {
                subject,
                projection,
                operation,
            } => {
                self.wire.write_u32(9);
                self.operand(subject)?;
                self.pattern_projection(*projection)?;
                self.pattern_operation(*operation);
            }
            MirOperationKind::Generator(operation) => {
                self.wire.write_u32(10);
                self.generator_operation(operation)?;
            }
            MirOperationKind::Call(call) => {
                self.wire.write_u32(11);
                self.call(call)?;
            }
            MirOperationKind::Memory(operation) => {
                self.wire.write_u32(12);
                self.memory_operation(operation)?;
            }
            MirOperationKind::Text(operation) => {
                self.wire.write_u32(13);
                self.text_operation(operation)?;
            }
            MirOperationKind::PanicReport(cause) => {
                self.wire.write_u32(14);
                self.panic_cause(cause)?;
            }
            MirOperationKind::Finalize(place) => {
                self.wire.write_u32(15);
                self.place(place)?;
            }
            MirOperationKind::Destroy(place) => {
                self.wire.write_u32(16);
                self.place(place)?;
            }
            MirOperationKind::Cleanup { phase, place } => {
                self.wire.write_u32(17);
                self.cleanup_phase(*phase);
                self.place(place)?;
            }
            MirOperationKind::Async(operation) => {
                self.wire.write_u32(18);
                self.async_operation(operation)?;
            }
            MirOperationKind::NullableQuery(query) => {
                self.wire.write_u32(21);

                self.wire.write_u32(match query.kind() {
                    bray_ir::MirNullableQueryKind::IsPresent => 0,
                    bray_ir::MirNullableQueryKind::IsAbsent => 1,
                });

                self.operand(query.operand())?;
                self.ty(query.operand_type())?;
                self.ty(query.nullable_type())?;
                self.ty(query.result_type())?;
            }
            MirOperationKind::Host(_) => {
                return Err(ExecutableTemplateEncodeError::InvalidUnitKind);
            }
        }

        Ok(())
    }

    fn operand(
        &mut self,
        operand: &MirOperand,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        match operand {
            MirOperand::Value(value) => {
                self.wire.write_u32(0);
                self.wire.write_u32(value.slot());
            }
            MirOperand::Constant { value, ty } => {
                self.wire.write_u32(1);
                self.constant_value(*value)?;
                self.ty(*ty)?;
            }
            MirOperand::Immediate { value, ty } => {
                self.wire.write_u32(2);
                self.immediate(*value);
                self.ty(*ty)?;
            }
            MirOperand::Copy(place) => {
                self.wire.write_u32(3);
                self.place(place)?;
            }
            MirOperand::Move(place) => {
                self.wire.write_u32(4);
                self.place(place)?;
            }
        }

        Ok(())
    }

    fn operands(
        &mut self,
        operands: &[MirOperand],
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        write_count(&mut self.wire, operands.len());

        for operand in operands {
            self.operand(operand)?;
        }

        Ok(())
    }

    fn place(&mut self, place: &MirPlace) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        self.wire.write_u32(place.storage().slot());
        self.ty(place.ty())?;
        write_count(&mut self.wire, place.projections().len());

        for projection in place.projections() {
            self.projection_kind(projection.kind())?;
            self.ty(projection.source_type())?;
            self.ty(projection.result_type())?;
        }

        Ok(())
    }

    fn projection_kind(
        &mut self,
        projection: &MirProjectionKind,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        match projection {
            MirProjectionKind::Dereference => self.wire.write_u32(0),
            MirProjectionKind::Field(field) => {
                self.wire.write_u32(1);

                self.symbol(match field {
                    bray_ir::MirFieldReference::Struct(field) => (*field).into(),
                    bray_ir::MirFieldReference::UnionPayload(field) => (*field).into(),
                })?;
            }
            MirProjectionKind::TupleField(ordinal) => {
                self.wire.write_u32(2);
                self.wire.write_u32(*ordinal);
            }
            MirProjectionKind::ElementFromStart(ordinal) => {
                self.wire.write_u32(3);
                self.wire.write_u32(*ordinal);
            }
            MirProjectionKind::ElementFromEnd(ordinal) => {
                self.wire.write_u32(4);
                self.wire.write_u32(*ordinal);
            }
            MirProjectionKind::Index(index) => {
                self.wire.write_u32(5);
                self.operand(index)?;
            }
            MirProjectionKind::Slice { start, end } => {
                self.wire.write_u32(6);
                self.optional_operand(start.as_ref())?;
                self.optional_operand(end.as_ref())?;
            }
            MirProjectionKind::Variant(variant) => {
                self.wire.write_u32(7);
                self.symbol((*variant).into())?;
            }
            MirProjectionKind::ActiveUnionPayloadField { variant, field } => {
                self.wire.write_u32(8);
                self.symbol((*variant).into())?;
                self.symbol((*field).into())?;
            }
            MirProjectionKind::NullableValue => self.wire.write_u32(9),
            MirProjectionKind::OwnedStorage => self.wire.write_u32(10),
            MirProjectionKind::ActiveUnionPayloadElement { variant, ordinal } => {
                self.wire.write_u32(11);
                self.symbol((*variant).into())?;
                self.wire.write_u32(ordinal.raw());
            }
        }

        Ok(())
    }

    fn optional_operand(
        &mut self,
        operand: Option<&MirOperand>,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        match operand {
            Some(operand) => {
                self.wire.write_u32(1);
                self.operand(operand)?;
            }
            None => self.wire.write_u32(0),
        }

        Ok(())
    }

    fn call(&mut self, call: &MirCall) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        match call.target() {
            MirCallTarget::Direct(reference) => {
                self.wire.write_u32(0);
                self.callable_instance(reference.instance())?;
                self.callable_abi(reference.abi());
            }
            MirCallTarget::Indirect { callee, abi } => {
                self.wire.write_u32(1);
                self.operand(callee)?;
                self.callable_abi(*abi);
            }
            MirCallTarget::Runtime(reference) => {
                self.wire.write_u32(2);
                self.runtime_reference(*reference);
            }
        }

        self.call_result(call.result())?;
        write_count(&mut self.wire, call.arguments().len());

        for argument in call.arguments() {
            self.call_argument(argument)?;
        }

        self.phase_behaviors(call.phase_behaviors())?;

        write_count(&mut self.wire, call.dispatch_witnesses().len());

        for witness in call.dispatch_witnesses() {
            self.implementation(*witness)?;
        }

        match call.trait_dispatch() {
            Some(dispatch) => {
                self.wire.write_u32(1);
                self.trait_dispatch(dispatch)?;
            }
            None => self.wire.write_u32(0),
        }

        match call.intrinsic() {
            Some(bray_ir::MirCallIntrinsic::Unary(operator)) => {
                self.wire.write_u32(1);
                self.unary_operator(operator);
            }
            Some(bray_ir::MirCallIntrinsic::Binary(operator)) => {
                self.wire.write_u32(2);
                self.binary_operator(operator);
            }
            Some(bray_ir::MirCallIntrinsic::Comparison {
                less,
                equal,
                greater,
            }) => {
                self.wire.write_u32(4);
                self.symbol(less.into())?;
                self.symbol(equal.into())?;
                self.symbol(greater.into())?;
            }
            Some(bray_ir::MirCallIntrinsic::Conversion(target)) => {
                self.wire.write_u32(3);
                self.ty(target)?;
            }
            None => self.wire.write_u32(0),
        }

        write_count(&mut self.wire, call.witnesses().len());

        for witness in call.witnesses() {
            self.implementation_requirement(witness.requirement())?;
            self.implementation(witness.witness())?;
        }

        Ok(())
    }

    fn phase_behaviors(
        &mut self,
        behaviors: Option<&CallablePhaseBehaviors>,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        let Some(behaviors) = behaviors else {
            self.wire.write_u32(0);

            return Ok(());
        };

        self.wire.write_u32(1);
        self.phase_behavior(behaviors.invocation())?;

        match behaviors.deferred_execution() {
            Some(behavior) => {
                self.wire.write_u32(1);
                self.phase_behavior(behavior)?;
            }
            None => self.wire.write_u32(0),
        }

        Ok(())
    }

    fn phase_behavior(
        &mut self,
        behavior: &CallablePhaseBehavior,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        write_count(&mut self.wire, behavior.effects().len());

        for requirement in behavior.effects() {
            self.symbol(requirement.declaration())?;
        }

        write_count(&mut self.wire, behavior.capabilities().len());

        for requirement in behavior.capabilities() {
            self.symbol(requirement.declaration())?;
        }

        write_count(&mut self.wire, behavior.trusted_capabilities().len());

        for requirement in behavior.trusted_capabilities() {
            self.wire.write_u32(requirement.ordinal().raw());
            self.symbol(requirement.capability().into())?;
        }

        write_count(&mut self.wire, behavior.execution_requirements().len());

        for requirement in behavior.execution_requirements() {
            self.symbol(requirement.declaration())?;
        }

        write_count(&mut self.wire, behavior.lifecycle_obligations().len());

        for obligation in behavior.lifecycle_obligations() {
            self.wire.write_u32(match obligation {
                LifecycleObligationKind::Destruction => 0,
                LifecycleObligationKind::Finalization => 1,
                LifecycleObligationKind::Cancellation => 2,
                LifecycleObligationKind::Joining => 3,
            });
        }

        let dependency = Self::semantic(
            self.context
                .dependency_contract_id(behavior.dependency_contract()),
        )?;

        self.wire.write_u32(dependency.raw());

        self.wire
            .write_u32(match behavior.current_run_cancellation() {
                bray_symbols::CurrentRunCancellation::NotEntered => 0,
                bray_symbols::CurrentRunCancellation::MayEnter => 1,
            });

        Ok(())
    }

    fn callable_instance(
        &mut self,
        instance: CallableInstanceData,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        self.symbol(instance.definition().symbol())?;

        self.substitution(instance.substitution())
    }

    fn call_result(
        &mut self,
        result: BoundCallResult,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        match result {
            BoundCallResult::Immediate(ty) => {
                self.wire.write_u32(0);
                self.ty(ty)?;
            }
            BoundCallResult::LazyFuture(future) => {
                self.wire.write_u32(1);
                self.ty(future.completion_type())?;
                self.ty(future.future_type())?;
            }
        }

        Ok(())
    }

    fn call_argument(
        &mut self,
        argument: &MirCallArgument,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        match argument {
            MirCallArgument::Receiver { parameter, value } => {
                self.wire.write_u32(0);
                self.symbol((*parameter).into())?;
                self.operand(value)?;
            }
            MirCallArgument::Explicit {
                parameter,
                ordinal,
                value,
            } => {
                self.wire.write_u32(1);

                match parameter {
                    Some(parameter) => {
                        self.wire.write_u32(1);
                        self.symbol((*parameter).into())?;
                    }
                    None => self.wire.write_u32(0),
                }

                self.wire.write_u32(*ordinal);
                self.operand(value)?;
            }
            MirCallArgument::Default {
                parameter,
                ordinal,
                provider,
            } => {
                self.wire.write_u32(2);
                self.symbol((*parameter).into())?;
                self.wire.write_u32(*ordinal);
                self.symbol((*provider).into())?;
            }
        }

        Ok(())
    }

    fn implementation_requirement(
        &mut self,
        requirement: bray_symbols::ImplementationRequirementKey,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        self.ty(requirement.subject())?;

        self.trait_application(requirement.trait_application())
    }

    fn terminator(
        &mut self,
        terminator: &MirTerminatorKind,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        match terminator {
            MirTerminatorKind::Goto(edge) => {
                self.wire.write_u32(0);
                self.edge(edge)?;
            }
            MirTerminatorKind::Branch {
                condition,
                then_edge,
                else_edge,
            } => {
                self.wire.write_u32(1);
                self.operand(condition)?;
                self.edge(then_edge)?;
                self.edge(else_edge)?;
            }
            MirTerminatorKind::PatternBranch {
                subject,
                predicate,
                matched,
                unmatched,
            } => {
                self.wire.write_u32(2);
                self.operand(subject)?;
                self.pattern_predicate(*predicate)?;
                self.edge(matched)?;
                self.edge(unmatched)?;
            }
            MirTerminatorKind::Iterate {
                cursor,
                next,
                witness,
                element_type,
                item,
                exhausted,
            } => {
                self.wire.write_u32(3);
                self.place(cursor)?;
                self.callable_instance(next.instance())?;
                self.callable_abi(next.abi());
                self.implementation(*witness)?;
                self.ty(*element_type)?;
                self.wire.write_u32(item.slot());
                self.edge(exhausted)?;
            }
            MirTerminatorKind::Switch {
                discriminant,
                cases,
                otherwise,
            } => {
                self.wire.write_u32(4);
                self.operand(discriminant)?;
                write_count(&mut self.wire, cases.len());

                for case in cases.iter() {
                    self.switch_case(case)?;
                }

                self.edge(otherwise)?;
            }
            MirTerminatorKind::Return(value) => {
                self.wire.write_u32(5);
                self.optional_operand(value.as_ref())?;
            }
            MirTerminatorKind::Unreachable => self.wire.write_u32(6),
            MirTerminatorKind::Suspend {
                kind,
                payload,
                resume_state,
                resume,
                cancellation,
                registration,
                wake,
            } => {
                self.wire.write_u32(7);

                self.wire.write_u32(match kind {
                    bray_ir::MirSuspensionKind::Awaited => 0,
                    bray_ir::MirSuspensionKind::Yield => 1,
                    bray_ir::MirSuspensionKind::TaskEvent => 2,
                });

                self.optional_operand(payload.as_ref())?;
                self.wire.write_u32(resume_state.raw());
                self.edge(resume)?;
                self.cleanup_edge(cancellation)?;
                self.runtime_reference(*registration);
                self.runtime_reference(*wake);
            }
            MirTerminatorKind::ForwardRunResult { result, edges } => {
                self.wire.write_u32(8);
                self.operand(result)?;
                self.symbol(edges.completed_variant().into())?;
                self.edge(edges.completed())?;
                self.symbol(edges.panicked_variant().into())?;
                self.cleanup_edge(edges.panicked())?;
                self.symbol(edges.cancelled_variant().into())?;
                self.cleanup_edge(edges.cancelled())?;
            }
            MirTerminatorKind::BeginCleanup(edge) => {
                self.wire.write_u32(9);
                self.cleanup_edge(edge)?;
            }
            MirTerminatorKind::ContinueCleanup(edge) => {
                self.wire.write_u32(10);
                self.cleanup_edge(edge)?;
            }
            MirTerminatorKind::Panic { report, cleanup } => {
                self.wire.write_u32(11);
                self.operand(report)?;
                self.cleanup_edge(cleanup)?;
            }
            MirTerminatorKind::PropagatePanic { report, runtime } => {
                self.wire.write_u32(12);
                self.operand(report)?;
                self.runtime_reference(*runtime);
            }
            MirTerminatorKind::PropagateCancellation { runtime } => {
                self.wire.write_u32(13);
                self.runtime_reference(*runtime);
            }
            MirTerminatorKind::CancelCurrentRun { cleanup } => {
                self.wire.write_u32(14);
                self.cleanup_edge(cleanup)?;
            }
            MirTerminatorKind::InlineAssembly(assembly) => {
                self.wire.write_u32(15);
                self.inline_assembly_contract(assembly.contract())?;
                self.operand(assembly.inputs())?;
                self.ty(assembly.inputs_type())?;
                self.ty(assembly.output_type())?;
                self.wire.write_u32(assembly.normal().slot());
                write_count(&mut self.wire, assembly.alternates().len());

                for alternate in assembly.alternates() {
                    self.wire.write_u32(alternate.slot());
                }

                self.callable_references(assembly.symbols())?;
            }
            MirTerminatorKind::RangeIterate {
                cursor,
                element_type,
                item,
                exhausted,
            } => {
                self.wire.write_u32(16);
                self.place(cursor)?;
                self.ty(*element_type)?;
                self.wire.write_u32(item.slot());
                self.edge(exhausted)?;
            }
            MirTerminatorKind::CheckCallPanic {
                completed,
                panicked,
            } => {
                self.wire.write_u32(17);
                self.edge(completed)?;
                self.wire.write_u32(panicked.target().slot());
                self.ty(panicked.report_type())?;
            }
        }

        Ok(())
    }

    fn edge(&mut self, edge: &MirEdge) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        self.wire.write_u32(edge.target().slot());

        self.operands(edge.arguments())
    }

    fn cleanup_edge(
        &mut self,
        edge: &MirCleanupEdge,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        self.cleanup_phase(edge.phase());

        self.edge(edge.edge())
    }

    fn switch_case(
        &mut self,
        case: &MirSwitchCase,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        self.constant_value(case.value())?;

        self.edge(case.edge())
    }

    fn frame_descriptor(
        &mut self,
        unit: &MirUnit,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        let Some(frame) = unit.frame_descriptor() else {
            self.wire.write_u32(0);

            return Ok(());
        };

        self.wire.write_u32(1);
        self.wire.write_bytes(&frame.frame().digest());
        self.wire.write_u16(frame.abi_version().major());
        self.wire.write_u16(frame.abi_version().minor());

        for operation in bray_runtime_interface::ProtectedFrameAbiOperation::ALL {
            let version = frame.frame_abi().operation(operation);

            self.wire.write_u16(version.major());
            self.wire.write_u16(version.minor());
        }

        self.ty(frame.result_type())?;
        write_count(&mut self.wire, frame.states().len());

        for state in frame.states() {
            self.wire.write_u32(state.state().raw());
            self.wire.write_u32(state.entry().slot());

            self.wire.write_u32(state.affinity().code());

            write_count(&mut self.wire, state.lane_requirements().len());

            for requirement in state.lane_requirements() {
                self.execution_lane(*requirement);
            }

            write_count(&mut self.wire, state.initialized_storages().len());

            for storage in state.initialized_storages() {
                self.wire.write_u32(storage.slot());
            }
        }

        Ok(())
    }

    fn immediate(&mut self, value: MirImmediateValue) {
        self.wire.write_u32(match value {
            MirImmediateValue::Boolean(false) => 0,
            MirImmediateValue::Boolean(true) => 1,
            MirImmediateValue::Unit => 2,
            MirImmediateValue::NullableAbsent => 3,
        });
    }

    fn store_kind(&mut self, kind: MirStoreKind) {
        self.wire.write_u32(match kind {
            MirStoreKind::Initialize => 0,
            MirStoreKind::Assign => 1,
        });
    }

    fn borrow_kind(&mut self, kind: bray_symbols::BorrowKind) {
        self.wire.write_u32(match kind {
            bray_symbols::BorrowKind::Shared => 0,
            bray_symbols::BorrowKind::Mutable => 1,
        });
    }

    fn unary_operator(&mut self, operator: MirUnaryOperator) {
        self.wire.write_u32(match operator {
            MirUnaryOperator::Negate => 0,
            MirUnaryOperator::Not => 1,
            MirUnaryOperator::BitwiseNot => 2,
        });
    }

    fn binary_operator(&mut self, operator: MirBinaryOperator) {
        self.wire.write_u32(match operator {
            MirBinaryOperator::Add => 0,
            MirBinaryOperator::Subtract => 1,
            MirBinaryOperator::Multiply => 2,
            MirBinaryOperator::Divide => 3,
            MirBinaryOperator::Remainder => 4,
            MirBinaryOperator::Equal => 5,
            MirBinaryOperator::NotEqual => 6,
            MirBinaryOperator::LessThan => 7,
            MirBinaryOperator::LessThanOrEqual => 8,
            MirBinaryOperator::GreaterThan => 9,
            MirBinaryOperator::GreaterThanOrEqual => 10,
            MirBinaryOperator::BitwiseAnd => 11,
            MirBinaryOperator::BitwiseOr => 12,
            MirBinaryOperator::BitwiseXor => 13,
            MirBinaryOperator::ShiftLeft => 14,
            MirBinaryOperator::ShiftRight => 15,
        });
    }

    fn aggregate_kind(&mut self, kind: MirAggregateKind) {
        self.wire.write_u32(match kind {
            MirAggregateKind::Tuple => 0,
            MirAggregateKind::Array => 1,
            MirAggregateKind::RepeatedArray => 2,
            MirAggregateKind::NullablePresent => 3,
            MirAggregateKind::Range => 4,
        });
    }

    fn numeric_conversion_kind(&mut self, kind: MirNumericConversionKind) {
        self.wire.write_u32(match kind {
            MirNumericConversionKind::Truncate => 0,
        });
    }

    fn cleanup_phase(&mut self, phase: MirCleanupPhase) {
        self.wire.write_u32(match phase {
            MirCleanupPhase::TaskCancellation => 0,
            MirCleanupPhase::LifecycleResolution => 1,
        });
    }

    fn callable_abi(&mut self, abi: bray_symbols::CallableAbi) {
        self.wire.write_u32(match abi {
            bray_symbols::CallableAbi::Bray => 0,
            bray_symbols::CallableAbi::C => 1,
            bray_symbols::CallableAbi::System => 2,
        });
    }

    fn execution_lane(&mut self, lane: bray_runtime_interface::ExecutionLaneRequirement) {
        self.wire.write_u32(match lane {
            bray_runtime_interface::ExecutionLaneRequirement::Blocking => 0,
            bray_runtime_interface::ExecutionLaneRequirement::Compute => 1,
            bray_runtime_interface::ExecutionLaneRequirement::MainThread => 2,
        });
    }

    fn runtime_reference(&mut self, reference: MirRuntimeReference) {
        let role = bray_runtime_interface::RuntimeAbiRole::ALL
            .iter()
            .position(|role| *role == reference.role())
            .and_then(|index| u32::try_from(index).ok())
            .unwrap_or_else(|| unreachable!("every runtime role has a stable ordinal"));

        self.wire.write_u32(role);
        self.wire.write_u16(reference.abi_version().major());
        self.wire.write_u16(reference.abi_version().minor());
    }

    fn pattern_predicate(
        &mut self,
        predicate: MirPatternPredicate,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        match predicate {
            MirPatternPredicate::Literal(value) => {
                self.wire.write_u32(0);
                self.constant_value(value)?;
            }
            MirPatternPredicate::Constant(term) => {
                self.wire.write_u32(1);
                self.constant_term(term)?;
            }
            MirPatternPredicate::NullableAbsent => self.wire.write_u32(2),
            MirPatternPredicate::NullablePresent => self.wire.write_u32(3),
            MirPatternPredicate::ActiveUnionVariant(variant) => {
                self.wire.write_u32(4);
                self.symbol(variant.into())?;
            }
            MirPatternPredicate::ProductShape(product) => {
                self.wire.write_u32(5);
                self.symbol(product.into())?;
            }
            MirPatternPredicate::TupleShape(arity) => {
                self.wire.write_u32(6);
                self.wire.write_u32(arity);
            }
            MirPatternPredicate::ArrayShape(length) => {
                self.wire.write_u32(7);
                self.wire.write_u32(length);
            }
            MirPatternPredicate::OwnedTarget => self.wire.write_u32(8),
        }

        Ok(())
    }

    fn construction_target(
        &mut self,
        target: ConstructionTarget,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        match target {
            ConstructionTarget::Struct(structure) => {
                self.wire.write_u32(0);
                self.symbol(structure.into())?;
            }
            ConstructionTarget::UnionVariant(variant) => {
                self.wire.write_u32(1);
                self.symbol(variant.into())?;
            }
            ConstructionTarget::TypeForm {
                callable,
                requirement,
                witness,
            } => {
                self.wire.write_u32(2);
                self.callable_instance(callable)?;
                self.implementation_requirement(requirement)?;
                self.implementation(witness)?;
            }
        }

        Ok(())
    }

    fn construction_input(
        &mut self,
        input: &MirConstructionInput,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        match input {
            MirConstructionInput::Explicit {
                input,
                ordinal,
                value,
            } => {
                self.wire.write_u32(0);
                self.construction_input_id(*input)?;
                self.wire.write_u32(*ordinal);
                self.operand(value)?;
            }
            MirConstructionInput::Default {
                input,
                ordinal,
                provider,
            } => {
                self.wire.write_u32(1);
                self.construction_input_id(*input)?;
                self.wire.write_u32(*ordinal);
                self.symbol(provider.symbol())?;
            }
        }

        Ok(())
    }

    fn construction_input_id(
        &mut self,
        input: ConstructionInputId,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        match input {
            ConstructionInputId::StructField(field) => {
                self.wire.write_u32(0);
                self.symbol(field.into())?;
            }
            ConstructionInputId::UnionPayloadField(field) => {
                self.wire.write_u32(1);
                self.symbol(field.into())?;
            }
            ConstructionInputId::CallableParameter(parameter) => {
                self.wire.write_u32(2);
                self.symbol(parameter.into())?;
            }
        }

        Ok(())
    }

    fn conversion(
        &mut self,
        conversion: &SelectedConversion,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        self.ty(conversion.source_type())?;
        self.ty(conversion.target_type())?;

        match conversion.target() {
            ConversionTarget::Identity => self.wire.write_u32(0),
            ConversionTarget::NullablePresent => self.wire.write_u32(6),
            ConversionTarget::BuiltInScalar => self.wire.write_u32(1),
            ConversionTarget::CVariadicPromotion => self.wire.write_u32(5),
            ConversionTarget::Composite(conversions) => {
                self.wire.write_u32(2);
                write_count(&mut self.wire, conversions.len());

                for conversion in conversions.iter() {
                    self.conversion(conversion)?;
                }
            }
            ConversionTarget::Trait {
                member,
                fulfillment,
                requirement,
                witness,
            } => {
                self.wire.write_u32(3);
                self.callable_instance(*member)?;
                self.callable_instance(*fulfillment)?;
                self.implementation_requirement(*requirement)?;
                self.implementation(*witness)?;
            }
            ConversionTarget::TraitConstraint {
                member,
                requirement,
                dispatch,
            } => {
                self.wire.write_u32(4);
                self.callable_instance(*member)?;
                self.implementation_requirement(*requirement)?;
                self.trait_dispatch(*dispatch)?;
            }
        }

        Ok(())
    }

    fn pattern_projection(
        &mut self,
        projection: PatternProjection,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        match projection {
            PatternProjection::ProductField(field) => {
                self.wire.write_u32(0);
                self.symbol(field.into())?;
            }
            PatternProjection::TupleElement(ordinal) => {
                self.wire.write_u32(1);
                self.wire.write_u32(ordinal.raw());
            }
            PatternProjection::ActiveUnionPayloadField { variant, field } => {
                self.wire.write_u32(2);
                self.symbol(variant.into())?;
                self.symbol(field.into())?;
            }
            PatternProjection::ElementFromStart(ordinal) => {
                self.wire.write_u32(3);
                self.wire.write_u32(ordinal.raw());
            }
            PatternProjection::ElementFromEnd(ordinal) => {
                self.wire.write_u32(4);
                self.wire.write_u32(ordinal.raw());
            }
            PatternProjection::NullableValue => self.wire.write_u32(5),
            PatternProjection::OwnedTarget => self.wire.write_u32(6),
        }

        Ok(())
    }

    fn pattern_operation(&mut self, operation: PatternOperation) {
        self.wire.write_u32(match operation {
            PatternOperation::Observe => 0,
            PatternOperation::SharedBorrow => 1,
            PatternOperation::MutableBorrow => 2,
            PatternOperation::Consume => 3,
            PatternOperation::Copy => 4,
            PatternOperation::Recovered => 5,
        });
    }

    fn generator_operation(
        &mut self,
        operation: &MirGeneratorOperation,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        match operation {
            MirGeneratorOperation::Begin {
                kind,
                destination,
                element,
                exact_count,
            } => {
                self.wire.write_u32(0);
                self.generator_kind(*kind);
                self.place(destination)?;
                self.ty(*element)?;

                match exact_count {
                    Some(term) => {
                        self.wire.write_u32(1);
                        self.constant_term(*term)?;
                    }
                    None => self.wire.write_u32(0),
                }
            }
            MirGeneratorOperation::Push { destination, value } => {
                self.wire.write_u32(1);
                self.place(destination)?;
                self.operand(value)?;
            }
            MirGeneratorOperation::Finish { destination } => {
                self.wire.write_u32(2);
                self.place(destination)?;
            }
            MirGeneratorOperation::CleanupBroadcast {
                destination,
                element,
                runtime,
            } => {
                self.wire.write_u32(3);
                self.place(destination)?;
                self.ty(*element)?;
                self.runtime_reference(*runtime);
            }
            MirGeneratorOperation::Destroy {
                destination,
                element,
                runtime,
            } => {
                self.wire.write_u32(4);
                self.place(destination)?;
                self.ty(*element)?;
                self.runtime_reference(*runtime);
            }
        }

        Ok(())
    }

    fn generator_kind(&mut self, kind: MirGeneratorKind) {
        self.wire.write_u32(match kind {
            MirGeneratorKind::Array => 0,
            MirGeneratorKind::General => 1,
        });
    }

    fn memory_operation(
        &mut self,
        operation: &MirMemoryOperation,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        self.memory_kind(operation.kind())?;
        self.operands(operation.operands())?;
        write_count(&mut self.wire, operation.operand_types().len());

        for ty in operation.operand_types() {
            self.ty(*ty)?;
        }

        match operation.result_type() {
            Some(ty) => {
                self.wire.write_u32(1);
                self.ty(ty)?;
            }
            None => self.wire.write_u32(0),
        }

        self.callable_references(operation.inline_assembly_symbols())?;

        Ok(())
    }

    fn memory_kind(
        &mut self,
        kind: CheckedMemoryOperationKind,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        use bray_bound_tree::CheckedMemoryOperationKind as Kind;

        match kind {
            Kind::UninitNew { element } => {
                self.wire.write_u32(48);
                self.ty(element)?;
            }
            Kind::UninitPointer { kind, element } => {
                self.wire.write_u32(49);
                self.memory_address_kind(kind);
                self.ty(element)?;
            }
            Kind::UninitWrite { element } => {
                self.wire.write_u32(50);
                self.ty(element)?;
            }
            Kind::UninitAssumeInitialized { element } => {
                self.wire.write_u32(51);
                self.ty(element)?;
            }
            Kind::UninitMove { element } => {
                self.wire.write_u32(52);
                self.ty(element)?;
            }
            Kind::BorrowFrom { kind, pointee } => {
                self.wire.write_u32(53);
                self.memory_address_kind(kind);
                self.ty(pointee)?;
            }
            Kind::Address { kind, pointee } => {
                self.wire.write_u32(0);
                self.memory_address_kind(kind);
                self.ty(pointee)?;
            }
            Kind::Null { pointee } => {
                self.wire.write_u32(1);
                self.ty(pointee)?;
            }
            Kind::IsNull { pointee } => {
                self.wire.write_u32(2);
                self.ty(pointee)?;
            }
            Kind::Offset { unit, pointee } => {
                self.wire.write_u32(3);

                self.wire.write_u32(match unit {
                    bray_bound_tree::MemoryOffsetUnit::Element => 0,
                    bray_bound_tree::MemoryOffsetUnit::Byte => 1,
                });

                self.ty(pointee)?;
            }
            Kind::Reinterpret { source, target } => {
                self.wire.write_u32(4);
                self.ty(source)?;
                self.ty(target)?;
            }
            Kind::CallableFromPointer { callable } => {
                self.wire.write_u32(54);
                self.ty(callable)?;
            }
            Kind::PointerFromCallable { callable } => {
                self.wire.write_u32(55);
                self.ty(callable)?;
            }
            Kind::Read { pointee, kind } => {
                self.wire.write_u32(5);
                self.ty(pointee)?;

                self.wire.write_u32(match kind {
                    bray_bound_tree::MemoryReadKind::Copy => 0,
                    bray_bound_tree::MemoryReadKind::Move => 1,
                });
            }
            Kind::Write { pointee } => {
                self.wire.write_u32(6);
                self.ty(pointee)?;
            }
            Kind::Copy { pointee, kind } => {
                self.wire.write_u32(7);
                self.ty(pointee)?;

                self.wire.write_u32(match kind {
                    bray_bound_tree::MemoryCopyKind::NonOverlapping => 0,
                    bray_bound_tree::MemoryCopyKind::Overlapping => 1,
                });
            }
            Kind::LayoutQuery { ty, kind } => {
                self.wire.write_u32(8);
                self.ty(ty)?;

                self.wire.write_u32(match kind {
                    bray_bound_tree::MemoryLayoutQueryKind::Size => 0,
                    bray_bound_tree::MemoryLayoutQueryKind::Alignment => 1,
                    bray_bound_tree::MemoryLayoutQueryKind::Stride => 2,
                    bray_bound_tree::MemoryLayoutQueryKind::Layout => 3,
                    bray_bound_tree::MemoryLayoutQueryKind::Trailing => 4,
                });
            }
            Kind::RawAllocate => self.wire.write_u32(9),
            Kind::RawDeallocate => self.wire.write_u32(10),
            Kind::Allocate => self.wire.write_u32(11),
            Kind::Deallocate => self.wire.write_u32(12),
            Kind::RawBufferCapacity => self.wire.write_u32(13),
            Kind::RawBufferInitializedCount => self.wire.write_u32(14),
            Kind::RawBufferPointer => self.wire.write_u32(15),
            Kind::RawBufferInitializedSlice => self.wire.write_u32(16),
            Kind::RawBufferInitializedSliceMut => self.wire.write_u32(17),
            Kind::RawBufferSparePointer { element } => {
                self.wire.write_u32(18);
                self.ty(element)?;
            }
            Kind::RawBufferSetInitializedCount => self.wire.write_u32(19),
            Kind::RawBufferRelease { element } => {
                self.wire.write_u32(20);
                self.ty(element)?;
            }
            Kind::RawBufferReplace { element } => {
                self.wire.write_u32(21);
                self.ty(element)?;
            }
            Kind::ByteBufferFill => self.wire.write_u32(22),
            Kind::RawBufferRelocate { element } => {
                self.wire.write_u32(23);
                self.ty(element)?;
            }
            Kind::ByteBufferRead => self.wire.write_u32(24),
            Kind::SequenceLength => self.wire.write_u32(25),
            Kind::CallbackState { state } => {
                self.wire.write_u32(26);
                self.ty(state)?;
            }
            Kind::ByteBufferCopy => self.wire.write_u32(27),
            Kind::VolatileRead {
                pointee,
                address_space,
                kind,
            } => {
                self.wire.write_u32(28);
                self.ty(pointee)?;

                self.wire.write_u32(match address_space {
                    bray_bound_tree::VolatileAddressSpace::Host => 0,
                    bray_bound_tree::VolatileAddressSpace::Device => 1,
                });

                self.wire.write_u32(match kind {
                    bray_bound_tree::MemoryReadKind::Copy => 0,
                    bray_bound_tree::MemoryReadKind::Move => 1,
                });
            }
            Kind::VolatileWrite {
                pointee,
                address_space,
            } => {
                self.wire.write_u32(29);
                self.ty(pointee)?;

                self.wire.write_u32(match address_space {
                    bray_bound_tree::VolatileAddressSpace::Host => 0,
                    bray_bound_tree::VolatileAddressSpace::Device => 1,
                });
            }
            Kind::ExposeAddress { pointee } => {
                self.wire.write_u32(30);
                self.ty(pointee)?;
            }
            Kind::FromExposedAddress { pointee } => {
                self.wire.write_u32(31);
                self.ty(pointee)?;
            }
            Kind::CompareAddress {
                pointee,
                comparison,
            } => {
                self.wire.write_u32(32);
                self.ty(pointee)?;

                self.wire.write_u32(match comparison {
                    bray_bound_tree::PointerAddressComparison::Equal => 0,
                    bray_bound_tree::PointerAddressComparison::Less => 1,
                });
            }
            Kind::Fence {
                compiler_only,
                order,
            } => {
                self.wire.write_u32(33);
                write_bool(&mut self.wire, compiler_only);
                self.wire.write_u32(order.to_u32());
            }
            Kind::CatastrophicAbort => self.wire.write_u32(34),
            Kind::DebuggerTrap => self.wire.write_u32(35),
            Kind::UnreachableTermination => self.wire.write_u32(36),
            Kind::SpinLoopHint => self.wire.write_u32(37),
            Kind::TargetFeatureEnabled { feature } => {
                self.wire.write_u32(38);
                self.constant_value(feature)?;
            }
            Kind::InlineAssembly {
                inputs,
                output,
                labels,
                contract,
            } => self.inline_assembly_kind(inputs, output, labels, contract)?,
            Kind::AtomicInitialize { value } => self.atomic_value_kind(40, value)?,
            Kind::AtomicLoad { value, order } => {
                self.atomic_ordered_value_kind(41, value, order)?
            }
            Kind::AtomicStore { value, order } => {
                self.atomic_ordered_value_kind(42, value, order)?;
            }
            Kind::AtomicExchange { value, order } => {
                self.atomic_ordered_value_kind(43, value, order)?;
            }
            Kind::AtomicCompareExchange {
                value,
                weak,
                success,
                failure,
            } => self.atomic_compare_exchange_kind(value, weak, success, failure)?,
            Kind::AtomicFetch { value, kind, order } => {
                self.atomic_fetch_kind(value, kind, order)?;
            }
            Kind::AtomicWait { value, order } => {
                self.atomic_ordered_value_kind(46, value, order)?;
            }
            Kind::AtomicNotify { value, all } => self.atomic_notify_kind(value, all)?,
        }

        Ok(())
    }

    fn inline_assembly_kind(
        &mut self,
        inputs: TypeId,
        output: Option<TypeId>,
        labels: Option<TypeId>,
        contract: InlineAssemblyContract,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        self.wire.write_u32(39);
        self.ty(inputs)?;

        match output {
            Some(output) => {
                self.wire.write_u32(1);
                self.ty(output)?;
            }
            None => self.wire.write_u32(0),
        }

        match labels {
            Some(labels) => {
                self.wire.write_u32(1);
                self.ty(labels)?;
            }
            None => self.wire.write_u32(0),
        }

        self.inline_assembly_contract(contract)
    }

    fn atomic_value_kind(
        &mut self,
        tag: u32,
        value: TypeId,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        self.wire.write_u32(tag);

        self.ty(value)
    }

    fn atomic_ordered_value_kind(
        &mut self,
        tag: u32,
        value: TypeId,
        order: bray_bound_tree::MemoryOrder,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        self.atomic_value_kind(tag, value)?;
        self.atomic_order(order);

        Ok(())
    }

    fn atomic_compare_exchange_kind(
        &mut self,
        value: TypeId,
        weak: bool,
        success: bray_bound_tree::MemoryOrder,
        failure: bray_bound_tree::MemoryOrder,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        self.atomic_value_kind(44, value)?;
        write_bool(&mut self.wire, weak);
        self.atomic_order(success);
        self.atomic_order(failure);

        Ok(())
    }

    fn atomic_fetch_kind(
        &mut self,
        value: TypeId,
        kind: bray_bound_tree::AtomicFetchKind,
        order: bray_bound_tree::MemoryOrder,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        self.atomic_value_kind(45, value)?;

        self.wire.write_u32(match kind {
            bray_bound_tree::AtomicFetchKind::Add => 0,
            bray_bound_tree::AtomicFetchKind::Subtract => 1,
            bray_bound_tree::AtomicFetchKind::And => 2,
            bray_bound_tree::AtomicFetchKind::Or => 3,
            bray_bound_tree::AtomicFetchKind::Xor => 4,
        });

        self.atomic_order(order);

        Ok(())
    }

    fn atomic_notify_kind(
        &mut self,
        value: TypeId,
        all: bool,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        self.atomic_value_kind(47, value)?;
        write_bool(&mut self.wire, all);

        Ok(())
    }

    fn inline_assembly_contract(
        &mut self,
        contract: InlineAssemblyContract,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        for value in contract.constant_values() {
            self.constant_value(value)?;
        }

        let operands = contract.operands().collect::<Vec<_>>();
        write_count(&mut self.wire, operands.len());

        for operand in operands {
            self.wire.write_u32(match operand.kind() {
                InlineAssemblyOperandKind::Input => 0,
                InlineAssemblyOperandKind::LateOutput => 1,
                InlineAssemblyOperandKind::Output => 2,
                InlineAssemblyOperandKind::InOut => 3,
                InlineAssemblyOperandKind::EarlyInOut => 4,
                InlineAssemblyOperandKind::Immediate => 5,
                InlineAssemblyOperandKind::Symbol => 6,
                InlineAssemblyOperandKind::Memory => 7,
                InlineAssemblyOperandKind::Label => 8,
            });

            self.ty(operand.ty())?;
            write_optional(&mut self.wire, operand.input(), WireEncoder::write_u16);

            write_optional(
                &mut self.wire,
                operand.runtime_input(),
                WireEncoder::write_u16,
            );

            write_optional(&mut self.wire, operand.output(), WireEncoder::write_u16);

            match operand.constant() {
                Some(constant) => {
                    self.wire.write_u32(1);
                    self.constant_value(constant)?;
                }
                None => self.wire.write_u32(0),
            }

            let (start, length) = operand.constraint_range();

            self.wire.write_u16(start);
            self.wire.write_u16(length);
        }

        Ok(())
    }

    fn callable_references(
        &mut self,
        references: &[bray_ir::MirCallableReference],
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        write_count(&mut self.wire, references.len());

        for reference in references {
            self.callable_instance(reference.instance())?;
            self.callable_abi(reference.abi());
        }

        Ok(())
    }

    fn atomic_order(&mut self, order: bray_bound_tree::MemoryOrder) {
        self.wire.write_u32(order.to_u32());
    }

    fn memory_address_kind(&mut self, kind: bray_bound_tree::MemoryAddressKind) {
        self.wire.write_u32(match kind {
            bray_bound_tree::MemoryAddressKind::Shared => 0,
            bray_bound_tree::MemoryAddressKind::Mutable => 1,
        });
    }

    fn text_operation(
        &mut self,
        operation: &MirTextOperation,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        self.text_kind(operation.kind());
        self.operands(operation.operands())?;
        write_count(&mut self.wire, operation.operand_types().len());

        for ty in operation.operand_types() {
            self.ty(*ty)?;
        }

        match operation.result_type() {
            Some(ty) => {
                self.wire.write_u32(1);
                self.ty(ty)?;
            }
            None => self.wire.write_u32(0),
        }

        Ok(())
    }

    fn text_kind(&mut self, kind: MirTextOperationKind) {
        self.wire.write_u32(match kind {
            MirTextOperationKind::ScalarCount => 0,
            MirTextOperationKind::IsEmpty => 1,
            MirTextOperationKind::Equals => 2,
            MirTextOperationKind::ScalarAt => 3,
            MirTextOperationKind::ScalarSlice => 4,
            MirTextOperationKind::Utf8 => 5,
            MirTextOperationKind::FromUtf8 => 6,
            MirTextOperationKind::CharacterScalarValue => 7,
            MirTextOperationKind::CharacterFromScalarValue => 8,
            MirTextOperationKind::CharacterUtf8Length => 9,
            MirTextOperationKind::CharacterUtf8Byte => 10,
            MirTextOperationKind::CharacterIsAlphabetic => 11,
            MirTextOperationKind::CharacterIsNumeric => 12,
            MirTextOperationKind::CharacterIsWhitespace => 13,
            MirTextOperationKind::Release => 14,
        });
    }

    fn panic_cause(
        &mut self,
        cause: &MirPanicCause,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        match cause {
            MirPanicCause::Message(message) => {
                self.wire.write_u32(0);
                self.operand(message)?;
            }
            MirPanicCause::Assertion(message) => {
                self.wire.write_u32(1);
                self.optional_operand(message.as_ref())?;
            }
            MirPanicCause::ExplicitTestFailure(message) => {
                self.wire.write_u32(2);
                self.operand(message)?;
            }
        }

        Ok(())
    }

    fn async_operation(
        &mut self,
        operation: &bray_ir::MirAsyncOperation,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        use bray_ir::MirAsyncOperation as Operation;

        match operation {
            Operation::CreateFrame { frame, initializer } => {
                self.wire.write_u32(0);
                self.frame_reference(*frame);

                match initializer {
                    bray_ir::MirFrameInitializer::Callable(call) => {
                        self.wire.write_u32(0);
                        self.call(call)?;
                    }
                    bray_ir::MirFrameInitializer::TaskObservation {
                        task,
                        result,
                        variants,
                        runtime,
                        request_cancellation,
                    } => {
                        self.wire.write_u32(1);
                        self.operand(task)?;
                        self.ty(result.completion_type())?;
                        self.ty(result.future_type())?;
                        self.run_result_variants(*variants)?;
                        self.runtime_reference(*runtime);
                        write_bool(&mut self.wire, *request_cancellation);
                    }
                }
            }
            Operation::MoveInactiveFrame {
                frame,
                source,
                destination,
            } => {
                self.wire.write_u32(1);
                self.frame_reference(*frame);
                self.place(source)?;
                self.place(destination)?;
            }
            Operation::ResumeFrame {
                frame,
                state,
                storage,
                runtime,
            } => {
                self.wire.write_u32(2);
                self.wire.write_bytes(&frame.digest());
                self.wire.write_u32(state.raw());
                self.wire.write_u32(storage.slot());
                self.runtime_reference(*runtime);
            }
            Operation::ComposeAwaitedFrame {
                parent,
                child,
                frame,
            } => {
                self.wire.write_u32(3);
                self.wire.write_bytes(&parent.digest());
                self.frame_reference(*child);
                self.operand(frame)?;
            }
            Operation::CommitAwaitedCompletion { child } => {
                self.wire.write_u32(4);
                self.frame_reference(*child);
            }
            Operation::StartTask {
                frame,
                value,
                allocation,
                start,
            } => {
                self.wire.write_u32(5);
                self.frame_reference(*frame);
                self.operand(value)?;
                self.runtime_reference(*allocation);
                self.runtime_reference(*start);
            }
            Operation::RequestTaskCancellation { task, runtime } => {
                self.wire.write_u32(6);
                self.operand(task)?;
                self.runtime_reference(*runtime);
            }
            Operation::ObserveCurrentRunCancellation { runtime } => {
                self.wire.write_u32(7);
                self.runtime_reference(*runtime);
            }
            Operation::ResolveTask {
                task,
                variants,
                runtime,
            } => {
                self.wire.write_u32(8);
                self.operand(task)?;
                self.run_result_variants(*variants)?;
                self.runtime_reference(*runtime);
            }
            Operation::PublishTerminalState { state, runtime } => {
                self.wire.write_u32(9);

                match state {
                    bray_ir::MirTaskTerminalState::Completed(value) => {
                        self.wire.write_u32(0);
                        self.operand(value)?;
                    }
                    bray_ir::MirTaskTerminalState::Cancelled => self.wire.write_u32(1),
                    bray_ir::MirTaskTerminalState::Panicked(report) => {
                        self.wire.write_u32(2);
                        self.operand(report)?;
                    }
                }

                self.runtime_reference(*runtime);
            }
            Operation::ExecuteCleanupBroadcast { frame, runtime } => {
                self.wire.write_u32(10);
                self.wire.write_bytes(&frame.digest());
                self.runtime_reference(*runtime);
            }
            Operation::ExecuteLifecycleResolution { frame, runtime } => {
                self.wire.write_u32(11);
                self.wire.write_bytes(&frame.digest());
                self.runtime_reference(*runtime);
            }
            Operation::TransferCleanupIncident { incident, runtime } => {
                self.wire.write_u32(12);
                self.operand(incident)?;
                self.runtime_reference(*runtime);
            }
            Operation::DestroyTerminalTask { task } => {
                self.wire.write_u32(13);
                self.operand(task)?;
            }
        }

        Ok(())
    }

    fn run_result_variants(
        &mut self,
        variants: bray_ir::MirRunResultVariants,
    ) -> Result<(), ExecutableTemplateEncodeError<C::Error>> {
        self.symbol(variants.completed().into())?;
        self.symbol(variants.panicked().into())?;

        self.symbol(variants.cancelled().into())
    }

    fn frame_reference(&mut self, reference: bray_ir::MirFrameReference) {
        match reference {
            bray_ir::MirFrameReference::Known(frame) => {
                self.wire.write_u32(0);
                self.wire.write_bytes(&frame.digest());
            }
            bray_ir::MirFrameReference::Erased => self.wire.write_u32(1),
        }
    }
}
