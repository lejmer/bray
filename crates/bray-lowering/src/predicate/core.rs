use std::collections::BTreeMap;
use std::sync::Arc;

use bray_bound_tree::{
    CheckedTemplate, CheckedTemplateInputKind, CheckedTemplateNodeId, CheckedTemplateOperation,
};
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirBlockKind, MirCall, MirHelperReference, MirOperand, MirOperationKind, MirPlace,
    MirPredicateReference, MirSourceAnchor, MirStorageKind, MirTargetContract, MirTerminatorKind,
    MirUnit, MirUnitBuilder, MirUnitId, MirUnitKey, MirValueId,
};
use bray_symbols::{
    AnySymbolId, ConstantTermId, ConstantValueId, GenericSubstitutionId, SymbolKey, TypeId,
};

use crate::{SyntheticLoweringContext, SyntheticLoweringError};

/// Resolves established checked semantics for generated observational predicate bodies.
pub trait PredicateLoweringContext: SyntheticLoweringContext {
    /// Returns the complete source or imported checked expression graph.
    fn predicate_template(
        &self,
        reference: &MirPredicateReference,
    ) -> Result<Arc<CheckedTemplate>, Self::Error>;
    /// Substitutes a type in the selected concrete predicate specialization.
    fn predicate_type(&self, ty: TypeId) -> Result<TypeId, Self::Error>;
    /// Resolves an already-selected declaration identity.
    fn predicate_symbol(&self, key: &SymbolKey) -> Result<AnySymbolId, Self::Error>;
    /// Materializes one compile-time-only term in the selected specialization.
    fn predicate_constant(&self, term: ConstantTermId) -> Result<ConstantValueId, Self::Error>;
    /// Resolves an already-selected named predicate with its readonly formal input types.
    fn predicate_application(
        &self,
        predicate: bray_symbols::PredicateInstanceData,
        inputs: Vec<MirOperand>,
    ) -> Result<(MirCall, Vec<TypeId>), Self::Error>;
    /// Resolves a declaration-owned constant or a closed generic constant input.
    fn predicate_declaration(
        &self,
        key: &SymbolKey,
        substitution: Option<GenericSubstitutionId>,
        ty: TypeId,
    ) -> Result<MirOperand, Self::Error>;
    /// Returns an established compiler-known implementation hook for a selected declaration.
    fn predicate_hook(
        &self,
        callable: bray_symbols::CallableInstanceData,
    ) -> Result<Option<bray_compiler_known::ImplementationHook>, Self::Error>;
    /// Returns a selected field's established declaration ordinal.
    fn predicate_input_ordinal(
        &self,
        field: bray_bound_tree::ConstructionInputId,
    ) -> Result<u32, Self::Error>;
    /// Reuses a term's selected callable and implementation.
    fn predicate_instance_call(
        &self,
        callable: bray_symbols::CallableInstanceData,
        implementation: Option<bray_symbols::ImplementationInstanceId>,
        inputs: Vec<MirOperand>,
    ) -> Result<(MirCall, Vec<TypeId>), Self::Error>;
    /// Resolves an indexed slice's already-checked nullable bound parameter type.
    fn predicate_index_bound_type(
        &self,
        selected: &bray_bound_tree::CheckedTemplateIndexCall,
    ) -> Result<TypeId, Self::Error>;
    /// Resolves a declared projection after substituting its subject type.
    fn predicate_field_type(
        &self,
        subject: TypeId,
        field: bray_ir::MirFieldReference,
    ) -> Result<TypeId, Self::Error>;
    /// Concretizes the already-selected callable or predicate and optional implementation.
    fn predicate_call(
        &self,
        key: &SymbolKey,
        substitution: GenericSubstitutionId,
        dispatch: Option<&bray_bound_tree::CheckedTemplateDispatch>,
        inputs: Vec<MirOperand>,
    ) -> Result<(MirCall, Vec<TypeId>), Self::Error>;
}

pub(super) struct PredicateLowerer<'context, C: PredicateLoweringContext + ?Sized> {
    pub(super) context: &'context C,
    pub(super) template: Arc<CheckedTemplate>,
    pub(super) builder: MirUnitBuilder,
    pub(super) source: MirSourceAnchor,
    pub(super) boolean: TypeId,
    pub(super) inputs: Vec<Option<MirOperand>>,
    pub(super) inserted: Vec<CheckedTemplateNodeId>,
    pub(super) values: BTreeMap<CheckedTemplateNodeId, MirOperand>,
    pub(super) value_blocks: BTreeMap<MirValueId, MirBlockId>,
    pub(super) predecessors: BTreeMap<MirBlockId, Vec<MirBlockId>>,
    pub(super) forwarded: BTreeMap<MirBlockId, BTreeMap<MirValueId, MirValueId>>,
    pub(super) imports: BTreeMap<MirBlockId, Vec<MirValueId>>,
}

