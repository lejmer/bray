// rust-style: allow(module-too-large, reason = "codegen specialization shares one recursive witness realization context")

use std::collections::BTreeMap;
use std::sync::Arc;

use bray_binder::SymbolQueryProvider;
use bray_codegen::{
    CodegenImplementationWitness, CodegenInstanceKey, CodegenReachability, CodegenSpecialization,
    CodegenTarget, DemandedCallableInstance, IntrinsicCall,
};
use bray_ir::{
    MirGeneratedLifecycleKey, MirGeneratedLifecycleRole, MirHelperReference, MirTargetContract,
    MirUnitKey,
};
use bray_symbols::{
    CallableInstanceData, CallableSignatureQuery, CheckedConstraintKind, ConstantTermData,
    ConstantValueData, ConstantValueKind, ExactSymbolId, GenericArgument, GenericConstraintsQuery,
    GenericOwnerId, GenericSubstitutionData, GenericSubstitutionId, ImplementationInstanceData,
    ImplementationInstanceId, ImplementationRequirementKey, ImplementationSelection,
    NamedTypeSymbolId, ProofOutcome, StaticInstanceKey, StaticReferenceSelection, StructSymbolId,
    SymbolQueryRequest, TargetSizedIntegerType, TraitCallableMemberSymbolId,
};

use super::super::CodegenPreparationError;
use super::super::Compilation;
use super::super::binder::binding_query_error;
use super::super::implementation::{
    callable_instance, implementation_fulfillments, implementation_instance_requirement,
    selected_callable,
};
use super::super::substitution::named_type;
use super::specialization_identity::encoding::structural_type_identity;
use crate::fact::{CancellationToken, FactQueryError};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct ConcreteCodegenInstance {
    key: CodegenInstanceKey,
    callable: Option<CallableInstanceData>,
    anonymous_callable_type: Option<bray_symbols::TypeId>,
    static_initializer: Option<bray_symbols::StaticSymbolId>,
    lifecycle: Option<MirHelperReference>,
    substitution: Option<GenericSubstitutionId>,
    witnesses: Arc<[ImplementationInstanceId]>,
}

pub(super) struct ConcreteCodegenReachability {
    graph: CodegenReachability,
    instances: BTreeMap<CodegenInstanceKey, ConcreteCodegenInstance>,
}

pub(super) enum ConcreteCodegenCallee {
    Instance(ConcreteCodegenInstance),
    Intrinsic(IntrinsicCall),
}

impl ConcreteCodegenReachability {
    pub(super) fn new(
        graph: CodegenReachability,
        instances: BTreeMap<CodegenInstanceKey, ConcreteCodegenInstance>,
    ) -> Self {
        Self { graph, instances }
    }

    pub(super) const fn graph(&self) -> &CodegenReachability {
        &self.graph
    }

    pub(super) fn instance(&self, key: &CodegenInstanceKey) -> Option<&ConcreteCodegenInstance> {
        self.instances.get(key)
    }
}

impl ConcreteCodegenInstance {
    pub(super) fn try_callable(
        key: CodegenInstanceKey,
        callable: CallableInstanceData,
        specialization: CodegenSpecialization,
        witnesses: impl IntoIterator<Item = (CodegenImplementationWitness, ImplementationInstanceId)>,
    ) -> Option<Self> {
        let mut witnesses: Vec<_> = witnesses.into_iter().collect();

        witnesses.sort_unstable_by(|left, right| left.0.cmp(&right.0));

        let identities: Vec<_> = witnesses
            .iter()
            .map(|(identity, _)| identity.clone())
            .collect();

        if key.specialization() != &specialization || key.witnesses() != identities {
            return None;
        }

        Some(Self {
            key,
            callable: Some(callable),
            anonymous_callable_type: None,
            static_initializer: None,
            lifecycle: None,
            substitution: Some(callable.substitution()),
            witnesses: witnesses
                .into_iter()
                .map(|(_, witness)| witness)
                .collect::<Vec<_>>()
                .into(),
        })
    }

