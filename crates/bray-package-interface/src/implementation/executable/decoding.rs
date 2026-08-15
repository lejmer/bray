// rust-style: allow(module-too-large, reason = "the executable MIR wire decoder keeps one exhaustive operation and terminator mapping")

use bray_bound_tree::{
    BoundCallResult, BoundFutureConstruction, CheckedMemoryOperationKind,
    ConstructionDefaultProvider, ConstructionInputId, ConstructionTarget, ConversionTarget,
    InlineAssemblyContract, InlineAssemblyOperand, InlineAssemblyOperandKind,
    MAX_INLINE_ASSEMBLY_OPERANDS, MemoryOrder, PatternOperation, PatternProjection,
    SelectedConversion, SelectedImplementationWitness,
};
use bray_ir::{
    MirAggregate, MirAggregateKind, MirAnonymousCallableReference, MirBinaryOperator, MirBlockId,
    MirBlockKind, MirCall, MirCallArgument, MirCallTarget, MirCallableReference, MirCleanupEdge,
    MirCleanupPhase, MirConstruction, MirConstructionInput, MirEdge, MirExecutableTemplateId,
    MirFrameDescriptor, MirFrameReference, MirFrameStateFacts, MirGeneratorKind,
    MirGeneratorOperation, MirImmediateValue, MirImportedExecutableKey, MirMemoryOperation,
    MirNumericConversionKind, MirOperand, MirOperationKind, MirPanicCause, MirPatternPredicate,
    MirPlace, MirProjection, MirProjectionKind, MirRuntimeReference, MirSourceAnchor, MirStorageId,
    MirStorageKind, MirStoreKind, MirSwitchCase, MirTargetFacts, MirTerminatorKind,
    MirTextOperation, MirTextOperationKind, MirUnaryOperator, MirUnit, MirUnitBuildError,
    MirUnitBuilder, MirUnitId, MirUnitKind, MirValueId,
};
use bray_runtime_interface::{
    ExecutionLaneRequirement, ProtectedAsyncFrameId, ProtectedFrameAbiOperation,
    ProtectedFrameAbiVersions, RuntimeAbiRole, RuntimeAbiVersion,
};
use bray_symbols::{
    AnySymbolId, CallableCapabilityRequirement, CallableDefinitionId, CallableEffectRequirement,
    CallableExecutionRequirement, CallableInstanceData, CallablePhaseBehavior,
    CallablePhaseBehaviors, CurrentRunCancellation, ExactSymbolId, GenericOwnerId,
    ImplementationRequirementKey, LifecycleObligationKind, SymbolOrdinal, TraitConstraintDispatch,
    TrustedCapabilityRequirement,
};

use crate::decode::map_wire_error;
use crate::semantic::{SemanticDecodeContext, read_symbol_reference};
use crate::wire::WireReader;
use crate::{
    ImportedSemanticFacts, InterfaceExecutableTemplate, InterfaceSymbolResolver,
    InterfaceConstantValueKind, InterfaceValidationError, InterfaceValidationLimits,
};
use bray_target::InlineAssemblyOptions;

use super::support::{FORMAT_VERSION, read_bool, read_count, read_optional, read_u32};

/// Failure while reconstructing one imported executable template.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExecutableTemplateDecodeError {
    /// The encoded template is malformed or references absent interface facts.
    Malformed,
    /// The encoded template was lowered for a different target contract.
    TargetMismatch,
    /// The encoded template violated package-interface validation policy.
    Validation(InterfaceValidationError),
    /// The reconstructed MIR failed ordinary unit validation.
    InvalidMir(MirUnitBuildError),
}

/// Reconstructs one source-independent executable template for a consumer target.
pub fn decode_executable_template(
    template: &InterfaceExecutableTemplate,
    owner: AnySymbolId,
    unit: MirUnitId,
    target: MirTargetFacts,
    facts: &ImportedSemanticFacts,
    symbols: &impl InterfaceSymbolResolver,
    limits: InterfaceValidationLimits,
) -> Result<MirUnit, ExecutableTemplateDecodeError> {
    let mut decoder = Decoder {
        reader: WireReader::new(template.payload()),
        semantic: SemanticDecodeContext::new(limits),
        facts,
        symbols,
        unit,
        owner,
        identity: template.identity(),
        family_size: template.family_size(),
    };

    if read_u32(&mut decoder.reader)? != FORMAT_VERSION {
        return Err(ExecutableTemplateDecodeError::Malformed);
    }

    decoder.require_target(&target)?;

    let kind = decoder.unit_kind()?;
    let entry_slot = read_u32(&mut decoder.reader)?;
    let key = MirImportedExecutableKey::new(owner, template.identity());
    let source = MirSourceAnchor::imported_executable(key);
    let mut builder = MirUnitBuilder::for_imported_executable(unit, key, kind, target);

    let block_count = decoder.count()?;
    let mut block_records = decoder.items(block_count)?;
    let mut blocks = decoder.derived_items(block_count)?;

    decoder.charge_items::<bray_ir::MirBlock>(block_count)?;

    for _ in 0..block_count {
        let kind = decoder.block_kind()?;
        let parameter_slots = decoder.slots()?;
        let operation_slots = decoder.slots()?;

        let block = builder
            .push_block(source.clone(), kind)
            .map_err(ExecutableTemplateDecodeError::InvalidMir)?;

        blocks.push(block);

        block_records.push(BlockRecord {
            parameter_slots,
            operation_slots,
        });
    }

    let storage_count = decoder.count()?;
    let mut storages = decoder.items(storage_count)?;

    decoder.charge_items::<bray_ir::MirStorage>(storage_count)?;

    for _ in 0..storage_count {
        let kind = decoder.storage_kind()?;
        let ty = decoder.ty()?;

        let storage = builder
            .push_storage(source.clone(), kind, ty)
            .map_err(ExecutableTemplateDecodeError::InvalidMir)?;

        storages.push(storage);
    }

    let value_count = decoder.count()?;
    let mut values = decoder.items(value_count)?;

    decoder.charge_items::<bray_ir::MirValue>(value_count)?;

    for _ in 0..value_count {
        let ty = decoder.ty()?;

        let origin = match read_u32(&mut decoder.reader)? {
            0 => ValueRecordOrigin::Block(read_u32(&mut decoder.reader)?),
            1 => ValueRecordOrigin::Operation(read_u32(&mut decoder.reader)?),
            _ => return Err(ExecutableTemplateDecodeError::Malformed),
        };

        values.push(ValueRecord { ty, origin });
    }

    let operation_count = decoder.count()?;
    let mut operations = decoder.items(operation_count)?;

    decoder.charge_items::<bray_ir::MirOperation>(operation_count)?;

    for _ in 0..operation_count {
        let result = read_optional(&mut decoder.reader, read_u32)?;
        let kind = decoder.operation()?;

        operations.push(OperationRecord { result, kind });
    }

    let mut terminators = decoder.items(block_count)?;

    for _ in 0..block_count {
        terminators.push(decoder.terminator()?);
    }

    let frame = decoder.frame_descriptor()?;

    let mut seen_parameters = decoder.derived_items(values.len())?;
    seen_parameters.resize(values.len(), false);

    let mut operation_owner_slots = decoder.derived_items(operation_count)?;
    operation_owner_slots.resize(operation_count, None);

    decoder
        .reader
        .finish()
        .map_err(|_| ExecutableTemplateDecodeError::Malformed)?;

    validate_block_parameters(&block_records, &values, seen_parameters)?;

    let operation_owners = operation_owners(&block_records, operation_owner_slots)?;
    let mut next_operation = 0;

    for (value_slot, value) in values.iter().enumerate() {
        match value.origin {
            ValueRecordOrigin::Block(block_slot) => {
                let block = item(&blocks, block_slot)?;

                let id = builder
                    .push_block_parameter(block, source.clone(), value.ty)
                    .map_err(ExecutableTemplateDecodeError::InvalidMir)?;

                require_slot(id.slot(), value_slot)?;
            }
            ValueRecordOrigin::Operation(operation_slot) => {
                let operation_index = index(operation_slot)?;

                while next_operation < operation_index {
                    push_operation(
                        &mut builder,
                        source.clone(),
                        &blocks,
                        &operation_owners,
                        &operations,
                        next_operation,
                        None,
                    )?;

                    next_operation += 1;
                }

                if operation_index != next_operation
                    || operations
                        .get(operation_index)
                        .and_then(|operation| operation.result)
                        != Some(
                            u32::try_from(value_slot)
                                .map_err(|_| ExecutableTemplateDecodeError::Malformed)?,
                        )
                {
                    return Err(ExecutableTemplateDecodeError::Malformed);
                }

                let commit = push_operation(
                    &mut builder,
                    source.clone(),
                    &blocks,
                    &operation_owners,
                    &operations,
                    next_operation,
                    Some(value.ty),
                )?;

                require_slot(
                    commit
                        .result()
                        .ok_or(ExecutableTemplateDecodeError::Malformed)?
                        .slot(),
                    value_slot,
                )?;

                next_operation += 1;
            }
        }
    }

    while next_operation < operations.len() {
        push_operation(
            &mut builder,
            source.clone(),
            &blocks,
            &operation_owners,
            &operations,
            next_operation,
            None,
        )?;

        next_operation += 1;
    }

    for ((block, terminator), record) in blocks.iter().copied().zip(terminators).zip(&block_records)
    {
        validate_block_record(record)?;

        builder
            .set_terminator(block, source.clone(), terminator)
            .map_err(ExecutableTemplateDecodeError::InvalidMir)?;
    }

    if let Some(frame) = frame {
        builder
            .set_frame_descriptor(frame)
            .map_err(ExecutableTemplateDecodeError::InvalidMir)?;
    }

    let entry = item(&blocks, entry_slot)?;

    builder
        .finish(entry)
        .map_err(ExecutableTemplateDecodeError::InvalidMir)
}

struct BlockRecord {
    parameter_slots: Vec<u32>,
    operation_slots: Vec<u32>,
}

struct ValueRecord {
    ty: bray_symbols::TypeId,
    origin: ValueRecordOrigin,
}

enum ValueRecordOrigin {
    Block(u32),
    Operation(u32),
}

struct OperationRecord {
    result: Option<u32>,
    kind: MirOperationKind,
}