/// Lowers a complete checked predicate into ordinary synchronous MIR over readonly inputs.
pub fn lower_predicate<C: PredicateLoweringContext + ?Sized>(
    context: &C,
    key: &bray_ir::MirPredicateKey,
    unit: MirUnitId,
    target: &MirTargetContract,
) -> Result<MirUnit, C::Error> {
    let reference = key.reference();

    let template = match reference {
        MirPredicateReference::Condition {
            condition,
            input_types,
        } => Arc::new(super::terms::condition_template(
            context,
            *condition,
            input_types,
        )?),
        _ => context.predicate_template(reference)?,
    };

    // The generated body and each source anchor retain the same shared semantic identity.
    let helper = MirHelperReference::Predicate(reference.clone());
    let source = MirSourceAnchor::generated_helper(helper.clone());

    let mut builder = MirUnitBuilder::for_generated_helper(
        unit,
        MirUnitKey::Predicate(key.clone()),
        helper,
        bray_ir::MirUnitKind::Synchronous,
        target.clone(),
    );

    let entry = builder
        .push_block(source.clone(), MirBlockKind::Ordinary)
        .map_err(capacity)?;

    let boolean = context.representation_type(RepresentationRole::ScalarBool)?;
    let mut inputs = Vec::new();
    let mut ordinal = 0;

    for input in template.inputs() {
        let ty = context.predicate_type(input.ty())?;

        let value = match input.kind() {
            CheckedTemplateInputKind::Receiver | CheckedTemplateInputKind::Parameter(_) => {
                let storage = builder
                    .push_storage(
                        source.clone(),
                        MirStorageKind::BorrowedParameter(ordinal),
                        ty,
                    )
                    .map_err(capacity)?;

                ordinal += 1;

                Some(MirOperand::Copy(MirPlace::new(storage, [], ty)))
            }
            CheckedTemplateInputKind::GenericConstant(key) => {
                Some(context.predicate_declaration(key, None, ty)?)
            }
            CheckedTemplateInputKind::GenericType(_) => {
                // Type inputs occur in semantic substitutions, never runtime value expressions.
                None
            }
            CheckedTemplateInputKind::PostconditionResult => {
                panic!("invocation predicate cannot observe a completion result")
            }
        };

        inputs.push(value);
    }

    let result = template.result();

    let mut lowerer = PredicateLowerer {
        context,
        template,
        builder,
        source,
        boolean,
        inputs,
        inserted: Vec::new(),
        values: BTreeMap::new(),
        value_blocks: BTreeMap::new(),
        predecessors: BTreeMap::new(),
        forwarded: BTreeMap::new(),
        imports: BTreeMap::new(),
    };

    let (block, value) = lowerer.node(entry, result)?;

    assert_eq!(
        lowerer.builder.operand_type(&value),
        boolean,
        "checked runtime predicate returns Boolean"
    );

    lowerer.terminate(
        block,
        MirTerminatorKind::Return {
            value: Some(value),
            report: None,
        },
    )?;

    lowerer.finish_edges();

    Ok(lowerer.builder.finish(entry))
}

pub(super) fn capacity<E: From<SyntheticLoweringError>>(cause: bray_ir::MirCapacityError) -> E {
    SyntheticLoweringError::Capacity(cause).into()
}

