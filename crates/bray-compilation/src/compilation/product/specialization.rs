use std::collections::BTreeMap;
use std::sync::Arc;

use bray_codegen::{
    CodegenImplementationWitness, CodegenInstanceKey, CodegenReachability,
    CodegenSpecialization, CodegenTarget, DemandedCallableInstance,
};
use bray_ir::{
    MirGeneratedLifecycleKey, MirGeneratedLifecycleRole, MirHelperReference,
    MirTargetFacts, MirUnitKey,
};
use bray_symbols::{
    CallableInstanceData, ConstantTermData, ConstantValueData, ConstantValueKind,
    GenericArgument, GenericSubstitutionData, GenericSubstitutionId,
    ImplementationInstanceData, ImplementationInstanceId, NamedTypeSymbolId,
    StructSymbolId, TargetSizedIntegerType,
};

use super::super::Compilation;
use super::super::CodegenFactError;
use super::super::substitution::{empty_substitution, named_type};
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

    pub(super) fn instance(
        &self,
        key: &CodegenInstanceKey,
    ) -> Option<&ConcreteCodegenInstance> {
        self.instances.get(key)
    }

}

impl ConcreteCodegenInstance {
    pub(super) fn try_callable(
        key: CodegenInstanceKey,
        callable: CallableInstanceData,
        specialization: CodegenSpecialization,
        witnesses: impl IntoIterator<
            Item = (CodegenImplementationWitness, ImplementationInstanceId),
        >,
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

    pub(super) fn bound_helper(
        owner: &Self,
        template: bray_bound_tree::BoundUnitKey,
        callable: CallableInstanceData,
    ) -> Self {
        Self {
            key: CodegenInstanceKey::new(
                MirUnitKey::Bound(template),
                owner.key.specialization().clone(),
                owner.key.witnesses().iter().cloned(),
                owner.key.target().clone(),
            ),
            callable: Some(callable),
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

        if Some(template.role())
            != bray_ir::MirGeneratedLifecycleRole::from_reference(&reference)
        {
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

    pub(super) const fn generated_lifecycle_reference(
        &self,
    ) -> Option<&MirHelperReference> {
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
        let symbol = self
            .symbol_graph()?
            .symbol_for_key(template.declared_owner())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let definition = bray_symbols::CallableDefinitionId::try_new(symbol)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let substitution = match owner.substitution() {
            Some(substitution) => substitution,
            None => empty_substitution(
                self.semantic_value_store()?,
                definition.symbol(),
            )?,
        };

        Ok(ConcreteCodegenInstance::bound_helper(
            owner,
            template,
            CallableInstanceData::new(definition, substitution),
        ))
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

        let identity = structural_type_identity(
            self.semantic_value_store()?,
            self.symbol_graph()?,
            ty,
        )?;

        let key = CodegenInstanceKey::new(
            MirUnitKey::GeneratedLifecycle(MirGeneratedLifecycleKey::new(
                role, identity,
            )),
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
        let witnesses = self.concrete_codegen_witnesses(witnesses)?;

        let key = CodegenInstanceKey::new(
            template,
            specialization.clone(),
            witnesses.iter().map(|(identity, _)| identity.clone()),
            MirTargetFacts::new(
                target.profile().clone(),
                self.selected_target().target().runtime_abi(),
            ),
        );

        ConcreteCodegenInstance::try_callable(
            key,
            callable,
            specialization,
            witnesses,
        )
        .ok_or(FactQueryError::InfrastructureFailure.into())
    }

    pub(super) fn concrete_codegen_callee(
        &self,
        owner: &ConcreteCodegenInstance,
        demand: &DemandedCallableInstance,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<ConcreteCodegenInstance, CodegenFactError> {
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
            .substitute_generic_substitution(
                callable.substitution(),
                owner_substitution,
            )
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let callable = CallableInstanceData::new(callable.definition(), substitution);

        let witnesses = demand
            .witnesses()
            .iter()
            .map(|witness| -> Result<_, CodegenFactError> {
                let data = values
                    .implementation_instance_data(*witness)
                    .map_err(|_| FactQueryError::InfrastructureFailure)?;

                let substitution = values
                    .substitute_generic_substitution(
                        data.substitution(),
                        owner_substitution,
                    )
                    .map_err(|_| FactQueryError::InfrastructureFailure)?;

                let substitution = self.realize_codegen_substitution(substitution)?;

                values
                    .intern_implementation_instance(ImplementationInstanceData::new(
                        data.definition(),
                        substitution,
                    ))
                    .map_err(|_| FactQueryError::InfrastructureFailure.into())
            })
            .collect::<Result<Vec<_>, _>>()?;

        self.concrete_codegen_callable(callable, witnesses, target, cancellation)
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
                .substitute_generic_substitution(
                    callable.substitution(),
                    owner_substitution,
                )
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

    fn realize_codegen_constant_argument(
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
            TargetSizedIntegerType::Isize => {
                bray_compiler_known::RepresentationRole::ScalarIsize
            }
            TargetSizedIntegerType::Usize => {
                bray_compiler_known::RepresentationRole::ScalarUsize
            }
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
    ) -> Result<
        Vec<(CodegenImplementationWitness, ImplementationInstanceId)>,
        CodegenFactError,
    > {
        let values = self.semantic_value_store()?;
        let symbols = self.symbol_graph()?;
        let mut concrete = Vec::new();

        for witness in witnesses {
            let data = values
                .implementation_instance_data(witness)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            values
                .require_concrete_substitution(data.substitution())
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            let definition = symbols
                .symbol_key(data.definition().into_any())
                .cloned()
                .ok_or(FactQueryError::InfrastructureFailure)?;

            let specialization = self.codegen_specialization(data.substitution())?;

            let identity = CodegenImplementationWitness::try_new(
                definition,
                specialization,
            )
            .ok_or(FactQueryError::InfrastructureFailure)?;

            concrete.push((identity, witness));
        }

        Ok(concrete)
    }
}