    pub(super) fn generated(key: CodegenInstanceKey) -> Self {
        Self {
            key,
            callable: None,
            anonymous_callable_type: None,
            static_initializer: None,
            lifecycle: None,
            substitution: None,
            witnesses: Arc::from([]),
        }
    }

    pub(super) fn anonymous_callable(
        owner: &Self,
        reference: &bray_ir::MirAnonymousCallableReference,
        callable_type: bray_symbols::TypeId,
    ) -> Self {
        let template = match reference {
            // The nested instance owns the source key beyond the parent MIR operation borrow.
            bray_ir::MirAnonymousCallableReference::Bound(key) => MirUnitKey::Bound(key.clone()),
            bray_ir::MirAnonymousCallableReference::Imported(key) => {
                MirUnitKey::ImportedExecutable(*key)
            }
        };

        // The nested instance shares its parent's immutable specialization context.
        Self {
            key: CodegenInstanceKey::new(
                template,
                owner.key.specialization().clone(),
                owner.key.witnesses().iter().cloned(),
                owner.key.target().clone(),
            ),
            callable: None,
            anonymous_callable_type: Some(callable_type),
            static_initializer: None,
            lifecycle: None,
            substitution: owner.substitution,
            witnesses: Arc::clone(&owner.witnesses),
        }
    }

    pub(super) fn bound_helper(owner: &Self, unit: bray_bound_tree::BoundUnitKey) -> Self {
        Self {
            key: CodegenInstanceKey::new(
                MirUnitKey::Bound(unit),
                owner.key.specialization().clone(),
                owner.key.witnesses().iter().cloned(),
                owner.key.target().clone(),
            ),
            callable: None,
            anonymous_callable_type: None,
            static_initializer: None,
            lifecycle: None,
            substitution: owner.substitution,
            witnesses: Arc::clone(&owner.witnesses),
        }
    }

    pub(super) fn static_initializer(
        template: MirUnitKey,
        declaration: bray_symbols::StaticSymbolId,
        substitution: GenericSubstitutionId,
        specialization: CodegenSpecialization,
        witnesses: &[(CodegenImplementationWitness, ImplementationInstanceId)],
        target: MirTargetContract,
    ) -> Self {
        Self {
            key: CodegenInstanceKey::new(
                template,
                specialization,
                witnesses.iter().map(|(identity, _)| identity.clone()),
                target,
            ),
            callable: None,
            anonymous_callable_type: None,
            static_initializer: Some(declaration),
            lifecycle: None,
            substitution: Some(substitution),
            witnesses: witnesses
                .iter()
                .map(|(_, witness)| *witness)
                .collect::<Vec<_>>()
                .into(),
        }
    }

    pub(super) fn imported_runtime_default(
        owner: &Self,
        provider: bray_symbols::AnySymbolId,
    ) -> Self {
        Self {
            key: CodegenInstanceKey::new(
                MirUnitKey::ImportedExecutable(bray_ir::MirImportedExecutableKey::new(
                    provider,
                    bray_ir::MirExecutableTemplateId::ROOT,
                )),
                owner.key.specialization().clone(),
                owner.key.witnesses().iter().cloned(),
                owner.key.target().clone(),
            ),
            callable: None,
            anonymous_callable_type: None,
            static_initializer: None,
            lifecycle: None,
            substitution: owner.substitution,
            witnesses: Arc::clone(&owner.witnesses),
        }
    }

    pub(super) fn try_generated_lifecycle(
        key: CodegenInstanceKey,
        reference: MirHelperReference,
    ) -> Option<Self> {
        let MirUnitKey::GeneratedLifecycle(template) = key.template() else {
            return None;
        };

        if Some(template.role()) != MirGeneratedLifecycleRole::from_reference(&reference) {
            return None;
        }

        Some(Self {
            key,
            callable: None,
            anonymous_callable_type: None,
            static_initializer: None,
            lifecycle: Some(reference),
            substitution: None,
            witnesses: Arc::from([]),
        })
    }