#[derive(Clone, Copy)]
enum AssemblyConstantRole {
    Text,
    Integer,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum AssemblyConstantPayload {
    Text(std::sync::Arc<str>),
    Integer(u64),
}

fn assembly_constant_payload(
    role: AssemblyConstantRole,
    kind: &InterfaceConstantValueKind,
) -> Option<AssemblyConstantPayload> {
    match (role, kind) {
        (AssemblyConstantRole::Text, InterfaceConstantValueKind::String(text)) => {
            Some(AssemblyConstantPayload::Text(text.clone()))
        }
        (AssemblyConstantRole::Integer, InterfaceConstantValueKind::Integer(integer)) => {
            integer.to_u64().map(AssemblyConstantPayload::Integer)
        }
        _ => None,
    }
}

fn assembly_options_valid(bits: u64, has_normal_output: bool) -> bool {
    InlineAssemblyOptions::try_new(bits)
        .is_some_and(|options| {
            !options.may_unwind() && (has_normal_output || !options.pure())
        })
}

fn assembly_type_counts(contract: InlineAssemblyContract) -> (usize, usize, usize) {
    let mut inputs = 0_usize;
    let mut outputs = 0_usize;
    let mut labels = 0_usize;

    for operand in contract.operands() {
        inputs += usize::from(operand.runtime_input().is_some());
        outputs += usize::from(operand.output().is_some());
        labels += usize::from(operand.kind() == InlineAssemblyOperandKind::Label);
    }

    (inputs, outputs, labels)
}

struct Decoder<'data, 'facts, R> {
    reader: WireReader<'data>,
    semantic: SemanticDecodeContext,
    facts: &'facts ImportedSemanticFacts,
    symbols: &'facts R,
    unit: MirUnitId,
    owner: AnySymbolId,
    identity: MirExecutableTemplateId,
    family_size: u32,
}

fn operation_owners(
    blocks: &[BlockRecord],
    mut owners: Vec<Option<u32>>,
) -> Result<Vec<u32>, ExecutableTemplateDecodeError> {
    for (block, record) in blocks.iter().enumerate() {
        let block = u32::try_from(block).map_err(|_| ExecutableTemplateDecodeError::Malformed)?;

        for operation in &record.operation_slots {
            let owner = owners
                .get_mut(index(*operation)?)
                .ok_or(ExecutableTemplateDecodeError::Malformed)?;

            if owner.replace(block).is_some() {
                return Err(ExecutableTemplateDecodeError::Malformed);
            }
        }
    }

    owners
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(ExecutableTemplateDecodeError::Malformed)
}

fn validate_block_parameters(
    blocks: &[BlockRecord],
    values: &[ValueRecord],
    mut seen: Vec<bool>,
) -> Result<(), ExecutableTemplateDecodeError> {
    for (block, record) in blocks.iter().enumerate() {
        let block = u32::try_from(block).map_err(|_| ExecutableTemplateDecodeError::Malformed)?;

        for parameter in &record.parameter_slots {
            let parameter = index(*parameter)?;

            let value = values
                .get(parameter)
                .ok_or(ExecutableTemplateDecodeError::Malformed)?;

            if seen.get(parameter).copied() != Some(false)
                || !matches!(value.origin, ValueRecordOrigin::Block(owner) if owner == block)
            {
                return Err(ExecutableTemplateDecodeError::Malformed);
            }

            seen[parameter] = true;
        }
    }

    if values
        .iter()
        .zip(seen)
        .any(|(value, seen)| matches!(value.origin, ValueRecordOrigin::Block(_)) != seen)
    {
        return Err(ExecutableTemplateDecodeError::Malformed);
    }

    Ok(())
}

fn push_operation(
    builder: &mut MirUnitBuilder,
    source: MirSourceAnchor,
    blocks: &[MirBlockId],
    owners: &[u32],
    operations: &[OperationRecord],
    operation: usize,
    result_type: Option<bray_symbols::TypeId>,
) -> Result<bray_ir::MirOperationCommit, ExecutableTemplateDecodeError> {
    let record = operations
        .get(operation)
        .ok_or(ExecutableTemplateDecodeError::Malformed)?;

    if record.result.is_some() != result_type.is_some() {
        return Err(ExecutableTemplateDecodeError::Malformed);
    }

    let owner = item(
        blocks,
        *owners
            .get(operation)
            .ok_or(ExecutableTemplateDecodeError::Malformed)?,
    )?;

    let commit = builder
        .push_operation(owner, source, record.kind.clone(), result_type)
        .map_err(ExecutableTemplateDecodeError::InvalidMir)?;

    require_slot(commit.operation().slot(), operation)?;

    Ok(commit)
}

fn validate_block_record(record: &BlockRecord) -> Result<(), ExecutableTemplateDecodeError> {
    if !record
        .parameter_slots
        .windows(2)
        .all(|pair| pair[0] < pair[1])
        || !record
            .operation_slots
            .windows(2)
            .all(|pair| pair[0] < pair[1])
    {
        return Err(ExecutableTemplateDecodeError::Malformed);
    }

    Ok(())
}

fn index(slot: u32) -> Result<usize, ExecutableTemplateDecodeError> {
    usize::try_from(slot).map_err(|_| ExecutableTemplateDecodeError::Malformed)
}

fn item<T: Copy>(items: &[T], slot: u32) -> Result<T, ExecutableTemplateDecodeError> {
    items
        .get(index(slot)?)
        .copied()
        .ok_or(ExecutableTemplateDecodeError::Malformed)
}

fn require_slot(actual: u32, expected: usize) -> Result<(), ExecutableTemplateDecodeError> {
    if usize::try_from(actual).ok() == Some(expected) {
        Ok(())
    } else {
        Err(ExecutableTemplateDecodeError::Malformed)
    }
}

impl From<InterfaceValidationError> for ExecutableTemplateDecodeError {
    fn from(error: InterfaceValidationError) -> Self {
        Self::Validation(error)
    }
}

impl<R: InterfaceSymbolResolver> Decoder<'_, '_, R> {
    fn count(&mut self) -> Result<usize, InterfaceValidationError> {
        read_count(&mut self.reader, self.semantic.limits())
    }

    fn items<T>(&mut self, count: usize) -> Result<Vec<T>, ExecutableTemplateDecodeError> {
        self.semantic
            .allocate_items(&self.reader, count)
            .map_err(Into::into)
    }

    fn derived_items<T>(&mut self, count: usize) -> Result<Vec<T>, ExecutableTemplateDecodeError> {
        self.semantic
            .allocate_derived_items(count)
            .map_err(Into::into)
    }

    fn charge_items<T>(&mut self, count: usize) -> Result<(), ExecutableTemplateDecodeError> {
        self.semantic.charge_items::<T>(count).map_err(Into::into)
    }

    fn require_target(
        &mut self,
        target: &MirTargetFacts,
    ) -> Result<(), ExecutableTemplateDecodeError> {
        let digest = self.reader.read_array::<32>().map_err(map_wire_error)?;

        if digest != target.compatibility_digest() {
            return Err(ExecutableTemplateDecodeError::TargetMismatch);
        }

        Ok(())
    }

