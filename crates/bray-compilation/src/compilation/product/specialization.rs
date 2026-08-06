use std::collections::BTreeMap;
use std::sync::Arc;

use bray_binder::SymbolFactProvider;
use bray_codegen::{
    CodegenImplementationWitness, CodegenInstanceKey, CodegenReachability, CodegenSpecialization,
    CodegenTarget, DemandedCallableInstance,
};
use bray_ir::{
    MirGeneratedLifecycleKey, MirGeneratedLifecycleRole, MirHelperReference, MirTargetFacts,
    MirUnitKey,
};
use bray_symbols::{
    CallableInstanceData, CheckedConstraintKind, ConstantTermData, ConstantValueData,
    ConstantValueKind, ExactSymbolId, GenericArgument, GenericConstraintsFact,
    GenericSubstitutionData, GenericSubstitutionId, ImplementationInstanceData,
    ImplementationInstanceId, ImplementationRequirementKey, ImplementationSelection,
    NamedTypeSymbolId, StructSymbolId, SymbolFactRequest, TargetSizedIntegerType,
    TraitCallableMemberSymbolId,
};

use super::super::CodegenFactError;
use super::super::Compilation;
use super::super::binder::binder_fact_error;
use super::super::implementation::{
    callable_instance, implementation_fulfillments, selected_callable,
};
use super::super::substitution::named_type;
use super::specialization_identity::encoding::structural_type_identity;
use crate::fact::{CancellationToken, FactQueryError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ConcreteCodegenInstance {
    key: CodegenInstanceKey,
    callable: Option<CallableInstanceData>,
    lifecycle: Option<MirHelperReference>,
    substitution: Option<GenericSubstitutionId>,
    witnesses: Arc<[ImplementationInstanceId]>,
}

pub(super) struct ConcreteCodegenReachability {
    graph: CodegenReachability,
    instances: BTreeMap<CodegenInstanceKey, ConcreteCodegenInstance>,
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
            lifecycle: None,
            substitution: None,
            witnesses: Arc::from([]),
        }
    }

    pub(super) fn bound_helper(owner: &Self, template: bray_bound_tree::BoundUnitKey) -> Self {
        Self {
            key: CodegenInstanceKey::new(
                MirUnitKey::Bound(template),
                owner.key.specialization().clone(),
                owner.key.witnesses().iter().cloned(),
                owner.key.target().clone(),
            ),
            callable: None,
            lifecycle: None,
            substitution: owner.substitution,
            witnesses: Arc::clone(&owner.witnesses),
        }
    }

    pub(super) fn external_runtime_default(
        owner: &Self,
        provider: bray_symbols::AnySymbolId,
    ) -> Self {
        Self {
            key: CodegenInstanceKey::new(
                MirUnitKey::ExternalRuntimeDefault(provider),
                owner.key.specialization().clone(),
                owner.key.witnesses().iter().cloned(),
                owner.key.target().clone(),
            ),
            callable: None,
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

        if Some(template.role()) != bray_ir::MirGeneratedLifecycleRole::from_reference(&reference) {
            return None;
        }

        Some(Self {
            key,
            callable: None,
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

    pub(super) fn substitution(&self) -> Option<GenericSubstitutionId> {
        self.substitution
    }

    pub(super) const fn generated_lifecycle_reference(&self) -> Option<&MirHelperReference> {
        self.lifecycle.as_ref()
    }

    #[cfg(test)]
    pub(super) fn witness_instances(&self) -> &[ImplementationInstanceId] {
        &self.witnesses
    }
}

impl Compilation {
    pub(super) fn concrete_codegen_bound_helper(
        &self,
        owner: &ConcreteCodegenInstance,
        template: bray_bound_tree::BoundUnitKey,
    ) -> Result<ConcreteCodegenInstance, CodegenFactError> {
        Ok(ConcreteCodegenInstance::bound_helper(owner, template))
    }

    pub(super) fn concrete_codegen_lifecycle(
        &self,
        reference: MirHelperReference,
        target: &CodegenTarget,
    ) -> Result<ConcreteCodegenInstance, CodegenFactError> {
        let role = MirGeneratedLifecycleRole::from_reference(&reference)
            .ok_or_else(|| CodegenFactError::MissingHelperInstance(reference.clone()))?;

        let ty = reference
            .lifecycle_type()
            .ok_or_else(|| CodegenFactError::MissingHelperInstance(reference.clone()))?;

        let facts = self.binder_facts(&self.state.cancellation)?;
        let identity = structural_type_identity(self.semantic_value_store()?, &facts, ty)?;

        let key = CodegenInstanceKey::new(
            MirUnitKey::GeneratedLifecycle(MirGeneratedLifecycleKey::new(role, identity)),
            CodegenSpecialization::NonGeneric,
            [],
            MirTargetFacts::new(
                target.profile().clone(),
                self.selected_target().target().runtime_abi(),
            ),
        );

        ConcreteCodegenInstance::try_generated_lifecycle(key, reference)
            .ok_or(FactQueryError::InfrastructureFailure.into())
    }

    pub(super) fn concrete_codegen_callable(
        &self,
        callable: CallableInstanceData,
        witnesses: impl IntoIterator<Item = ImplementationInstanceId>,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<ConcreteCodegenInstance, CodegenFactError> {
        cancellation.check()?;

        let callable = CallableInstanceData::new(
            callable.definition(),
            self.realize_codegen_substitution(callable.substitution())?,
        );

        let template = self
            .callable_body_key(callable.definition())?
            .map(MirUnitKey::Bound)
            .unwrap_or_else(|| MirUnitKey::ExternalCallable(callable.definition()));

        let specialization = self.codegen_specialization(callable.substitution())?;
        let witnesses = self.concrete_codegen_witnesses(witnesses, cancellation)?;

        let key = CodegenInstanceKey::new(
            template,
            specialization.clone(),
            witnesses.iter().map(|(identity, _)| identity.clone()),
            MirTargetFacts::new(
                target.profile().clone(),
                self.selected_target().target().runtime_abi(),
            ),
        );

        ConcreteCodegenInstance::try_callable(key, callable, specialization, witnesses)
            .ok_or(FactQueryError::InfrastructureFailure.into())
    }

    pub(super) fn concrete_codegen_callee(
        &self,
        owner: &ConcreteCodegenInstance,
        demand: &DemandedCallableInstance,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<ConcreteCodegenInstance, CodegenFactError> {
        if demand.trait_dispatch().is_some() {
            return self.concrete_codegen_generic_callee(owner, demand, target, cancellation);
        }

        let values = self.semantic_value_store()?;
        let reference = demand.reference();
        let callable = reference.instance();

        let Some(owner_substitution) = owner.substitution() else {
            values
                .require_concrete_substitution(callable.substitution())
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            return self.concrete_codegen_callable(
                callable,
                demand.witnesses().iter().copied(),
                target,
                cancellation,
            );
        };

        let substitution = values
            .substitute_generic_substitution(callable.substitution(), owner_substitution)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let callable = CallableInstanceData::new(callable.definition(), substitution);

        let witnesses =
            self.concrete_codegen_demand_witnesses(demand.witnesses(), owner_substitution)?;

        self.concrete_codegen_callable(callable, witnesses, target, cancellation)
    }

    fn concrete_codegen_generic_callee(
        &self,
        owner: &ConcreteCodegenInstance,
        demand: &DemandedCallableInstance,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<ConcreteCodegenInstance, CodegenFactError> {
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
        let facts = self.binder_facts(cancellation)?;

        let constraints = facts
            .symbol_fact(SymbolFactRequest::<GenericConstraintsFact>::new(
                dispatch.owner(),
            ))
            .map_err(binder_fact_error)?;

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

        let selection =
            self.implementation_selection_result_with_cancellation(requirement, cancellation)?;

        let ImplementationSelection::Selected(witness) = selection.value() else {
            return Err(FactQueryError::InfrastructureFailure.into());
        };

        let implementation = values
            .implementation_instance_data(*witness)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let fulfillments = implementation_fulfillments(&facts, implementation.definition())?;

        let fulfillment = selected_callable(&facts, fulfillments.callables, member)
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

        witnesses.push(*witness);

        self.concrete_codegen_callable(callable, witnesses, target, cancellation)
    }

    fn concrete_codegen_demand_witnesses(
        &self,
        witnesses: &[ImplementationInstanceId],
        owner_substitution: GenericSubstitutionId,
    ) -> Result<Vec<ImplementationInstanceId>, CodegenFactError> {
        let values = self.semantic_value_store()?;

        witnesses
            .iter()
            .map(|witness| -> Result<_, CodegenFactError> {
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
    ) -> Result<ConcreteCodegenInstance, CodegenFactError> {
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
    ) -> Result<GenericSubstitutionId, CodegenFactError> {
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
            .collect::<Result<Vec<_>, CodegenFactError>>()?;

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
    ) -> Result<bray_symbols::ConstantTermId, CodegenFactError> {
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

    fn concrete_codegen_witnesses(
        &self,
        witnesses: impl IntoIterator<Item = ImplementationInstanceId>,
        cancellation: &CancellationToken,
    ) -> Result<Vec<(CodegenImplementationWitness, ImplementationInstanceId)>, CodegenFactError>
    {
        let values = self.semantic_value_store()?;
        let facts = self.binder_facts(cancellation)?;
        let mut concrete = BTreeMap::new();

        for witness in witnesses {
            let data = values
                .implementation_instance_data(witness)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            values
                .require_concrete_substitution(data.substitution())
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            let definition =
                self.portable_codegen_symbol_key(&facts, data.definition().into_any())?;

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