    pub(super) const fn key(&self) -> &CodegenInstanceKey {
        &self.key
    }

    pub(super) fn callable_instance(&self) -> Option<CallableInstanceData> {
        self.callable
    }

    pub(super) const fn anonymous_callable_type(&self) -> Option<bray_symbols::TypeId> {
        self.anonymous_callable_type
    }

    pub(super) const fn static_declaration(&self) -> Option<bray_symbols::StaticSymbolId> {
        self.static_initializer
    }

    pub(super) fn substitution(&self) -> Option<GenericSubstitutionId> {
        self.substitution
    }

    pub(super) const fn generated_lifecycle_reference(&self) -> Option<&MirHelperReference> {
        self.lifecycle.as_ref()
    }

    pub(super) fn implementation_witnesses(&self) -> &[ImplementationInstanceId] {
        &self.witnesses
    }
}

impl Compilation {
    pub(super) fn concrete_codegen_static_selection(
        &self,
        owner: &ConcreteCodegenInstance,
        reference: &StaticReferenceSelection,
        cancellation: &CancellationToken,
    ) -> Result<
        (
            StaticInstanceKey,
            CodegenSpecialization,
            Vec<(CodegenImplementationWitness, ImplementationInstanceId)>,
        ),
        CodegenPreparationError,
    > {
        let values = self.semantic_value_store()?;

        let (template, substitution, witnesses, target) = match reference {
            StaticReferenceSelection::Open {
                template,
                substitution,
                selected_witnesses,
                target,
            } => {
                let substitution = match owner.substitution() {
                    Some(owner) => values
                        .substitute_generic_substitution(*substitution, owner)
                        .map_err(|_| FactQueryError::InfrastructureFailure)?,
                    None => *substitution,
                };

                let substitution = self.realize_codegen_substitution(substitution)?;
                let owner_substitution = owner.substitution();

                let selected_witnesses = if selected_witnesses.is_empty()
                    && owner.static_declaration() == Some(template.declaration())
                {
                    owner.implementation_witnesses()
                } else {
                    selected_witnesses
                };

                let witnesses = selected_witnesses
                    .iter()
                    .map(|witness| {
                        let data = values
                            .implementation_instance_data(*witness)
                            .map_err(|_| FactQueryError::InfrastructureFailure)?;

                        let nested = match owner_substitution {
                            Some(owner) => values
                                .substitute_generic_substitution(data.substitution(), owner)
                                .map_err(|_| FactQueryError::InfrastructureFailure)?,
                            None => data.substitution(),
                        };

                        let nested = self.realize_codegen_substitution(nested)?;

                        values
                            .intern_implementation_instance(ImplementationInstanceData::new(
                                data.definition(),
                                nested,
                            ))
                            .map_err(|_| FactQueryError::InfrastructureFailure.into())
                    })
                    .collect::<Result<Vec<_>, CodegenPreparationError>>()?;

                (*template, substitution, witnesses, target.clone())
            }
            StaticReferenceSelection::Closed(instance) => (
                instance.template(),
                instance.substitution().substitution(),
                instance.selected_witnesses().to_vec(),
                instance.target().clone(),
            ),
        };

        let substitution = self.realize_codegen_substitution(substitution)?;

        let concrete_substitution = values
            .require_concrete_substitution(substitution)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let instance = StaticInstanceKey::new(
            template,
            concrete_substitution,
            witnesses.iter().copied(),
            target,
        );

        let specialization = self.codegen_specialization(substitution)?;
        let witnesses = self.concrete_codegen_witnesses(witnesses, cancellation)?;

        Ok((instance, specialization, witnesses))
    }

    pub(super) fn concrete_codegen_anonymous_callable(
        &self,
        owner: &ConcreteCodegenInstance,
        reference: &bray_ir::MirAnonymousCallableReference,
        callable_type: bray_symbols::TypeId,
    ) -> Result<ConcreteCodegenInstance, CodegenPreparationError> {
        Ok(ConcreteCodegenInstance::anonymous_callable(
            owner,
            reference,
            callable_type,
        ))
    }