    fn operation(&mut self) -> Result<MirOperationKind, ExecutableTemplateDecodeError> {
        match read_u32(&mut self.reader)? {
            1 => Ok(MirOperationKind::Store {
                kind: self.store_kind()?,
                destination: self.place()?,
                value: self.operand()?,
            }),
            2 => Ok(MirOperationKind::Borrow {
                kind: self.borrow_kind()?,
                place: self.place()?,
            }),
            3 => Ok(MirOperationKind::Unary {
                operator: self.unary_operator()?,
                operand: self.operand()?,
            }),
            4 => Ok(MirOperationKind::Binary {
                operator: self.binary_operator()?,
                left: self.operand()?,
                right: self.operand()?,
            }),
            5 => Ok(MirOperationKind::Aggregate(MirAggregate::new(
                self.aggregate_kind()?,
                self.operands()?,
            ))),
            6 => {
                let target = self.construction_target()?;
                let count = self.count()?;
                let mut inputs = self.items(count)?;

                for _ in 0..count {
                    inputs.push(self.construction_input()?);
                }

                Ok(MirOperationKind::Construct(MirConstruction::new(
                    target, inputs,
                )))
            }
            7 => Ok(MirOperationKind::Convert {
                operand: self.operand()?,
                conversion: self.conversion()?,
            }),
            8 => Ok(MirOperationKind::NumericConversion {
                kind: self.numeric_conversion_kind()?,
                operand: self.operand()?,
            }),
            9 => Ok(MirOperationKind::PatternProjection {
                subject: self.operand()?,
                projection: self.pattern_projection()?,
                operation: self.pattern_operation()?,
            }),
            10 => Ok(MirOperationKind::Generator(self.generator_operation()?)),
            11 => Ok(MirOperationKind::Call(self.call()?)),
            12 => Ok(MirOperationKind::Memory(self.memory_operation()?)),
            13 => Ok(MirOperationKind::Text(self.text_operation()?)),
            14 => Ok(MirOperationKind::PanicReport(self.panic_cause()?)),
            15 => Ok(MirOperationKind::Finalize(self.place()?)),
            16 => Ok(MirOperationKind::Destroy(self.place()?)),
            17 => Ok(MirOperationKind::Cleanup {
                phase: self.cleanup_phase()?,
                place: self.place()?,
            }),
            18 => Ok(MirOperationKind::Async(self.async_operation()?)),
            19 => Ok(MirOperationKind::DeclaredCallable(
                MirCallableReference::new(self.callable_instance()?, self.callable_abi()?),
            )),
            20 => {
                let identity = MirExecutableTemplateId::new(read_u32(&mut self.reader)?);

                if identity.raw() <= self.identity.raw() || identity.raw() >= self.family_size {
                    return Err(ExecutableTemplateDecodeError::Malformed);
                }

                Ok(MirOperationKind::AnonymousCallable(
                    MirAnonymousCallableReference::imported(MirImportedExecutableKey::new(
                        self.owner, identity,
                    )),
                ))
            }
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn operand(&mut self) -> Result<MirOperand, ExecutableTemplateDecodeError> {
        match read_u32(&mut self.reader)? {
            0 => Ok(MirOperand::Value(self.value_id()?)),
            1 => Ok(MirOperand::Constant {
                value: self.constant_value()?,
                ty: self.ty()?,
            }),
            2 => Ok(MirOperand::Immediate {
                value: self.immediate()?,
                ty: self.ty()?,
            }),
            3 => Ok(MirOperand::Copy(self.place()?)),
            4 => Ok(MirOperand::Move(self.place()?)),
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn operands(&mut self) -> Result<Vec<MirOperand>, ExecutableTemplateDecodeError> {
        let count = self.count()?;
        let mut operands = self.items(count)?;

        for _ in 0..count {
            operands.push(self.operand()?);
        }

        Ok(operands)
    }

    fn optional_operand(&mut self) -> Result<Option<MirOperand>, ExecutableTemplateDecodeError> {
        match read_u32(&mut self.reader)? {
            0 => Ok(None),
            1 => self.operand().map(Some),
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn place(&mut self) -> Result<MirPlace, ExecutableTemplateDecodeError> {
        let storage = self.storage_id()?;
        let ty = self.ty()?;
        let count = self.count()?;
        let mut projections = self.items(count)?;

        for _ in 0..count {
            let kind = self.projection_kind()?;
            let source_type = self.ty()?;
            let result_type = self.ty()?;

            projections.push(MirProjection::new(kind, source_type, result_type));
        }

        Ok(MirPlace::new(storage, projections, ty))
    }

    fn projection_kind(&mut self) -> Result<MirProjectionKind, ExecutableTemplateDecodeError> {
        match read_u32(&mut self.reader)? {
            0 => Ok(MirProjectionKind::Dereference),
            1 => {
                let symbol = self.symbol()?;

                let field = match symbol {
                    AnySymbolId::StructField(field) => bray_ir::MirFieldReference::Struct(field),
                    AnySymbolId::UnionPayloadField(field) => {
                        bray_ir::MirFieldReference::UnionPayload(field)
                    }
                    _ => return Err(ExecutableTemplateDecodeError::Malformed),
                };

                Ok(MirProjectionKind::Field(field))
            }
            2 => Ok(MirProjectionKind::TupleField(read_u32(&mut self.reader)?)),
            3 => Ok(MirProjectionKind::ElementFromStart(read_u32(
                &mut self.reader,
            )?)),
            4 => Ok(MirProjectionKind::ElementFromEnd(read_u32(
                &mut self.reader,
            )?)),
            5 => Ok(MirProjectionKind::Index(self.operand()?)),
            6 => Ok(MirProjectionKind::Slice {
                start: self.optional_operand()?,
                end: self.optional_operand()?,
            }),
            7 => Ok(MirProjectionKind::Variant(self.exact_symbol()?)),
            8 => Ok(MirProjectionKind::ActiveUnionPayloadField {
                variant: self.exact_symbol()?,
                field: self.exact_symbol()?,
            }),
            9 => Ok(MirProjectionKind::NullableValue),
            10 => Ok(MirProjectionKind::OwnedStorage),
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn immediate(&mut self) -> Result<MirImmediateValue, ExecutableTemplateDecodeError> {
        match read_u32(&mut self.reader)? {
            0 => Ok(MirImmediateValue::Boolean(false)),
            1 => Ok(MirImmediateValue::Boolean(true)),
            2 => Ok(MirImmediateValue::Unit),
            3 => Ok(MirImmediateValue::NullableAbsent),
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn store_kind(&mut self) -> Result<MirStoreKind, ExecutableTemplateDecodeError> {
        match read_u32(&mut self.reader)? {
            0 => Ok(MirStoreKind::Initialize),
            1 => Ok(MirStoreKind::Assign),
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn borrow_kind(&mut self) -> Result<bray_symbols::BorrowKind, ExecutableTemplateDecodeError> {
        match read_u32(&mut self.reader)? {
            0 => Ok(bray_symbols::BorrowKind::Shared),
            1 => Ok(bray_symbols::BorrowKind::Mutable),
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn unary_operator(&mut self) -> Result<MirUnaryOperator, ExecutableTemplateDecodeError> {
        match read_u32(&mut self.reader)? {
            0 => Ok(MirUnaryOperator::Negate),
            1 => Ok(MirUnaryOperator::Not),
            2 => Ok(MirUnaryOperator::BitwiseNot),
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn binary_operator(&mut self) -> Result<MirBinaryOperator, ExecutableTemplateDecodeError> {
        match read_u32(&mut self.reader)? {
            0 => Ok(MirBinaryOperator::Add),
            1 => Ok(MirBinaryOperator::Subtract),
            2 => Ok(MirBinaryOperator::Multiply),
            3 => Ok(MirBinaryOperator::Divide),
            4 => Ok(MirBinaryOperator::Remainder),
            5 => Ok(MirBinaryOperator::Equal),
            6 => Ok(MirBinaryOperator::NotEqual),
            7 => Ok(MirBinaryOperator::LessThan),
            8 => Ok(MirBinaryOperator::LessThanOrEqual),
            9 => Ok(MirBinaryOperator::GreaterThan),
            10 => Ok(MirBinaryOperator::GreaterThanOrEqual),
            11 => Ok(MirBinaryOperator::BitwiseAnd),
            12 => Ok(MirBinaryOperator::BitwiseOr),
            13 => Ok(MirBinaryOperator::BitwiseXor),
            14 => Ok(MirBinaryOperator::ShiftLeft),
            15 => Ok(MirBinaryOperator::ShiftRight),
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn aggregate_kind(&mut self) -> Result<MirAggregateKind, ExecutableTemplateDecodeError> {
        match read_u32(&mut self.reader)? {
            0 => Ok(MirAggregateKind::Tuple),
            1 => Ok(MirAggregateKind::Array),
            2 => Ok(MirAggregateKind::RepeatedArray),
            3 => Ok(MirAggregateKind::NullablePresent),
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn numeric_conversion_kind(
        &mut self,
    ) -> Result<MirNumericConversionKind, ExecutableTemplateDecodeError> {
        match read_u32(&mut self.reader)? {
            0 => Ok(MirNumericConversionKind::Truncate),
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn call(&mut self) -> Result<MirCall, ExecutableTemplateDecodeError> {
        let target = match read_u32(&mut self.reader)? {
            0 => MirCallTarget::Direct(MirCallableReference::new(
                self.callable_instance()?,
                self.callable_abi()?,
            )),
            1 => MirCallTarget::Indirect {
                callee: self.operand()?,
                abi: self.callable_abi()?,
            },
            _ => return Err(ExecutableTemplateDecodeError::Malformed),
        };

        let result = self.call_result()?;
        let argument_count = self.count()?;
        let mut arguments = self.items(argument_count)?;

        for _ in 0..argument_count {
            arguments.push(self.call_argument()?);
        }

        let phase_behaviors = self.phase_behaviors()?;

        let dispatch_count = self.count()?;
        let mut dispatch_witnesses = self.items(dispatch_count)?;

        for _ in 0..dispatch_count {
            dispatch_witnesses.push(self.implementation()?);
        }

        let trait_dispatch = match read_u32(&mut self.reader)? {
            0 => None,
            1 => Some(self.trait_dispatch()?),
            _ => return Err(ExecutableTemplateDecodeError::Malformed),
        };

        let intrinsic = match read_u32(&mut self.reader)? {
            0 => None,
            1 => Some(bray_ir::MirCallIntrinsic::Unary(self.unary_operator()?)),
            2 => Some(bray_ir::MirCallIntrinsic::Binary(self.binary_operator()?)),
            3 => Some(bray_ir::MirCallIntrinsic::Conversion(self.ty()?)),
            _ => return Err(ExecutableTemplateDecodeError::Malformed),
        };

        let witness_count = self.count()?;
        let mut witnesses = self.items(witness_count)?;

        for _ in 0..witness_count {
            witnesses.push(SelectedImplementationWitness::new(
                self.implementation_requirement()?,
                self.implementation()?,
            ));
        }

        Ok(MirCall::imported(
            target,
            result,
            arguments,
            phase_behaviors,
            dispatch_witnesses,
            trait_dispatch,
            intrinsic,
            witnesses,
        ))
    }

    fn phase_behaviors(
        &mut self,
    ) -> Result<Option<CallablePhaseBehaviors>, ExecutableTemplateDecodeError> {
        match read_u32(&mut self.reader)? {
            0 => Ok(None),
            1 => {
                let invocation = self.phase_behavior()?;

                let behaviors = match read_u32(&mut self.reader)? {
                    0 => CallablePhaseBehaviors::synchronous(invocation),
                    1 => CallablePhaseBehaviors::asynchronous(invocation, self.phase_behavior()?),
                    _ => return Err(ExecutableTemplateDecodeError::Malformed),
                };

                Ok(Some(behaviors))
            }
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn phase_behavior(&mut self) -> Result<CallablePhaseBehavior, ExecutableTemplateDecodeError> {
        let effect_count = self.count()?;
        let mut effects = self.items(effect_count)?;

        for _ in 0..effect_count {
            effects.push(CallableEffectRequirement::new(self.symbol()?));
        }

        let capability_count = self.count()?;
        let mut capabilities = self.items(capability_count)?;

        for _ in 0..capability_count {
            capabilities.push(CallableCapabilityRequirement::new(self.symbol()?));
        }

        let trusted_count = self.count()?;
        let mut trusted_capabilities = self.items(trusted_count)?;

        for _ in 0..trusted_count {
            trusted_capabilities.push(TrustedCapabilityRequirement::new(
                SymbolOrdinal::new(read_u32(&mut self.reader)?),
                self.exact_symbol()?,
            ));
        }

        let execution_count = self.count()?;
        let mut execution_requirements = self.items(execution_count)?;

        for _ in 0..execution_count {
            execution_requirements.push(CallableExecutionRequirement::new(self.symbol()?));
        }

        let lifecycle_count = self.count()?;
        let mut lifecycle_obligations = self.items(lifecycle_count)?;

        for _ in 0..lifecycle_count {
            lifecycle_obligations.push(match read_u32(&mut self.reader)? {
                0 => LifecycleObligationKind::Destruction,
                1 => LifecycleObligationKind::Finalization,
                2 => LifecycleObligationKind::Cancellation,
                3 => LifecycleObligationKind::Joining,
                _ => return Err(ExecutableTemplateDecodeError::Malformed),
            });
        }

        let dependency_slot = index(read_u32(&mut self.reader)?)?;

        let dependency_contract = self
            .facts
            .dependency_contracts()
            .get(dependency_slot)
            .copied()
            .ok_or(ExecutableTemplateDecodeError::Malformed)?;

        let cancellation = match read_u32(&mut self.reader)? {
            0 => CurrentRunCancellation::NotEntered,
            1 => CurrentRunCancellation::MayEnter,
            _ => return Err(ExecutableTemplateDecodeError::Malformed),
        };

        Ok(CallablePhaseBehavior::new(
            effects,
            capabilities,
            trusted_capabilities,
            execution_requirements,
            lifecycle_obligations,
            dependency_contract,
            cancellation,
        ))
    }

    fn call_result(&mut self) -> Result<BoundCallResult, ExecutableTemplateDecodeError> {
        match read_u32(&mut self.reader)? {
            0 => Ok(BoundCallResult::Immediate(self.ty()?)),
            1 => Ok(BoundCallResult::LazyFuture(BoundFutureConstruction::new(
                self.ty()?,
                self.ty()?,
            ))),
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn call_argument(&mut self) -> Result<MirCallArgument, ExecutableTemplateDecodeError> {
        match read_u32(&mut self.reader)? {
            0 => Ok(MirCallArgument::Receiver {
                parameter: self.exact_symbol()?,
                value: self.operand()?,
            }),
            1 => {
                let parameter = match read_u32(&mut self.reader)? {
                    0 => None,
                    1 => Some(self.exact_symbol()?),
                    _ => return Err(ExecutableTemplateDecodeError::Malformed),
                };

                Ok(MirCallArgument::Explicit {
                    parameter,
                    ordinal: read_u32(&mut self.reader)?,
                    value: self.operand()?,
                })
            }
            2 => Ok(MirCallArgument::Default {
                parameter: self.exact_symbol()?,
                ordinal: read_u32(&mut self.reader)?,
                provider: self.exact_symbol()?,
            }),
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn callable_abi(&mut self) -> Result<bray_symbols::CallableAbi, ExecutableTemplateDecodeError> {
        match read_u32(&mut self.reader)? {
            0 => Ok(bray_symbols::CallableAbi::Bray),
            1 => Ok(bray_symbols::CallableAbi::C),
            2 => Ok(bray_symbols::CallableAbi::System),
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn construction_target(&mut self) -> Result<ConstructionTarget, ExecutableTemplateDecodeError> {
        match read_u32(&mut self.reader)? {
            0 => Ok(ConstructionTarget::Struct(self.exact_symbol()?)),
            1 => Ok(ConstructionTarget::UnionVariant(self.exact_symbol()?)),
            2 => Ok(ConstructionTarget::TypeForm {
                callable: self.callable_instance()?,
                requirement: self.implementation_requirement()?,
                witness: self.implementation()?,
            }),
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn construction_input(
        &mut self,
    ) -> Result<MirConstructionInput, ExecutableTemplateDecodeError> {
        match read_u32(&mut self.reader)? {
            0 => Ok(MirConstructionInput::Explicit {
                input: self.construction_input_id()?,
                ordinal: read_u32(&mut self.reader)?,
                value: self.operand()?,
            }),
            1 => {
                let input = self.construction_input_id()?;
                let ordinal = read_u32(&mut self.reader)?;
                let symbol = self.symbol()?;

                let provider = match symbol {
                    AnySymbolId::StructFieldDefaultProvider(provider) => {
                        ConstructionDefaultProvider::StructField(provider)
                    }
                    AnySymbolId::UnionPayloadDefaultProvider(provider) => {
                        ConstructionDefaultProvider::UnionPayload(provider)
                    }
                    AnySymbolId::CallableParameterDefaultProvider(provider) => {
                        ConstructionDefaultProvider::CallableParameter(provider)
                    }
                    _ => return Err(ExecutableTemplateDecodeError::Malformed),
                };

                Ok(MirConstructionInput::Default {
                    input,
                    ordinal,
                    provider,
                })
            }
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn construction_input_id(
        &mut self,
    ) -> Result<ConstructionInputId, ExecutableTemplateDecodeError> {
        match read_u32(&mut self.reader)? {
            0 => Ok(ConstructionInputId::StructField(self.exact_symbol()?)),
            1 => Ok(ConstructionInputId::UnionPayloadField(self.exact_symbol()?)),
            2 => Ok(ConstructionInputId::CallableParameter(self.exact_symbol()?)),
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn conversion(&mut self) -> Result<SelectedConversion, ExecutableTemplateDecodeError> {
        let source_type = self.ty()?;
        let target_type = self.ty()?;

        let target = match read_u32(&mut self.reader)? {
            0 => ConversionTarget::Identity,
            1 => ConversionTarget::BuiltInScalar,
            2 => {
                let count = self.count()?;
                let mut conversions = self.items(count)?;

                for _ in 0..count {
                    conversions.push(self.conversion()?);
                }

                ConversionTarget::Composite(conversions.into())
            }
            3 => ConversionTarget::Trait {
                member: self.callable_instance()?,
                fulfillment: self.callable_instance()?,
                requirement: self.implementation_requirement()?,
                witness: self.implementation()?,
            },
            4 => ConversionTarget::TraitConstraint {
                member: self.callable_instance()?,
                requirement: self.implementation_requirement()?,
                dispatch: self.trait_dispatch()?,
            },
            _ => return Err(ExecutableTemplateDecodeError::Malformed),
        };

        Ok(SelectedConversion::new(source_type, target_type, target))
    }

    fn pattern_projection(&mut self) -> Result<PatternProjection, ExecutableTemplateDecodeError> {
        match read_u32(&mut self.reader)? {
            0 => Ok(PatternProjection::ProductField(self.exact_symbol()?)),
            1 => Ok(PatternProjection::TupleElement(SymbolOrdinal::new(
                read_u32(&mut self.reader)?,
            ))),
            2 => Ok(PatternProjection::ActiveUnionPayloadField {
                variant: self.exact_symbol()?,
                field: self.exact_symbol()?,
            }),
            3 => Ok(PatternProjection::ElementFromStart(SymbolOrdinal::new(
                read_u32(&mut self.reader)?,
            ))),
            4 => Ok(PatternProjection::ElementFromEnd(SymbolOrdinal::new(
                read_u32(&mut self.reader)?,
            ))),
            5 => Ok(PatternProjection::NullableValue),
            6 => Ok(PatternProjection::OwnedTarget),
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn pattern_operation(&mut self) -> Result<PatternOperation, ExecutableTemplateDecodeError> {
        match read_u32(&mut self.reader)? {
            0 => Ok(PatternOperation::Observe),
            1 => Ok(PatternOperation::SharedBorrow),
            2 => Ok(PatternOperation::MutableBorrow),
            3 => Ok(PatternOperation::Consume),
            4 => Ok(PatternOperation::Copy),
            5 => Ok(PatternOperation::Recovered),
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn generator_operation(
        &mut self,
    ) -> Result<MirGeneratorOperation, ExecutableTemplateDecodeError> {
        match read_u32(&mut self.reader)? {
            0 => {
                let kind = self.generator_kind()?;
                let destination = self.place()?;
                let element = self.ty()?;

                let exact_count = match read_u32(&mut self.reader)? {
                    0 => None,
                    1 => Some(self.constant_term()?),
                    _ => return Err(ExecutableTemplateDecodeError::Malformed),
                };

                Ok(MirGeneratorOperation::Begin {
                    kind,
                    destination,
                    element,
                    exact_count,
                })
            }
            1 => Ok(MirGeneratorOperation::Push {
                destination: self.place()?,
                value: self.operand()?,
            }),
            2 => Ok(MirGeneratorOperation::Finish {
                destination: self.place()?,
            }),
            3 => Ok(MirGeneratorOperation::CleanupBroadcast {
                destination: self.place()?,
                element: self.ty()?,
                runtime: self.runtime_reference()?,
            }),
            4 => Ok(MirGeneratorOperation::Destroy {
                destination: self.place()?,
                element: self.ty()?,
                runtime: self.runtime_reference()?,
            }),
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn generator_kind(&mut self) -> Result<MirGeneratorKind, ExecutableTemplateDecodeError> {
        match read_u32(&mut self.reader)? {
            0 => Ok(MirGeneratorKind::Array),
            1 => Ok(MirGeneratorKind::General),
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn memory_operation(&mut self) -> Result<MirMemoryOperation, ExecutableTemplateDecodeError> {
        let kind = self.memory_kind()?;
        let operands = self.operands()?;
        let type_count = self.count()?;
        let mut operand_types = self.items(type_count)?;

        for _ in 0..type_count {
            operand_types.push(self.ty()?);
        }

        let result_type = match read_u32(&mut self.reader)? {
            0 => None,
            1 => Some(self.ty()?),
            _ => return Err(ExecutableTemplateDecodeError::Malformed),
        };

        validate_decoded_atomic_result(kind, result_type, self.facts)?;

        Ok(
            MirMemoryOperation::new(kind, operands, operand_types, result_type)
                .with_inline_assembly_symbols(self.callable_references()?),
        )
    }

    fn memory_kind(&mut self) -> Result<CheckedMemoryOperationKind, ExecutableTemplateDecodeError> {
        use bray_bound_tree::CheckedMemoryOperationKind as Kind;

        match read_u32(&mut self.reader)? {
            0 => Ok(Kind::Address {
                kind: match read_u32(&mut self.reader)? {
                    0 => bray_bound_tree::MemoryAddressKind::Shared,
                    1 => bray_bound_tree::MemoryAddressKind::Mutable,
                    _ => return Err(ExecutableTemplateDecodeError::Malformed),
                },
                pointee: self.ty()?,
            }),
            1 => Ok(Kind::Null {
                pointee: self.ty()?,
            }),
            2 => Ok(Kind::IsNull {
                pointee: self.ty()?,
            }),
            3 => Ok(Kind::Offset {
                unit: match read_u32(&mut self.reader)? {
                    0 => bray_bound_tree::MemoryOffsetUnit::Element,
                    1 => bray_bound_tree::MemoryOffsetUnit::Byte,
                    _ => return Err(ExecutableTemplateDecodeError::Malformed),
                },
                pointee: self.ty()?,
            }),
            4 => Ok(Kind::Reinterpret {
                source: self.ty()?,
                target: self.ty()?,
            }),
            5 => Ok(Kind::Read {
                pointee: self.ty()?,
                kind: match read_u32(&mut self.reader)? {
                    0 => bray_bound_tree::MemoryReadKind::Copy,
                    1 => bray_bound_tree::MemoryReadKind::Move,
                    _ => return Err(ExecutableTemplateDecodeError::Malformed),
                },
            }),
            6 => Ok(Kind::Write {
                pointee: self.ty()?,
            }),
            7 => Ok(Kind::Copy {
                pointee: self.ty()?,
                kind: match read_u32(&mut self.reader)? {
                    0 => bray_bound_tree::MemoryCopyKind::NonOverlapping,
                    1 => bray_bound_tree::MemoryCopyKind::Overlapping,
                    _ => return Err(ExecutableTemplateDecodeError::Malformed),
                },
            }),
            8 => Ok(Kind::LayoutQuery {
                ty: self.ty()?,
                kind: match read_u32(&mut self.reader)? {
                    0 => bray_bound_tree::MemoryLayoutQueryKind::Size,
                    1 => bray_bound_tree::MemoryLayoutQueryKind::Alignment,
                    2 => bray_bound_tree::MemoryLayoutQueryKind::Stride,
                    3 => bray_bound_tree::MemoryLayoutQueryKind::Layout,
                    _ => return Err(ExecutableTemplateDecodeError::Malformed),
                },
            }),
            9 => Ok(Kind::RawAllocate),
            10 => Ok(Kind::RawDeallocate),
            11 => Ok(Kind::Allocate),
            12 => Ok(Kind::Deallocate),
            13 => Ok(Kind::RawBufferCapacity),
            14 => Ok(Kind::RawBufferInitializedCount),
            15 => Ok(Kind::RawBufferPointer),
            16 => Ok(Kind::RawBufferInitializedSlice),
            17 => Ok(Kind::RawBufferInitializedSliceMut),
            18 => Ok(Kind::RawBufferSparePointer {
                element: self.ty()?,
            }),
            19 => Ok(Kind::RawBufferSetInitializedCount),
            20 => Ok(Kind::RawBufferRelease {
                element: self.ty()?,
            }),
            21 => Ok(Kind::RawBufferReplace {
                element: self.ty()?,
            }),
            22 => Ok(Kind::ByteBufferFill),
            23 => Ok(Kind::RawBufferRelocate {
                element: self.ty()?,
            }),
            24 => Ok(Kind::ByteBufferRead),
            25 => Ok(Kind::SliceLength),
            26 => Ok(Kind::CallbackState { state: self.ty()? }),
            27 => Ok(Kind::ByteSliceCopy),
            28 => Ok(Kind::VolatileRead {
                pointee: self.ty()?,
                address_space: match read_u32(&mut self.reader)? {
                    0 => bray_bound_tree::VolatileAddressSpace::Host,
                    1 => bray_bound_tree::VolatileAddressSpace::Device,
                    _ => return Err(ExecutableTemplateDecodeError::Malformed),
                },
                kind: match read_u32(&mut self.reader)? {
                    0 => bray_bound_tree::MemoryReadKind::Copy,
                    1 => bray_bound_tree::MemoryReadKind::Move,
                    _ => return Err(ExecutableTemplateDecodeError::Malformed),
                },
            }),
            29 => Ok(Kind::VolatileWrite {
                pointee: self.ty()?,
                address_space: match read_u32(&mut self.reader)? {
                    0 => bray_bound_tree::VolatileAddressSpace::Host,
                    1 => bray_bound_tree::VolatileAddressSpace::Device,
                    _ => return Err(ExecutableTemplateDecodeError::Malformed),
                },
            }),
            30 => Ok(Kind::ExposeAddress {
                pointee: self.ty()?,
            }),
            31 => Ok(Kind::FromExposedAddress {
                pointee: self.ty()?,
            }),
            32 => Ok(Kind::CompareAddress {
                pointee: self.ty()?,
                comparison: match read_u32(&mut self.reader)? {
                    0 => bray_bound_tree::PointerAddressComparison::Equal,
                    1 => bray_bound_tree::PointerAddressComparison::Less,
                    _ => return Err(ExecutableTemplateDecodeError::Malformed),
                },
            }),
            33 => {
                let compiler_only = super::support::read_bool(&mut self.reader)?;

                let Some(order) = MemoryOrder::from_u64(u64::from(read_u32(&mut self.reader)?))
                else {
                    return Err(ExecutableTemplateDecodeError::Malformed);
                };

                if !order.valid_for_fence() {
                    return Err(ExecutableTemplateDecodeError::Malformed);
                }

                Ok(Kind::Fence {
                    compiler_only,
                    order,
                })
            }
            34 => Ok(Kind::CatastrophicAbort),
            35 => Ok(Kind::DebuggerTrap),
            36 => Ok(Kind::UnreachableTermination),
            37 => Ok(Kind::SpinLoopHint),
            38 => Ok(Kind::TargetFeatureEnabled {
                feature: self.constant_value()?,
            }),
            39 => {
                let inputs = self.ty()?;

                let output = match read_u32(&mut self.reader)? {
                    0 => None,
                    1 => Some(self.ty()?),
                    _ => return Err(ExecutableTemplateDecodeError::Malformed),
                };

                let labels = match read_u32(&mut self.reader)? {
                    0 => None,
                    1 => Some(self.ty()?),
                    _ => return Err(ExecutableTemplateDecodeError::Malformed),
                };

                let contract = self.inline_assembly_contract(output.is_some())?;

                self.validate_inline_assembly_types(inputs, output, labels, contract)?;

                Ok(Kind::InlineAssembly {
                    inputs,
                    output,
                    labels,
                    contract,
                })
            }
            40 => Ok(Kind::AtomicInitialize { value: self.ty()? }),
            41 => decoded_atomic_kind(Kind::AtomicLoad {
                value: self.ty()?,
                order: self.atomic_order()?,
            }),
            42 => decoded_atomic_kind(Kind::AtomicStore {
                value: self.ty()?,
                order: self.atomic_order()?,
            }),
            43 => decoded_atomic_kind(Kind::AtomicExchange {
                value: self.ty()?,
                order: self.atomic_order()?,
            }),
            44 => decoded_atomic_kind(Kind::AtomicCompareExchange {
                value: self.ty()?,
                weak: read_bool(&mut self.reader)?,
                success: self.atomic_order()?,
                failure: self.atomic_order()?,
            }),
            45 => decoded_atomic_kind(Kind::AtomicFetch {
                value: self.ty()?,
                kind: match read_u32(&mut self.reader)? {
                    0 => bray_bound_tree::AtomicFetchKind::Add,
                    1 => bray_bound_tree::AtomicFetchKind::Subtract,
                    2 => bray_bound_tree::AtomicFetchKind::And,
                    3 => bray_bound_tree::AtomicFetchKind::Or,
                    4 => bray_bound_tree::AtomicFetchKind::Xor,
                    _ => return Err(ExecutableTemplateDecodeError::Malformed),
                },
                order: self.atomic_order()?,
            }),
            46 => decoded_atomic_kind(Kind::AtomicWait {
                value: self.ty()?,
                order: self.atomic_order()?,
            }),
            47 => Ok(Kind::AtomicNotify {
                value: self.ty()?,
                all: read_bool(&mut self.reader)?,
            }),
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn atomic_order(
        &mut self,
    ) -> Result<bray_bound_tree::MemoryOrder, ExecutableTemplateDecodeError> {
        bray_bound_tree::MemoryOrder::from_u64(u64::from(read_u32(&mut self.reader)?))
            .ok_or(ExecutableTemplateDecodeError::Malformed)
    }

    fn text_operation(&mut self) -> Result<MirTextOperation, ExecutableTemplateDecodeError> {
        let kind = self.text_kind()?;
        let operands = self.operands()?;
        let type_count = self.count()?;
        let mut operand_types = self.items(type_count)?;

        for _ in 0..type_count {
            operand_types.push(self.ty()?);
        }

        let result_type = match read_u32(&mut self.reader)? {
            0 => None,
            1 => Some(self.ty()?),
            _ => return Err(ExecutableTemplateDecodeError::Malformed),
        };

        Ok(MirTextOperation::new(
            kind,
            operands,
            operand_types,
            result_type,
        ))
    }

    fn inline_assembly_contract(
        &mut self,
        has_normal_output: bool,
    ) -> Result<InlineAssemblyContract, ExecutableTemplateDecodeError> {
        let (template, template_text) = self.string_constant_value()?;

        let (constraints, constraint_text) = self.string_constant_value()?;

        let (clobbers, _) = self.string_constant_value()?;

        let (features, _) = self.string_constant_value()?;

        let (options, option_bits) = self.integer_constant_value()?;

        if !assembly_options_valid(option_bits, has_normal_output) {
            return Err(ExecutableTemplateDecodeError::Malformed);
        }

        let count = self.count()?;

        if count > MAX_INLINE_ASSEMBLY_OPERANDS {
            return Err(ExecutableTemplateDecodeError::Malformed);
        }

        let mut operands = [None; MAX_INLINE_ASSEMBLY_OPERANDS];

        for destination in operands.iter_mut().take(count) {
            let kind = match read_u32(&mut self.reader)? {
                0 => InlineAssemblyOperandKind::Input,
                1 => InlineAssemblyOperandKind::LateOutput,
                2 => InlineAssemblyOperandKind::Output,
                3 => InlineAssemblyOperandKind::InOut,
                4 => InlineAssemblyOperandKind::EarlyInOut,
                5 => InlineAssemblyOperandKind::Immediate,
                6 => InlineAssemblyOperandKind::Symbol,
                7 => InlineAssemblyOperandKind::Memory,
                8 => InlineAssemblyOperandKind::Label,
                _ => return Err(ExecutableTemplateDecodeError::Malformed),
            };

            let ty = self.ty()?;
            let input = self.optional_u16()?;
            let runtime_input = self.optional_u16()?;
            let output = self.optional_u16()?;

            let constant = match read_u32(&mut self.reader)? {
                0 => None,
                1 => Some(self.integer_constant_value()?.0),
                _ => return Err(ExecutableTemplateDecodeError::Malformed),
            };

            let start = self
                .reader
                .read_u16()
                .map_err(|_| ExecutableTemplateDecodeError::Malformed)?;

            let length = self
                .reader
                .read_u16()
                .map_err(|_| ExecutableTemplateDecodeError::Malformed)?;

            *destination = Some(InlineAssemblyOperand::new(
                kind,
                ty,
                input,
                runtime_input,
                output,
                constant,
                None,
                start,
                length,
            ));
        }

        let count = u8::try_from(count).map_err(|_| ExecutableTemplateDecodeError::Malformed)?;

        decoded_inline_assembly_contract(
            template,
            constraints,
            clobbers,
            features,
            options,
            operands,
            count,
            &template_text,
            &constraint_text,
        )
    }

    fn validate_inline_assembly_types(
        &self,
        inputs: bray_symbols::TypeId,
        output: Option<bray_symbols::TypeId>,
        labels: Option<bray_symbols::TypeId>,
        contract: InlineAssemblyContract,
    ) -> Result<(), ExecutableTemplateDecodeError> {
        let (input_count, output_count, label_count) = assembly_type_counts(contract);

        let inputs = self.assembly_tuple_elements(Some(inputs), input_count)?;
        let outputs = self.assembly_tuple_elements(output, output_count)?;
        let labels = self.assembly_tuple_elements(labels, label_count)?;

        decoded_inline_assembly_types(contract, &inputs, &outputs, &labels)
    }

    fn validate_inline_assembly_value_types(
        &self,
        inputs: bray_symbols::TypeId,
        output: Option<bray_symbols::TypeId>,
        contract: InlineAssemblyContract,
    ) -> Result<(), ExecutableTemplateDecodeError> {
        let (input_count, output_count, _) = assembly_type_counts(contract);

        let inputs = self.assembly_tuple_elements(Some(inputs), input_count)?;
        let outputs = self.assembly_tuple_elements(output, output_count)?;

        decoded_inline_assembly_value_types(contract, &inputs, &outputs)
    }

    fn assembly_tuple_elements(
        &self,
        ty: Option<bray_symbols::TypeId>,
        expected: usize,
    ) -> Result<Vec<bray_symbols::TypeId>, ExecutableTemplateDecodeError> {
        let elements = match ty {
            Some(ty) => self
                .facts
                .assembly_value_element_types(ty)
                .ok_or(ExecutableTemplateDecodeError::Malformed)?,
            None if expected == 0 => Vec::new(),
            None => return Err(ExecutableTemplateDecodeError::Malformed),
        };

        if elements.len() != expected {
            return Err(ExecutableTemplateDecodeError::Malformed);
        }

        Ok(elements)
    }

    fn optional_u16(&mut self) -> Result<Option<u16>, ExecutableTemplateDecodeError> {
        match read_u32(&mut self.reader)? {
            0 => Ok(None),
            1 => self
                .reader
                .read_u16()
                .map(Some)
                .map_err(|_| ExecutableTemplateDecodeError::Malformed),
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn callable_references(
        &mut self,
    ) -> Result<Vec<MirCallableReference>, ExecutableTemplateDecodeError> {
        let count = self.count()?;
        let mut references = self.items(count)?;

        for _ in 0..count {
            references.push(MirCallableReference::new(
                self.callable_instance()?,
                self.callable_abi()?,
            ));
        }

        Ok(references)
    }

    fn text_kind(&mut self) -> Result<MirTextOperationKind, ExecutableTemplateDecodeError> {
        match read_u32(&mut self.reader)? {
            0 => Ok(MirTextOperationKind::ScalarCount),
            1 => Ok(MirTextOperationKind::IsEmpty),
            2 => Ok(MirTextOperationKind::Equals),
            3 => Ok(MirTextOperationKind::ScalarAt),
            4 => Ok(MirTextOperationKind::ScalarSlice),
            5 => Ok(MirTextOperationKind::Utf8),
            6 => Ok(MirTextOperationKind::FromUtf8),
            7 => Ok(MirTextOperationKind::CharacterScalarValue),
            8 => Ok(MirTextOperationKind::CharacterFromScalarValue),
            9 => Ok(MirTextOperationKind::CharacterUtf8Length),
            10 => Ok(MirTextOperationKind::CharacterUtf8Byte),
            11 => Ok(MirTextOperationKind::CharacterIsAlphabetic),
            12 => Ok(MirTextOperationKind::CharacterIsNumeric),
            13 => Ok(MirTextOperationKind::CharacterIsWhitespace),
            14 => Ok(MirTextOperationKind::Release),
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn panic_cause(&mut self) -> Result<MirPanicCause, ExecutableTemplateDecodeError> {
        match read_u32(&mut self.reader)? {
            0 => Ok(MirPanicCause::Message(self.operand()?)),
            1 => Ok(MirPanicCause::Assertion(self.optional_operand()?)),
            2 => Ok(MirPanicCause::ExplicitTestFailure(self.operand()?)),
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn async_operation(
        &mut self,
    ) -> Result<bray_ir::MirAsyncOperation, ExecutableTemplateDecodeError> {
        use bray_ir::MirAsyncOperation as Operation;

        match read_u32(&mut self.reader)? {
            0 => {
                let frame = self.frame_reference()?;

                let initializer = match read_u32(&mut self.reader)? {
                    0 => bray_ir::MirFrameInitializer::Callable(self.call()?),
                    1 => bray_ir::MirFrameInitializer::TaskObservation {
                        task: self.operand()?,
                        result: BoundFutureConstruction::new(self.ty()?, self.ty()?),
                        request_cancellation: read_bool(&mut self.reader)?,
                    },
                    _ => return Err(ExecutableTemplateDecodeError::Malformed),
                };

                Ok(Operation::CreateFrame { frame, initializer })
            }
            1 => Ok(Operation::MoveInactiveFrame {
                frame: self.frame_reference()?,
                source: self.place()?,
                destination: self.place()?,
            }),
            2 => Ok(Operation::ResumeFrame {
                frame: self.frame_id()?,
                state: bray_ir::MirFrameStateId::new(read_u32(&mut self.reader)?),
                storage: self.storage_id()?,
                runtime: self.runtime_reference()?,
            }),
            3 => Ok(Operation::ComposeAwaitedFrame {
                parent: self.frame_id()?,
                child: self.frame_reference()?,
                frame: self.operand()?,
            }),
            4 => Ok(Operation::CommitAwaitedCompletion {
                child: self.frame_reference()?,
            }),
            5 => Ok(Operation::StartTask {
                frame: self.frame_reference()?,
                value: self.operand()?,
                allocation: self.runtime_reference()?,
                start: self.runtime_reference()?,
            }),
            6 => Ok(Operation::RequestTaskCancellation {
                task: self.operand()?,
                runtime: self.runtime_reference()?,
            }),
            7 => Ok(Operation::ObserveCurrentRunCancellation {
                runtime: self.runtime_reference()?,
            }),
            8 => Ok(Operation::ResolveTask {
                task: self.operand()?,
                runtime: self.runtime_reference()?,
            }),
            9 => {
                let state = match read_u32(&mut self.reader)? {
                    0 => bray_ir::MirTaskTerminalState::Completed(self.operand()?),
                    1 => bray_ir::MirTaskTerminalState::Cancelled,
                    2 => bray_ir::MirTaskTerminalState::Panicked(self.operand()?),
                    _ => return Err(ExecutableTemplateDecodeError::Malformed),
                };

                Ok(Operation::PublishTerminalState {
                    state,
                    runtime: self.runtime_reference()?,
                })
            }
            10 => Ok(Operation::ExecuteCleanupBroadcast {
                frame: self.frame_id()?,
                runtime: self.runtime_reference()?,
            }),
            11 => Ok(Operation::ExecuteLifecycleResolution {
                frame: self.frame_id()?,
                runtime: self.runtime_reference()?,
            }),
            12 => Ok(Operation::TransferCleanupIncident {
                incident: self.operand()?,
                runtime: self.runtime_reference()?,
            }),
            13 => Ok(Operation::DestroyTerminalTask {
                task: self.operand()?,
            }),
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn frame_reference(&mut self) -> Result<MirFrameReference, ExecutableTemplateDecodeError> {
        match read_u32(&mut self.reader)? {
            0 => Ok(MirFrameReference::Known(self.frame_id()?)),
            1 => Ok(MirFrameReference::Erased),
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn terminator(&mut self) -> Result<MirTerminatorKind, ExecutableTemplateDecodeError> {
        match read_u32(&mut self.reader)? {
            0 => Ok(MirTerminatorKind::Goto(self.edge()?)),
            1 => Ok(MirTerminatorKind::Branch {
                condition: self.operand()?,
                then_edge: self.edge()?,
                else_edge: self.edge()?,
            }),
            2 => Ok(MirTerminatorKind::PatternBranch {
                subject: self.operand()?,
                predicate: self.pattern_predicate()?,
                matched: self.edge()?,
                unmatched: self.edge()?,
            }),
            3 => Ok(MirTerminatorKind::Iterate {
                cursor: self.place()?,
                next: MirCallableReference::new(self.callable_instance()?, self.callable_abi()?),
                witness: self.implementation()?,
                element_type: self.ty()?,
                item: self.block_id()?,
                exhausted: self.edge()?,
            }),
            4 => {
                let discriminant = self.operand()?;
                let count = self.count()?;
                let mut cases = self.items(count)?;

                for _ in 0..count {
                    cases.push(MirSwitchCase::new(self.constant_value()?, self.edge()?));
                }

                Ok(MirTerminatorKind::Switch {
                    discriminant,
                    cases: cases.into(),
                    otherwise: self.edge()?,
                })
            }
            5 => Ok(MirTerminatorKind::Return(self.optional_operand()?)),
            6 => Ok(MirTerminatorKind::Unreachable),
            7 => {
                let kind = match read_u32(&mut self.reader)? {
                    0 => bray_ir::MirSuspensionKind::Awaited,
                    1 => bray_ir::MirSuspensionKind::Yield,
                    _ => return Err(ExecutableTemplateDecodeError::Malformed),
                };

                Ok(MirTerminatorKind::Suspend {
                    kind,
                    resume_state: bray_ir::MirFrameStateId::new(read_u32(&mut self.reader)?),
                    resume: self.edge()?,
                    cancellation: self.cleanup_edge()?,
                    registration: self.runtime_reference()?,
                    wake: self.runtime_reference()?,
                })
            }
            8 => {
                let result = self.operand()?;
                let completed = (self.exact_symbol()?, self.edge()?);
                let panicked = (self.exact_symbol()?, self.cleanup_edge()?);
                let cancelled = (self.exact_symbol()?, self.cleanup_edge()?);

                Ok(MirTerminatorKind::ForwardRunResult {
                    result,
                    edges: bray_ir::MirRunResultEdges::new(completed, panicked, cancelled),
                })
            }
            9 => Ok(MirTerminatorKind::BeginCleanup(self.cleanup_edge()?)),
            10 => Ok(MirTerminatorKind::ContinueCleanup(self.cleanup_edge()?)),
            11 => Ok(MirTerminatorKind::Panic {
                report: self.operand()?,
                cleanup: self.cleanup_edge()?,
            }),
            12 => Ok(MirTerminatorKind::PropagatePanic {
                report: self.operand()?,
                runtime: self.runtime_reference()?,
            }),
            13 => Ok(MirTerminatorKind::PropagateCancellation {
                runtime: self.runtime_reference()?,
            }),
            14 => Ok(MirTerminatorKind::CancelCurrentRun {
                cleanup: self.cleanup_edge()?,
            }),
            15 => {
                let contract = self.inline_assembly_contract(true)?;
                let inputs = self.operand()?;
                let inputs_type = self.ty()?;
                let output_type = self.ty()?;

                self.validate_inline_assembly_value_types(
                    inputs_type,
                    Some(output_type),
                    contract,
                )?;

                let normal = self.block_id()?;
                let alternate_count = self.count()?;
                let mut alternates = self.items(alternate_count)?;

                for _ in 0..alternate_count {
                    alternates.push(self.block_id()?);
                }

                Ok(MirTerminatorKind::InlineAssembly(
                    bray_ir::MirInlineAssemblyTerminator::new(
                        contract,
                        inputs,
                        inputs_type,
                        output_type,
                        normal,
                        alternates,
                        self.callable_references()?,
                    ),
                ))
            }
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn edge(&mut self) -> Result<MirEdge, ExecutableTemplateDecodeError> {
        Ok(MirEdge::new(self.block_id()?, self.operands()?))
    }

    fn cleanup_edge(&mut self) -> Result<MirCleanupEdge, ExecutableTemplateDecodeError> {
        Ok(MirCleanupEdge::new(self.cleanup_phase()?, self.edge()?))
    }

    fn cleanup_phase(&mut self) -> Result<MirCleanupPhase, ExecutableTemplateDecodeError> {
        match read_u32(&mut self.reader)? {
            0 => Ok(MirCleanupPhase::TaskCancellation),
            1 => Ok(MirCleanupPhase::LifecycleResolution),
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn pattern_predicate(&mut self) -> Result<MirPatternPredicate, ExecutableTemplateDecodeError> {
        match read_u32(&mut self.reader)? {
            0 => Ok(MirPatternPredicate::Literal(self.constant_value()?)),
            1 => Ok(MirPatternPredicate::Constant(self.constant_term()?)),
            2 => Ok(MirPatternPredicate::NullableAbsent),
            3 => Ok(MirPatternPredicate::NullablePresent),
            4 => Ok(MirPatternPredicate::ActiveUnionVariant(
                self.exact_symbol()?,
            )),
            5 => Ok(MirPatternPredicate::ProductShape(self.exact_symbol()?)),
            6 => Ok(MirPatternPredicate::TupleShape(read_u32(&mut self.reader)?)),
            7 => Ok(MirPatternPredicate::ArrayShape(read_u32(&mut self.reader)?)),
            8 => Ok(MirPatternPredicate::OwnedTarget),
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn runtime_reference(&mut self) -> Result<MirRuntimeReference, ExecutableTemplateDecodeError> {
        let role = RuntimeAbiRole::ALL
            .get(index(read_u32(&mut self.reader)?)?)
            .copied()
            .ok_or(ExecutableTemplateDecodeError::Malformed)?;

        let version = self.runtime_version()?;

        Ok(MirRuntimeReference::new(role, version))
    }

    fn runtime_version(&mut self) -> Result<RuntimeAbiVersion, ExecutableTemplateDecodeError> {
        let major = self
            .reader
            .read_u16()
            .map_err(|_| ExecutableTemplateDecodeError::Malformed)?;

        let minor = self
            .reader
            .read_u16()
            .map_err(|_| ExecutableTemplateDecodeError::Malformed)?;

        Ok(RuntimeAbiVersion::new(major, minor))
    }

    fn execution_lane(
        &mut self,
    ) -> Result<ExecutionLaneRequirement, ExecutableTemplateDecodeError> {
        match read_u32(&mut self.reader)? {
            0 => Ok(ExecutionLaneRequirement::Blocking),
            1 => Ok(ExecutionLaneRequirement::Compute),
            2 => Ok(ExecutionLaneRequirement::MainThread),
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn frame_descriptor(
        &mut self,
    ) -> Result<Option<MirFrameDescriptor>, ExecutableTemplateDecodeError> {
        match read_u32(&mut self.reader)? {
            0 => Ok(None),
            1 => {
                let frame = self.frame_id()?;
                let abi_version = self.runtime_version()?;
                let mut versions = Vec::with_capacity(ProtectedFrameAbiOperation::ALL.len());

                for _ in ProtectedFrameAbiOperation::ALL {
                    versions.push(self.runtime_version()?);
                }

                let frame_abi = ProtectedFrameAbiVersions::new(
                    versions[0],
                    versions[1],
                    versions[2],
                    versions[3],
                    versions[4],
                );

                let result_type = self.ty()?;
                let state_count = self.count()?;
                let mut states = self.items(state_count)?;

                for _ in 0..state_count {
                    let state = bray_ir::MirFrameStateId::new(read_u32(&mut self.reader)?);
                    let entry = self.block_id()?;
                    let lane_count = self.count()?;
                    let mut lanes = self.items(lane_count)?;

                    for _ in 0..lane_count {
                        lanes.push(self.execution_lane()?);
                    }

                    let storage_count = self.count()?;
                    let mut storages = self.items(storage_count)?;

                    for _ in 0..storage_count {
                        storages.push(self.storage_id()?);
                    }

                    states.push(MirFrameStateFacts::new(state, entry, lanes, storages));
                }

                MirFrameDescriptor::try_new(frame, abi_version, frame_abi, result_type, states)
                    .map(Some)
                    .map_err(|_| ExecutableTemplateDecodeError::Malformed)
            }
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }
    fn unit_kind(&mut self) -> Result<MirUnitKind, ExecutableTemplateDecodeError> {
        match read_u32(&mut self.reader)? {
            0 => Ok(MirUnitKind::Synchronous),
            1 => Ok(MirUnitKind::ProtectedAsyncFrame(self.frame_id()?)),
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn block_kind(&mut self) -> Result<MirBlockKind, ExecutableTemplateDecodeError> {
        match read_u32(&mut self.reader)? {
            0 => Ok(MirBlockKind::Ordinary),
            1 => Ok(MirBlockKind::CleanupBroadcast),
            2 => Ok(MirBlockKind::LifecycleResolution),
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn storage_kind(&mut self) -> Result<MirStorageKind, ExecutableTemplateDecodeError> {
        match read_u32(&mut self.reader)? {
            0 => Ok(MirStorageKind::Parameter(read_u32(&mut self.reader)?)),
            1 => Ok(MirStorageKind::Local),
            2 => Ok(MirStorageKind::Temporary),
            3 => Ok(MirStorageKind::Return),
            4 => Ok(MirStorageKind::InactiveFrame),
            5 => Ok(MirStorageKind::CurrentFrame),
            6 => Ok(MirStorageKind::CurrentTask),
            7 => Ok(MirStorageKind::ChildTask),
            _ => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn slots(&mut self) -> Result<Vec<u32>, ExecutableTemplateDecodeError> {
        let count = self.count()?;
        let mut slots = self.items(count)?;

        for _ in 0..count {
            slots.push(read_u32(&mut self.reader)?);
        }

        Ok(slots)
    }

    fn ty(&mut self) -> Result<bray_symbols::TypeId, ExecutableTemplateDecodeError> {
        let slot = index(read_u32(&mut self.reader)?)?;

        self.facts
            .types()
            .get(slot)
            .copied()
            .ok_or(ExecutableTemplateDecodeError::Malformed)
    }

    fn constant_value(
        &mut self,
    ) -> Result<bray_symbols::ConstantValueId, ExecutableTemplateDecodeError> {
        let slot = index(read_u32(&mut self.reader)?)?;

        self.facts
            .constant_values()
            .get(slot)
            .copied()
            .ok_or(ExecutableTemplateDecodeError::Malformed)
    }

    fn string_constant_value(
        &mut self,
    ) -> Result<(bray_symbols::ConstantValueId, std::sync::Arc<str>), ExecutableTemplateDecodeError>
    {
        let (value, payload) = self.assembly_constant_value(AssemblyConstantRole::Text)?;

        match payload {
            AssemblyConstantPayload::Text(text) => Ok((value, text)),
            AssemblyConstantPayload::Integer(_) => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn integer_constant_value(
        &mut self,
    ) -> Result<(bray_symbols::ConstantValueId, u64), ExecutableTemplateDecodeError> {
        let (value, payload) = self.assembly_constant_value(AssemblyConstantRole::Integer)?;

        match payload {
            AssemblyConstantPayload::Integer(integer) => Ok((value, integer)),
            AssemblyConstantPayload::Text(_) => Err(ExecutableTemplateDecodeError::Malformed),
        }
    }

    fn assembly_constant_value(
        &mut self,
        role: AssemblyConstantRole,
    ) -> Result<
        (bray_symbols::ConstantValueId, AssemblyConstantPayload),
        ExecutableTemplateDecodeError,
    > {
        let slot = index(read_u32(&mut self.reader)?)?;

        let value = self
            .facts
            .constant_values()
            .get(slot)
            .copied()
            .ok_or(ExecutableTemplateDecodeError::Malformed)?;

        let kind = self
            .facts
            .interface_constant_values()
            .get(slot)
            .map(crate::InterfaceConstantValue::kind)
            .ok_or(ExecutableTemplateDecodeError::Malformed)?;

        assembly_constant_payload(role, kind)
            .map(|payload| (value, payload))
            .ok_or(ExecutableTemplateDecodeError::Malformed)
    }

    fn constant_term(
        &mut self,
    ) -> Result<bray_symbols::ConstantTermId, ExecutableTemplateDecodeError> {
        let slot = index(read_u32(&mut self.reader)?)?;

        self.facts
            .constant_terms()
            .get(slot)
            .copied()
            .ok_or(ExecutableTemplateDecodeError::Malformed)
    }

    fn substitution(
        &mut self,
    ) -> Result<bray_symbols::GenericSubstitutionId, ExecutableTemplateDecodeError> {
        let slot = index(read_u32(&mut self.reader)?)?;

        self.facts
            .substitutions()
            .get(slot)
            .copied()
            .ok_or(ExecutableTemplateDecodeError::Malformed)
    }

    fn trait_application(
        &mut self,
    ) -> Result<bray_symbols::TraitApplicationId, ExecutableTemplateDecodeError> {
        let slot = index(read_u32(&mut self.reader)?)?;

        self.facts
            .trait_applications()
            .get(slot)
            .copied()
            .ok_or(ExecutableTemplateDecodeError::Malformed)
    }

    fn implementation(
        &mut self,
    ) -> Result<bray_symbols::ImplementationInstanceId, ExecutableTemplateDecodeError> {
        let slot = index(read_u32(&mut self.reader)?)?;

        self.facts
            .implementation_instances()
            .get(slot)
            .copied()
            .ok_or(ExecutableTemplateDecodeError::Malformed)
    }

    fn symbol(&mut self) -> Result<AnySymbolId, ExecutableTemplateDecodeError> {
        let reference = read_symbol_reference(&mut self.reader, &mut self.semantic)?;

        self.symbols
            .resolve(&reference)
            .ok_or(ExecutableTemplateDecodeError::Malformed)
    }

    fn exact_symbol<I: ExactSymbolId>(&mut self) -> Result<I, ExecutableTemplateDecodeError> {
        I::try_from_any(self.symbol()?).ok_or(ExecutableTemplateDecodeError::Malformed)
    }

    fn callable_definition(
        &mut self,
    ) -> Result<CallableDefinitionId, ExecutableTemplateDecodeError> {
        CallableDefinitionId::try_new(self.symbol()?)
            .ok_or(ExecutableTemplateDecodeError::Malformed)
    }

    fn callable_instance(&mut self) -> Result<CallableInstanceData, ExecutableTemplateDecodeError> {
        Ok(CallableInstanceData::new(
            self.callable_definition()?,
            self.substitution()?,
        ))
    }

    fn implementation_requirement(
        &mut self,
    ) -> Result<ImplementationRequirementKey, ExecutableTemplateDecodeError> {
        Ok(ImplementationRequirementKey::new(
            self.ty()?,
            self.trait_application()?,
        ))
    }

    fn trait_dispatch(&mut self) -> Result<TraitConstraintDispatch, ExecutableTemplateDecodeError> {
        let owner = GenericOwnerId::try_new(self.symbol()?)
            .ok_or(ExecutableTemplateDecodeError::Malformed)?;

        let ordinal = SymbolOrdinal::new(read_u32(&mut self.reader)?);

        Ok(TraitConstraintDispatch::new(owner, ordinal))
    }

    fn frame_id(&mut self) -> Result<ProtectedAsyncFrameId, ExecutableTemplateDecodeError> {
        let digest = self
            .reader
            .read_array::<32>()
            .map_err(|_| ExecutableTemplateDecodeError::Malformed)?;

        Ok(ProtectedAsyncFrameId::new(digest))
    }

    fn block_id(&mut self) -> Result<MirBlockId, ExecutableTemplateDecodeError> {
        Ok(MirBlockId::from_slot(
            self.unit,
            read_u32(&mut self.reader)?,
        ))
    }

    fn storage_id(&mut self) -> Result<MirStorageId, ExecutableTemplateDecodeError> {
        Ok(MirStorageId::from_slot(
            self.unit,
            read_u32(&mut self.reader)?,
        ))
    }

    fn value_id(&mut self) -> Result<MirValueId, ExecutableTemplateDecodeError> {
        Ok(MirValueId::from_slot(
            self.unit,
            read_u32(&mut self.reader)?,
        ))
    }
}

fn decoded_atomic_kind(
    kind: CheckedMemoryOperationKind,
) -> Result<CheckedMemoryOperationKind, ExecutableTemplateDecodeError> {
    kind.has_valid_atomic_ordering()
        .then_some(kind)
        .ok_or(ExecutableTemplateDecodeError::Malformed)
}

fn validate_decoded_atomic_result(
    kind: CheckedMemoryOperationKind,
    result: Option<bray_symbols::TypeId>,
    facts: &ImportedSemanticFacts,
) -> Result<(), ExecutableTemplateDecodeError> {
    let CheckedMemoryOperationKind::AtomicCompareExchange { value, .. } = kind else {
        return Ok(());
    };

    let result = result.ok_or(ExecutableTemplateDecodeError::Malformed)?;

    let elements = facts
        .tuple_element_types(result)
        .ok_or(ExecutableTemplateDecodeError::Malformed)?;

    let boolean = bray_compiler_known::CompilerKnownDeclarationKey::try_new("Bool")
        .ok_or(ExecutableTemplateDecodeError::Malformed)?;

    atomic_compare_exchange_result_elements_valid(value, &elements, |ty| {
        facts.is_compiler_known_type(ty, &boolean)
    })
    .then_some(())
    .ok_or(ExecutableTemplateDecodeError::Malformed)
}

fn atomic_compare_exchange_result_elements_valid(
    value: bray_symbols::TypeId,
    elements: &[bray_symbols::TypeId],
    is_boolean: impl FnOnce(bray_symbols::TypeId) -> bool,
) -> bool {
    let [actual, boolean] = elements else {
        return false;
    };

    *actual == value && is_boolean(*boolean)
}

#[expect(
    clippy::too_many_arguments,
    reason = "package validation joins every decoded assembly contract field"
)]
fn decoded_inline_assembly_contract(
    template: bray_symbols::ConstantValueId,
    constraints: bray_symbols::ConstantValueId,
    clobbers: bray_symbols::ConstantValueId,
    features: bray_symbols::ConstantValueId,
    options: bray_symbols::ConstantValueId,
    operands: [Option<InlineAssemblyOperand>; MAX_INLINE_ASSEMBLY_OPERANDS],
    count: u8,
    template_text: &str,
    constraint_text: &str,
) -> Result<InlineAssemblyContract, ExecutableTemplateDecodeError> {
    InlineAssemblyContract::try_new(
        template,
        constraints,
        clobbers,
        features,
        options,
        operands,
        count,
        template_text,
        constraint_text,
    )
    .ok_or(ExecutableTemplateDecodeError::Malformed)
}

fn decoded_inline_assembly_types(
    contract: InlineAssemblyContract,
    inputs: &[bray_symbols::TypeId],
    outputs: &[bray_symbols::TypeId],
    labels: &[bray_symbols::TypeId],
) -> Result<(), ExecutableTemplateDecodeError> {
    contract
        .operand_types_valid(inputs, outputs, labels)
        .then_some(())
        .ok_or(ExecutableTemplateDecodeError::Malformed)
}

fn decoded_inline_assembly_value_types(
    contract: InlineAssemblyContract,
    inputs: &[bray_symbols::TypeId],
    outputs: &[bray_symbols::TypeId],
) -> Result<(), ExecutableTemplateDecodeError> {
    contract
        .value_types_valid(inputs, outputs)
        .then_some(())
        .ok_or(ExecutableTemplateDecodeError::Malformed)
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        CheckedMemoryOperationKind, InlineAssemblyOperand, InlineAssemblyOperandKind,
        MAX_INLINE_ASSEMBLY_OPERANDS, MemoryOrder,
    };
    use bray_symbols::{
        ConstantValueData, ConstantValueKind, IntegerConstant, SemanticValueStore, TypeData,
    };

    use crate::InterfaceConstantValueKind;

    use super::{
        AssemblyConstantPayload, AssemblyConstantRole, ExecutableTemplateDecodeError,
        assembly_constant_payload, assembly_options_valid,
        atomic_compare_exchange_result_elements_valid, decoded_atomic_kind,
        decoded_inline_assembly_contract, decoded_inline_assembly_types,
        decoded_inline_assembly_value_types,
    };

    #[test]
    fn malformed_atomic_orderings_are_rejected_before_mir() {
        let values = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("test semantic values must be available: {error:?}"));

        let value = values
            .intern_type(TypeData::Error)
            .unwrap_or_else(|error| panic!("test atomic type must intern: {error:?}"));

        for kind in [
            CheckedMemoryOperationKind::AtomicLoad {
                value,
                order: MemoryOrder::Release,
            },
            CheckedMemoryOperationKind::AtomicStore {
                value,
                order: MemoryOrder::Acquire,
            },
            CheckedMemoryOperationKind::AtomicCompareExchange {
                value,
                weak: false,
                success: MemoryOrder::Relaxed,
                failure: MemoryOrder::Acquire,
            },
            CheckedMemoryOperationKind::AtomicWait {
                value,
                order: MemoryOrder::AcquireRelease,
            },
        ] {
            assert_eq!(
                decoded_atomic_kind(kind),
                Err(ExecutableTemplateDecodeError::Malformed)
            );
        }

        assert_eq!(
            decoded_atomic_kind(CheckedMemoryOperationKind::AtomicLoad {
                value,
                order: MemoryOrder::Acquire,
            }),
            Ok(CheckedMemoryOperationKind::AtomicLoad {
                value,
                order: MemoryOrder::Acquire,
            })
        );
    }

    #[test]
    fn imported_atomic_compare_exchange_requires_exact_value_boolean_result_tuple() {
        let values = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("test semantic values must be available: {error:?}"));

        let value = values
            .intern_type(TypeData::Error)
            .unwrap_or_else(|error| panic!("test atomic value type must intern: {error:?}"));

        let boolean = values
            .intern_type(TypeData::tuple([]))
            .unwrap_or_else(|error| panic!("test Boolean marker type must intern: {error:?}"));

        let other = values
            .intern_type(TypeData::tuple([value]))
            .unwrap_or_else(|error| panic!("test mismatched type must intern: {error:?}"));

        let is_boolean = |candidate| candidate == boolean;

        assert!(atomic_compare_exchange_result_elements_valid(
            value,
            &[value, boolean],
            is_boolean,
        ));

        assert!(!atomic_compare_exchange_result_elements_valid(
            value,
            &[other, boolean],
            is_boolean,
        ));

        assert!(!atomic_compare_exchange_result_elements_valid(
            value,
            &[value, other],
            is_boolean,
        ));

        assert!(!atomic_compare_exchange_result_elements_valid(
            value,
            &[value],
            is_boolean,
        ));
    }

    #[test]
    fn malformed_package_assembly_constant_roles_are_rejected_before_mir() {
        let text = InterfaceConstantValueKind::String("reg".into());
        let integer = InterfaceConstantValueKind::Integer(IntegerConstant::from_u64(0));
        let boolean = InterfaceConstantValueKind::Boolean(false);

        assert_eq!(
            assembly_constant_payload(AssemblyConstantRole::Text, &text),
            Some(AssemblyConstantPayload::Text("reg".into()))
        );

        assert_eq!(
            assembly_constant_payload(AssemblyConstantRole::Integer, &integer),
            Some(AssemblyConstantPayload::Integer(0))
        );

        assert_eq!(
            assembly_constant_payload(AssemblyConstantRole::Text, &integer),
            None
        );

        assert_eq!(
            assembly_constant_payload(AssemblyConstantRole::Integer, &text),
            None
        );

        assert_eq!(
            assembly_constant_payload(AssemblyConstantRole::Text, &boolean),
            None
        );
    }

    #[test]
    fn malformed_package_assembly_options_are_rejected_before_mir() {
        assert!(assembly_options_valid(0, false));
        assert!(assembly_options_valid(1, true));
        assert!(!assembly_options_valid(1, false));
        assert!(!assembly_options_valid(1 << 3, true));
        assert!(!assembly_options_valid(1 << 4, true));
    }

    #[test]
    fn malformed_package_assembly_descriptors_are_rejected_before_mir() {
        let values = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("test semantic values must be available: {error:?}"));

        let ty = values
            .intern_type(TypeData::Error)
            .unwrap_or_else(|error| panic!("test assembly type must intern: {error:?}"));

        let constant = values
            .intern_constant_value(ConstantValueData::new(
                ty,
                ConstantValueKind::Boolean(false),
            ))
            .unwrap_or_else(|error| panic!("test assembly constant must intern: {error:?}"));

        let input = InlineAssemblyOperand::new(
            InlineAssemblyOperandKind::Input,
            ty,
            Some(0),
            Some(0),
            None,
            None,
            None,
            0,
            3,
        );

        let mut operands = [None; MAX_INLINE_ASSEMBLY_OPERANDS];
        operands[0] = Some(input);

        let decode = |operands, count, template, constraints| {
            decoded_inline_assembly_contract(
                constant,
                constant,
                constant,
                constant,
                constant,
                operands,
                count,
                template,
                constraints,
            )
        };

        assert!(decode(operands, 1, "use $0", "reg").is_ok());

        assert_eq!(
            decode(operands, 1, "use $1", "reg"),
            Err(ExecutableTemplateDecodeError::Malformed)
        );

        let mut reserved_register = operands;

        reserved_register[0] = Some(InlineAssemblyOperand::new(
            InlineAssemblyOperandKind::Input,
            ty,
            Some(0),
            Some(0),
            None,
            None,
            None,
            0,
            1,
        ));

        assert_eq!(
            decode(reserved_register, 1, "", "m"),
            Err(ExecutableTemplateDecodeError::Malformed)
        );

        let contract = decode(operands, 1, "", "reg")
            .unwrap_or_else(|error| panic!("valid package contract must decode: {error:?}"));

        let mismatched = values
            .intern_type(TypeData::tuple([]))
            .unwrap_or_else(|error| panic!("mismatched package type must intern: {error:?}"));

        assert_eq!(
            decoded_inline_assembly_types(contract, &[mismatched], &[], &[]),
            Err(ExecutableTemplateDecodeError::Malformed)
        );

        let mut mixed = [None; MAX_INLINE_ASSEMBLY_OPERANDS];
        mixed[0] = Some(input);

        mixed[1] = Some(InlineAssemblyOperand::new(
            InlineAssemblyOperandKind::Memory,
            ty,
            Some(1),
            Some(1),
            None,
            None,
            None,
            4,
            1,
        ));

        mixed[2] = Some(InlineAssemblyOperand::new(
            InlineAssemblyOperandKind::Immediate,
            ty,
            Some(2),
            None,
            None,
            Some(constant),
            None,
            6,
            1,
        ));

        mixed[3] = Some(InlineAssemblyOperand::new(
            InlineAssemblyOperandKind::Symbol,
            ty,
            Some(3),
            None,
            None,
            None,
            None,
            8,
            1,
        ));

        mixed[4] = Some(InlineAssemblyOperand::new(
            InlineAssemblyOperandKind::Label,
            ty,
            None,
            None,
            None,
            None,
            None,
            10,
            5,
        ));

        let mixed = decode(mixed, 5, "", "reg,m,i,s,label")
            .unwrap_or_else(|error| panic!("mixed package contract must decode: {error:?}"));

        assert_eq!(
            decoded_inline_assembly_types(mixed, &[ty, ty], &[], &[ty]),
            Ok(())
        );

        assert_eq!(
            decoded_inline_assembly_value_types(mixed, &[ty, ty], &[]),
            Ok(())
        );

        let mut duplicate_runtime = operands;

        duplicate_runtime[1] = Some(InlineAssemblyOperand::new(
            InlineAssemblyOperandKind::Input,
            ty,
            Some(1),
            Some(0),
            None,
            None,
            None,
            4,
            3,
        ));

        assert_eq!(
            decode(duplicate_runtime, 2, "", "reg,reg"),
            Err(ExecutableTemplateDecodeError::Malformed)
        );

        let mut sparse = operands;
        sparse[0] = None;
        sparse[1] = Some(input);

        assert_eq!(
            decode(sparse, 2, "", "reg,reg"),
            Err(ExecutableTemplateDecodeError::Malformed)
        );
    }
}