impl<C: PredicateLoweringContext + ?Sized> PredicateLowerer<'_, C> {
    pub(in crate::predicate) fn integer(
        &self,
        ty: TypeId,
        value: u64,
    ) -> Result<MirOperand, C::Error> {
        crate::operand::integer_constant(self.context.semantic_values(), ty, value)
            .map_err(|cause| crate::SyntheticLoweringError::SemanticValue(cause).into())
    }

    pub(in crate::predicate) fn machine(
        &mut self,
        block: MirBlockId,
        operator: bray_ir::MirBinaryOperator,
        left: MirOperand,
        right: MirOperand,
        ty: TypeId,
    ) -> Result<MirOperand, C::Error> {
        self.value(
            block,
            MirOperationKind::Binary {
                operator,
                left,
                right,
            },
            ty,
        )
    }

    pub(super) fn value(
        &mut self,
        block: MirBlockId,
        operation: MirOperationKind,
        ty: TypeId,
    ) -> Result<MirOperand, C::Error> {
        let operation = self.local_operation(block, operation)?;

        let result = self
            .builder
            .push_operation(block, self.source.clone(), operation, Some(ty))
            .map_err(capacity)?;

        let value = result
            .result()
            .expect("value-producing predicate operation retains its SSA result");

        self.value_blocks.insert(value, block);

        Ok(MirOperand::Value(value))
    }

    pub(super) fn node(
        &mut self,
        mut block: MirBlockId,
        id: CheckedTemplateNodeId,
    ) -> Result<(MirBlockId, MirOperand), C::Error> {
        if let Some(value) = self.values.get(&id) {
            // Reusing a dominating pure value preserves lazy branch evaluation.
            return Ok((block, value.clone()));
        }

        // Retain the small operation and shared child lists across recursive MIR construction.
        let node = self.template.nodes()
            [usize::try_from(id.raw()).expect("checked template index fits the host")]
        .clone();

        let ty = self.context.predicate_type(node.ty())?;

        let (continued, value) = match node.operation() {
            CheckedTemplateOperation::Unreachable => panic!(
                "excluded template value is lowered only as a terminating conditional alternative"
            ),
            CheckedTemplateOperation::Input(input) => (
                block,
                self.inputs
                    [usize::try_from(input.raw()).expect("checked input index fits the host")]
                .as_ref()
                .expect("runtime expression cannot read a type-only input")
                .clone(),
            ),
            CheckedTemplateOperation::Constant { term, .. } => self.term(block, *term, ty)?,
            CheckedTemplateOperation::Unary { operation, operand } => {
                let (continued, operand) = self.node(block, *operand)?;

                (continued, self.unary(continued, *operation, operand, ty)?)
            }
            CheckedTemplateOperation::Binary {
                operation: bray_symbols::ConstantBinaryOperation::LogicalAnd,
                left,
                right,
            } => self.short_circuit(
                block,
                bray_bound_tree::CheckedTemplateShortCircuitKind::And,
                *left,
                *right,
            )?,
            CheckedTemplateOperation::Binary {
                operation: bray_symbols::ConstantBinaryOperation::LogicalOr,
                left,
                right,
            } => self.short_circuit(
                block,
                bray_bound_tree::CheckedTemplateShortCircuitKind::Or,
                *left,
                *right,
            )?,
            CheckedTemplateOperation::Binary {
                operation,
                left,
                right,
            } => {
                let (continued, left) = self.node(block, *left)?;

                let (continued, right) = self.node(continued, *right)?;

                self.binary(continued, *operation, left, right, ty)?
            }
            CheckedTemplateOperation::Conditional {
                condition,
                when_true,
                when_false,
            } => self.conditional(block, *condition, *when_true, *when_false, ty)?,
            CheckedTemplateOperation::ShortCircuit { kind, left, right } => {
                self.short_circuit(block, *kind, *left, *right)?
            }
            CheckedTemplateOperation::Call {
                callable,
                substitution,
                arguments,
                ordinals,
                dispatch,
            } => {
                let mut inputs = Vec::with_capacity(arguments.len());

                for argument in arguments.iter() {
                    let (continued, value) = self.node(block, *argument)?;

                    block = continued;
                    inputs.push(value);
                }

                let mut ordered = ordinals.iter().copied().zip(inputs).collect::<Vec<_>>();

                ordered.sort_by_key(|(ordinal, _)| *ordinal);

                let inputs = ordered
                    .into_iter()
                    .map(|(_, value)| value)
                    .collect::<Vec<_>>();

                let call = self.context.predicate_call(
                    callable,
                    *substitution,
                    dispatch.as_ref(),
                    inputs,
                )?;

                self.call(block, call)?
            }
            CheckedTemplateOperation::Declaration {
                declaration,
                substitution,
            } => (
                block,
                self.context
                    .predicate_declaration(declaration, *substitution, ty)?,
            ),
            CheckedTemplateOperation::Convert { value, target } => {
                let (continued, value) = self.node(block, *value)?;

                let target = self.context.predicate_type(*target)?;

                (continued, self.convert(continued, value, target)?)
            }
            CheckedTemplateOperation::Temporary(temporary) => {
                let initializer = self.template.temporaries()[usize::try_from(temporary.raw())
                    .expect("checked temporary index fits the host")]
                .initializer();

                self.node(block, initializer)?
            }
            operation => self.structural(block, operation, ty)?,
        };

        // This invocation owns the memoized operand while its immutable place remains shared.
        self.values.insert(id, value.clone());
        self.inserted.push(id);

        Ok((continued, value))
    }
}