    pub(super) fn concrete_codegen_bound_helper(
        &self,
        owner: &ConcreteCodegenInstance,
        unit: bray_bound_tree::BoundUnitKey,
    ) -> Result<ConcreteCodegenInstance, CodegenPreparationError> {
        Ok(ConcreteCodegenInstance::bound_helper(owner, unit))
    }

    pub(super) fn concrete_codegen_lifecycle(
        &self,
        reference: MirHelperReference,
        target: &CodegenTarget,
    ) -> Result<ConcreteCodegenInstance, CodegenPreparationError> {
        let role = MirGeneratedLifecycleRole::from_reference(&reference)
            .ok_or_else(|| CodegenPreparationError::MissingHelperInstance(reference.clone()))?;

        let ty = reference
            .lifecycle_type()
            .ok_or_else(|| CodegenPreparationError::MissingHelperInstance(reference.clone()))?;

        let binding_context = self.binding_context(&self.state.cancellation)?;

        let identity =
            structural_type_identity(self.semantic_value_store()?, &binding_context, ty)?;

        let key = CodegenInstanceKey::new(
            MirUnitKey::GeneratedLifecycle(MirGeneratedLifecycleKey::new(role, identity)),
            CodegenSpecialization::NonGeneric,
            [],
            MirTargetContract::new(
                target.profile().clone(),
                self.selected_target().target().runtime_abi(),
            ),
        );

        ConcreteCodegenInstance::try_generated_lifecycle(key, reference)
            .ok_or_else(|| FactQueryError::InfrastructureFailure.into())
    }

    pub(super) fn concrete_codegen_callable(
        &self,
        callable: CallableInstanceData,
        witnesses: impl IntoIterator<Item = ImplementationInstanceId>,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<ConcreteCodegenInstance, CodegenPreparationError> {
        cancellation.check()?;

        let substitution = self.realize_codegen_substitution(callable.substitution())?;
        let callable = CallableInstanceData::new(callable.definition(), substitution);

        let template = self.codegen_callable_template(callable.definition(), cancellation)?;

        let specialization = self.codegen_specialization(callable.substitution())?;
        let witnesses = self.concrete_codegen_witnesses(witnesses, cancellation)?;

        let key = CodegenInstanceKey::new(
            template,
            specialization.clone(),
            witnesses.iter().map(|(identity, _)| identity.clone()),
            MirTargetContract::new(
                target.profile().clone(),
                self.selected_target().target().runtime_abi(),
            ),
        );

        ConcreteCodegenInstance::try_callable(key, callable, specialization, witnesses)
            .ok_or_else(|| FactQueryError::InfrastructureFailure.into())
    }

    fn codegen_callable_template(
        &self,
        definition: bray_symbols::CallableDefinitionId,
        cancellation: &CancellationToken,
    ) -> Result<MirUnitKey, CodegenPreparationError> {
        if let Some(body) = self.callable_body_key(definition)? {
            return Ok(MirUnitKey::Bound(body));
        }

        let skeleton = self.imported_symbol_skeleton_result_with_cancellation(cancellation)?;

        let Some(address) = skeleton.value().as_ref().and_then(|skeleton| {
            skeleton.imported_semantic_address(definition.callable_symbol().into_any())
        }) else {
            return Ok(MirUnitKey::ExternalCallable(definition));
        };

        let binding_context = self.binding_context(cancellation)?;

        let signature = binding_context
            .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(
                definition.callable_symbol(),
            ))
            .map_err(binding_query_error)?;

        if !signature.value().has_body() {
            return Ok(MirUnitKey::ExternalCallable(definition));
        }

        let template = self.imported_executable_template_with_cancellation(
            crate::fact::ImportedExecutableTemplateAddress::root(address),
            cancellation,
        )?;

        if let Some(template) = template.value() {
            let MirUnitKey::ImportedExecutable(key) = template.key() else {
                return Err(FactQueryError::ImportedExecutableTemplateMismatch.into());
            };

            return Ok(MirUnitKey::ImportedExecutable(*key));
        }

        Err(CodegenPreparationError::Diagnostics(
            signature.diagnostics().merged(template.diagnostics()),
        ))
    }

    pub(super) fn concrete_codegen_callee(
        &self,
        owner: &ConcreteCodegenInstance,
        demand: &DemandedCallableInstance,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<ConcreteCodegenCallee, CodegenPreparationError> {
        if demand.trait_dispatch().is_some() {
            return self.concrete_codegen_generic_callee(owner, demand, target, cancellation);
        }

        let values = self.semantic_value_store()?;
        let reference = demand.reference();
        let callable = reference.instance();

        let (callable, mut witnesses) = match owner.substitution() {
            Some(owner_substitution) => {
                let substitution = values
                    .substitute_generic_substitution(callable.substitution(), owner_substitution)
                    .map_err(|_| FactQueryError::InfrastructureFailure)?;

                let callable = CallableInstanceData::new(callable.definition(), substitution);

                let witnesses =
                    self.concrete_codegen_demand_witnesses(demand.witnesses(), owner_substitution)?;

                (callable, witnesses)
            }
            None => {
                values
                    .require_concrete_substitution(callable.substitution())
                    .map_err(|_| FactQueryError::InfrastructureFailure)?;

                (callable, demand.witnesses().to_vec())
            }
        };

        witnesses.extend(self.concrete_codegen_forwarded_constraint_witnesses(
            owner,
            &callable,
            &witnesses,
            cancellation,
        )?);

        self.concrete_codegen_callable(callable, witnesses, target, cancellation)
            .map(ConcreteCodegenCallee::Instance)
    }

    fn concrete_codegen_generic_callee(
        &self,
        owner: &ConcreteCodegenInstance,
        demand: &DemandedCallableInstance,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<ConcreteCodegenCallee, CodegenPreparationError> {
        cancellation.check()?;

        let dispatch = demand
            .trait_dispatch()
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let owner_substitution = owner
            .substitution()
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let member = TraitCallableMemberSymbolId::try_from_any(
            demand.reference().instance().definition().symbol(),
        )
        .ok_or(FactQueryError::InfrastructureFailure)?;

        let values = self.semantic_value_store()?;
        let binding_context = self.binding_context(cancellation)?;

        let constraints = binding_context
            .resolve_symbol_query(SymbolQueryRequest::<GenericConstraintsQuery>::new(
                dispatch.owner(),
            ))
            .map_err(binding_query_error)?;

        let constraint = constraints
            .value()
            .constraints()
            .iter()
            .find(|constraint| constraint.ordinal() == dispatch.ordinal())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let CheckedConstraintKind::TraitSatisfaction {
            subject,
            application,
        } = constraint.kind()
        else {
            return Err(FactQueryError::InfrastructureFailure.into());
        };

        let subject = values
            .substitute_type(subject, owner_substitution)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let application = values
            .substitute_trait_application(application, owner_substitution)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let requirement = ImplementationRequirementKey::new(subject, application);

        let witness = self.concrete_codegen_dispatch_witness(owner, requirement, cancellation)?;

        let Some(witness) = witness else {
            let intrinsic = demand
                .intrinsic()
                .ok_or(FactQueryError::InfrastructureFailure)?;

            let context = super::super::checker::CompilationCheckerContext::new(binding_context);

            let outcome =
                bray_checker::built_in_trait_constraint_outcome(&context, subject, application)
                    .map_err(FactQueryError::CheckerInfrastructure)?;

            if outcome != Some(ProofOutcome::Proven) {
                return Err(FactQueryError::InfrastructureFailure.into());
            }

            let intrinsic = match intrinsic {
                bray_ir::MirCallIntrinsic::Unary(operator) => IntrinsicCall::Unary(operator),
                bray_ir::MirCallIntrinsic::Binary(operator) => IntrinsicCall::Binary(operator),
                bray_ir::MirCallIntrinsic::Conversion(target) => {
                    let target = values
                        .substitute_type(target, owner_substitution)
                        .map_err(|_| FactQueryError::InfrastructureFailure)?;

                    let conversion = bray_checker::built_in_conversion_plan_for_context(
                        &context, subject, target,
                    )
                    .map_err(FactQueryError::CheckerInfrastructure)?
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                    IntrinsicCall::Conversion(conversion)
                }
            };

            return Ok(ConcreteCodegenCallee::Intrinsic(intrinsic));
        };

        let implementation = values
            .implementation_instance_data(witness)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let fulfillments =
            implementation_fulfillments(&binding_context, implementation.definition())?;

        let fulfillment = selected_callable(&binding_context, fulfillments.callables, member)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let application = values
            .trait_application_data(application)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let callable = callable_instance(
            values,
            fulfillment.into(),
            [application.substitution(), implementation.substitution()],
        )?;

        let mut witnesses =
            self.concrete_codegen_demand_witnesses(demand.witnesses(), owner_substitution)?;

        witnesses.extend(
            self.concrete_codegen_implementation_constraint_witnesses(witness, cancellation)?,
        );

        witnesses.push(witness);

        self.concrete_codegen_callable(callable, witnesses, target, cancellation)
            .map(ConcreteCodegenCallee::Instance)
    }

    fn concrete_codegen_dispatch_witness(
        &self,
        owner: &ConcreteCodegenInstance,
        requirement: ImplementationRequirementKey,
        cancellation: &CancellationToken,
    ) -> Result<Option<ImplementationInstanceId>, CodegenPreparationError> {
        self.concrete_codegen_matching_witness(
            owner.implementation_witnesses().iter().copied(),
            requirement,
            cancellation,
        )
    }

    fn concrete_codegen_matching_witness(
        &self,
        witnesses: impl IntoIterator<Item = ImplementationInstanceId>,
        requirement: ImplementationRequirementKey,
        cancellation: &CancellationToken,
    ) -> Result<Option<ImplementationInstanceId>, CodegenPreparationError> {
        for witness in witnesses {
            if requirement == self.concrete_codegen_witness_requirement(witness, cancellation)? {
                return Ok(Some(witness));
            }
        }

        Ok(None)
    }

    fn concrete_codegen_witness_requirement(
        &self,
        witness: ImplementationInstanceId,
        cancellation: &CancellationToken,
    ) -> Result<ImplementationRequirementKey, CodegenPreparationError> {
        let binding_context = self.binding_context(cancellation)?;

        implementation_instance_requirement(&binding_context, witness)
            .map_err(CodegenPreparationError::from)
    }

    fn concrete_codegen_forwarded_constraint_witnesses(
        &self,
        owner: &ConcreteCodegenInstance,
        callable: &CallableInstanceData,
        direct_witnesses: &[ImplementationInstanceId],
        cancellation: &CancellationToken,
    ) -> Result<Vec<ImplementationInstanceId>, CodegenPreparationError> {
        let Some(generic_owner) =
            GenericOwnerId::try_new(callable.definition().callable_symbol().into_any())
        else {
            return Ok(Vec::new());
        };

        let requirements = self.concrete_codegen_constraint_requirements(
            generic_owner,
            callable.substitution(),
            cancellation,
        )?;

        let mut forwarded = Vec::new();

        for requirement in requirements {
            let available = direct_witnesses
                .iter()
                .copied()
                .chain(owner.implementation_witnesses().iter().copied());

            let witness =
                self.concrete_codegen_requirement_witness(available, requirement, cancellation)?;

            forwarded.push(witness);
        }

        Ok(forwarded)
    }

    fn concrete_codegen_implementation_constraint_witnesses(
        &self,
        implementation: ImplementationInstanceId,
        cancellation: &CancellationToken,
    ) -> Result<Vec<ImplementationInstanceId>, CodegenPreparationError> {
        let values = self.semantic_value_store()?;

        let instance = values
            .implementation_instance_data(implementation)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let owner = GenericOwnerId::try_new(instance.definition().into_any())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let requirements = self.concrete_codegen_constraint_requirements(
            owner,
            instance.substitution(),
            cancellation,
        )?;

        let mut witnesses = Vec::new();

        for requirement in requirements {
            witnesses.push(self.concrete_codegen_requirement_witness(
                std::iter::empty(),
                requirement,
                cancellation,
            )?);
        }

        Ok(witnesses)
    }

    fn concrete_codegen_requirement_witness(
        &self,
        available: impl IntoIterator<Item = ImplementationInstanceId>,
        requirement: ImplementationRequirementKey,
        cancellation: &CancellationToken,
    ) -> Result<ImplementationInstanceId, CodegenPreparationError> {
        if let Some(witness) =
            self.concrete_codegen_matching_witness(available, requirement, cancellation)?
        {
            return Ok(witness);
        }

        let selection =
            self.implementation_selection_result_with_cancellation(requirement, cancellation)?;

        if selection.diagnostics().has_errors() {
            return Err(CodegenPreparationError::Diagnostics(
                selection.diagnostics().clone(),
            ));
        }

        let ImplementationSelection::Selected(witness) = selection.value() else {
            return Err(FactQueryError::InfrastructureFailure.into());
        };

        Ok(*witness)
    }

    fn concrete_codegen_constraint_requirements(
        &self,
        owner: GenericOwnerId,
        substitution: GenericSubstitutionId,
        cancellation: &CancellationToken,
    ) -> Result<Vec<ImplementationRequirementKey>, CodegenPreparationError> {
        let values = self.semantic_value_store()?;
        let binding_context = self.binding_context(cancellation)?;

        let constraints = binding_context
            .resolve_symbol_query(SymbolQueryRequest::<GenericConstraintsQuery>::new(owner))
            .map_err(binding_query_error)?;

        if constraints.diagnostics().has_errors() {
            return Err(CodegenPreparationError::Diagnostics(
                constraints.diagnostics().clone(),
            ));
        }

        let context = super::super::checker::CompilationCheckerContext::new(binding_context);
        let mut requirements = Vec::new();

        for constraint in constraints.value().constraints() {
            let CheckedConstraintKind::TraitSatisfaction {
                subject,
                application,
            } = constraint.kind()
            else {
                continue;
            };

            let subject = values
                .substitute_type(subject, substitution)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            let application = values
                .substitute_trait_application(application, substitution)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            let application_data = values
                .trait_application_data(application)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            if bray_checker::built_in_trait_constraint_outcome(&context, subject, application)
                .map_err(FactQueryError::CheckerInfrastructure)?
                == Some(ProofOutcome::Proven)
                || self.is_copyable_trait(application_data.definition())?
            {
                continue;
            }

            requirements.push(ImplementationRequirementKey::new(subject, application));
        }

        Ok(requirements)
    }

    fn concrete_codegen_demand_witnesses(
        &self,
        witnesses: &[ImplementationInstanceId],
        owner_substitution: GenericSubstitutionId,
    ) -> Result<Vec<ImplementationInstanceId>, CodegenPreparationError> {
        let values = self.semantic_value_store()?;

        witnesses
            .iter()
            .map(|witness| -> Result<_, CodegenPreparationError> {
                let data = values
                    .implementation_instance_data(*witness)
                    .map_err(|_| FactQueryError::InfrastructureFailure)?;

                let substitution = values
                    .substitute_generic_substitution(data.substitution(), owner_substitution)
                    .map_err(|_| FactQueryError::InfrastructureFailure)?;

                let substitution = self.realize_codegen_substitution(substitution)?;

                values
                    .intern_implementation_instance(ImplementationInstanceData::new(
                        data.definition(),
                        substitution,
                    ))
                    .map_err(|_| FactQueryError::InfrastructureFailure.into())
            })
            .collect()
    }

    pub(super) fn concrete_codegen_callable_data(
        &self,
        owner: &ConcreteCodegenInstance,
        callable: &CallableInstanceData,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<ConcreteCodegenInstance, CodegenPreparationError> {
        let values = self.semantic_value_store()?;

        let substitution = match owner.substitution() {
            Some(owner_substitution) => values
                .substitute_generic_substitution(callable.substitution(), owner_substitution)
                .map_err(|_| FactQueryError::InfrastructureFailure)?,
            None => callable.substitution(),
        };

        self.concrete_codegen_callable(
            CallableInstanceData::new(callable.definition(), substitution),
            [],
            target,
            cancellation,
        )
    }

    fn realize_codegen_substitution(
        &self,
        substitution: GenericSubstitutionId,
    ) -> Result<GenericSubstitutionId, CodegenPreparationError> {
        let values = self.semantic_value_store()?;

        let data = values
            .generic_substitution_data(substitution)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let arguments = data
            .bindings()
            .iter()
            .map(|binding| match binding.argument() {
                GenericArgument::Type(ty) => Ok(GenericArgument::Type(ty)),
                GenericArgument::Constant(term) => self
                    .realize_codegen_constant_argument(term)
                    .map(GenericArgument::Constant),
            })
            .collect::<Result<Vec<_>, CodegenPreparationError>>()?;

        let realized = GenericSubstitutionData::try_new(
            data.owner(),
            data.bindings().iter().map(|binding| binding.parameter()),
            arguments,
        )
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let realized = values
            .intern_generic_substitution(realized)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        values
            .require_concrete_substitution(realized)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        Ok(realized)
    }

    pub(super) fn realize_codegen_constant_argument(
        &self,
        term: bray_symbols::ConstantTermId,
    ) -> Result<bray_symbols::ConstantTermId, CodegenPreparationError> {
        let values = self.semantic_value_store()?;

        let data = values
            .constant_term_data(term)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let ConstantTermData::IntegerLiteral { ty, value } = data.as_ref() else {
            return Ok(term);
        };

        let role = match ty {
            TargetSizedIntegerType::Isize => bray_compiler_known::RepresentationRole::ScalarIsize,
            TargetSizedIntegerType::Usize => bray_compiler_known::RepresentationRole::ScalarUsize,
        };

        let definition = self
            .available_compiler_known_symbols()
            .representation_symbol::<StructSymbolId>(role)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let ty = named_type(values, NamedTypeSymbolId::Struct(definition))?;

        let value = values
            .intern_constant_value(ConstantValueData::new(
                ty,
                ConstantValueKind::Integer(value.clone()),
            ))
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        values
            .intern_constant_term(ConstantTermData::Value(value))
            .map_err(|_| FactQueryError::InfrastructureFailure.into())
    }

    pub(super) fn concrete_codegen_witnesses(
        &self,
        witnesses: impl IntoIterator<Item = ImplementationInstanceId>,
        cancellation: &CancellationToken,
    ) -> Result<
        Vec<(CodegenImplementationWitness, ImplementationInstanceId)>,
        CodegenPreparationError,
    > {
        let values = self.semantic_value_store()?;
        let binding_context = self.binding_context(cancellation)?;
        let mut concrete = BTreeMap::new();

        for witness in witnesses {
            let data = values
                .implementation_instance_data(witness)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            values
                .require_concrete_substitution(data.substitution())
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            let definition =
                self.portable_codegen_symbol_key(&binding_context, data.definition().into_any())?;

            let specialization = self.codegen_specialization(data.substitution())?;

            let identity = CodegenImplementationWitness::try_new(definition, specialization)
                .ok_or(FactQueryError::InfrastructureFailure)?;

            match concrete.insert(identity, witness) {
                Some(existing) if existing != witness => {
                    return Err(FactQueryError::InfrastructureFailure.into());
                }
                Some(_) | None => {}
            }
        }

        Ok(concrete.into_iter().collect())
    }
}
