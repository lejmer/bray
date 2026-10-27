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
    CallableInstanceData, CheckedConstraintKind, ConstantTermData, ConstantValueData,
    ConstantValueKind, ExactSymbolId, GenericArgument, GenericConstraintsQuery, GenericOwnerId,
    GenericSubstitutionData, GenericSubstitutionId, ImplementationInstanceData,
    ImplementationInstanceId, ImplementationRequirementKey, ImplementationSelection,
    NamedTypeSymbolId, ProofOutcome, StaticInstanceKey, StaticReferenceSelection, StructSymbolId,
    SymbolQueryRequest, TargetSizedIntegerType, TraitCallableMemberSymbolId,
};

use super::super::CodegenPreparationError;
use super::super::Compilation;
use super::super::binder::binding_query_error;
use super::super::implementation::{
    implementation_callable_instance, implementation_fulfillments,
    implementation_instance_requirement,
};
use super::super::substitution::named_type;
use super::realization::{
    codegen_instance_contextual_self, substitute_callable_instance, substitute_contextual_self,
    substitute_contextual_self_in_application, substitute_contextual_self_in_substitution,
};
use super::specialization_identity::encoding::structural_type_identity;
use super::{ProductDataKind, ProductQueryContext, ProductQueryFailure};
use crate::fact::{CancellationToken, FactQueryError};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct ConcreteCodegenInstance {
    key: CodegenInstanceKey,
    callable: Option<CallableInstanceData>,
    anonymous_callable_type: Option<bray_symbols::TypeId>,
    static_initializer: Option<StaticInstanceKey>,
    lifecycle: Option<MirHelperReference>,
    substitution: Option<GenericSubstitutionId>,
    witnesses: Arc<[ImplementationInstanceId]>,
    contextual_self_witness: Option<ImplementationInstanceId>,
}

pub(super) struct ConcreteCodegenReachability {
    execution_requirements:
        BTreeMap<CodegenInstanceKey, &'static [bray_runtime_interface::ExecutionLaneRequirement]>,
    graph: CodegenReachability,
    instances: BTreeMap<CodegenInstanceKey, ConcreteCodegenInstance>,
    demands: Arc<[super::codegen::NativeDemand]>,
    selected_native: BTreeMap<CodegenInstanceKey, super::codegen::reuse::SelectedNativeUnit>,
}

pub(super) enum ConcreteCodegenCallee {
    Instance(ConcreteCodegenInstance),
    Intrinsic(IntrinsicCall),
}

type ConcreteReachabilityParts = (CodegenReachability, BTreeMap<CodegenInstanceKey, ConcreteCodegenInstance>, Arc<[super::codegen::NativeDemand]>, BTreeMap<CodegenInstanceKey, super::codegen::reuse::SelectedNativeUnit>);
type ConcreteStaticSelection = (StaticInstanceKey, CodegenSpecialization, Vec<(CodegenImplementationWitness, ImplementationInstanceId)>);

impl ConcreteCodegenReachability {
    pub(super) fn new(
        execution_requirements: BTreeMap<
            CodegenInstanceKey,
            &'static [bray_runtime_interface::ExecutionLaneRequirement],
        >,
        graph: CodegenReachability,
        instances: BTreeMap<CodegenInstanceKey, ConcreteCodegenInstance>,
        demands: impl Into<Arc<[super::codegen::NativeDemand]>>,
        selected_native: BTreeMap<CodegenInstanceKey, super::codegen::reuse::SelectedNativeUnit>,
    ) -> Self {
        Self {
            execution_requirements,
            graph,
            instances,
            demands: demands.into(),
            selected_native,
        }
    }

    pub(super) fn execution_requirements(
        &self,
        key: &CodegenInstanceKey,
    ) -> Option<&'static [bray_runtime_interface::ExecutionLaneRequirement]> {
        self.execution_requirements.get(key).copied()
    }

    pub(super) const fn graph(&self) -> &CodegenReachability {
        &self.graph
    }

    pub(super) fn instance(&self, key: &CodegenInstanceKey) -> Option<&ConcreteCodegenInstance> {
        self.instances.get(key)
    }

    pub(super) fn selected_native(
        &self,
        key: &CodegenInstanceKey,
    ) -> Option<&super::codegen::reuse::SelectedNativeUnit> {
        self.selected_native.get(key)
    }

    #[cfg(test)]
    pub(super) fn selected_native_units(
        &self,
    ) -> impl Iterator<Item = &super::codegen::reuse::SelectedNativeUnit> {
        self.selected_native.values()
    }

    pub(super) fn demands(&self) -> &[super::codegen::NativeDemand] {
        &self.demands
    }

    pub(super) fn with_demands(
        mut self,
        added: impl IntoIterator<Item = super::codegen::NativeDemand>,
    ) -> Self {
        // Existing Arc-backed demands remain shared while the planning phase extends them.
        let mut demands = self
            .demands
            .iter()
            .cloned()
            .chain(added)
            .collect::<Vec<_>>();

        demands.sort_unstable();
        demands.dedup();

        self.demands = demands.into();

        self
    }

    pub(super) fn into_parts(
        self,
    ) -> ConcreteReachabilityParts {
        (
            self.graph,
            self.instances,
            self.demands,
            self.selected_native,
        )
    }
}

impl ConcreteCodegenInstance {
    pub(super) fn try_callable(
        key: CodegenInstanceKey,
        callable: CallableInstanceData,
        specialization: CodegenSpecialization,
        witnesses: impl IntoIterator<Item = (CodegenImplementationWitness, ImplementationInstanceId)>,
        contextual_self_witness: Option<ImplementationInstanceId>,
    ) -> Option<Self> {
        let mut witnesses: Vec<_> = witnesses.into_iter().collect();

        witnesses.sort_unstable_by(|left, right| left.0.cmp(&right.0));

        let identities: Vec<_> = witnesses
            .iter()
            .map(|(identity, _)| identity.clone())
            .collect();

        let contextual_self_identity = contextual_self_witness.and_then(|selected| {
            witnesses
                .iter()
                .find_map(|(identity, witness)| (*witness == selected).then_some(identity))
        });

        if key.specialization() != &specialization
            || key.witnesses() != identities
            || key.contextual_self_witness() != contextual_self_identity
        {
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
            contextual_self_witness,
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
            contextual_self_witness: None,
        }
    }

    pub(super) fn anonymous_callable(
        owner: &Self,
        reference: &bray_ir::MirAnonymousCallableReference,
        callable_type: bray_symbols::TypeId,
    ) -> Option<Self> {
        let template = match reference {
            // The nested instance owns the source key beyond the parent MIR operation borrow.
            bray_ir::MirAnonymousCallableReference::Bound(key) => MirUnitKey::Bound(key.clone()),
            bray_ir::MirAnonymousCallableReference::Imported(key) => {
                MirUnitKey::ImportedExecutable(*key)
            }
        };

        // The nested instance shares its parent's immutable specialization context.
        Some(Self {
            key: Self::inherited_key(owner, template, owner.key.specialization().clone())?,
            callable: None,
            anonymous_callable_type: Some(callable_type),
            static_initializer: None,
            lifecycle: None,
            substitution: owner.substitution,
            witnesses: Arc::clone(&owner.witnesses),
            contextual_self_witness: owner.contextual_self_witness,
        })
    }

    pub(super) fn inherited_helper(owner: &Self, template: MirUnitKey) -> Option<Self> {
        Some(Self {
            key: Self::inherited_key(owner, template, owner.key.specialization().clone())?,
            callable: None,
            anonymous_callable_type: None,
            static_initializer: None,
            lifecycle: None,
            substitution: owner.substitution,
            witnesses: Arc::clone(&owner.witnesses),
            contextual_self_witness: owner.contextual_self_witness,
        })
    }

    /// Retains selected witnesses for a helper whose reference is already fully instantiated.
    pub(super) fn closed_helper(owner: &Self, template: MirUnitKey) -> Option<Self> {
        let key = Self::inherited_key(owner, template, CodegenSpecialization::NonGeneric)?;

        Some(Self {
            witnesses: Arc::clone(&owner.witnesses),
            contextual_self_witness: owner.contextual_self_witness,
            ..Self::generated(key)
        })
    }

    pub(super) fn static_initializer(
        template: MirUnitKey,
        instance: StaticInstanceKey,
        specialization: CodegenSpecialization,
        witnesses: &[(CodegenImplementationWitness, ImplementationInstanceId)],
        target: MirTargetContract,
    ) -> Self {
        let substitution = instance.substitution();

        Self {
            key: CodegenInstanceKey::new(
                template,
                specialization,
                witnesses.iter().map(|(identity, _)| identity.clone()),
                target,
            ),
            callable: None,
            anonymous_callable_type: None,
            static_initializer: Some(instance),
            lifecycle: None,
            substitution: Some(substitution),
            witnesses: witnesses
                .iter()
                .map(|(_, witness)| *witness)
                .collect::<Vec<_>>()
                .into(),
            contextual_self_witness: None,
        }
    }

    fn specialization_key(
        template: MirUnitKey,
        specialization: CodegenSpecialization,
        witnesses: &[(CodegenImplementationWitness, ImplementationInstanceId)],
        contextual_self_witness: Option<ImplementationInstanceId>,
        target: MirTargetContract,
    ) -> CodegenInstanceKey {
        let key = CodegenInstanceKey::new(
            template,
            specialization,
            witnesses.iter().map(|(identity, _)| identity.clone()),
            target,
        );

        let Some(selected) = contextual_self_witness else {
            return key;
        };

        let identity = witnesses
            .iter()
            .find_map(|(identity, witness)| (*witness == selected).then_some(identity.clone()))
            .expect("selected contextual Self witness belongs to the specialization");

        key.try_with_contextual_self_witness(identity)
            .expect("selected contextual Self identity belongs to the specialization key")
    }

    pub(super) fn specialized_helper(
        template: MirUnitKey,
        substitution: GenericSubstitutionId,
        specialization: CodegenSpecialization,
        witnesses: &[(CodegenImplementationWitness, ImplementationInstanceId)],
        contextual_self_witness: Option<ImplementationInstanceId>,
        target: MirTargetContract,
    ) -> Self {
        Self {
            key: Self::specialization_key(
                template,
                specialization,
                witnesses,
                contextual_self_witness,
                target,
            ),
            callable: None,
            anonymous_callable_type: None,
            static_initializer: None,
            lifecycle: None,
            substitution: Some(substitution),
            witnesses: witnesses.iter().map(|(_, witness)| *witness).collect(),
            contextual_self_witness,
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
            contextual_self_witness: None,
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

    pub(super) fn static_declaration(&self) -> Option<bray_symbols::StaticSymbolId> {
        self.static_initializer
            .as_ref()
            .map(|instance| instance.template().declaration())
    }

    pub(super) fn static_instance(&self) -> Option<&StaticInstanceKey> {
        self.static_initializer.as_ref()
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

    pub(super) const fn contextual_self_witness(&self) -> Option<ImplementationInstanceId> {
        self.contextual_self_witness
    }

    fn inherited_key(
        owner: &Self,
        template: MirUnitKey,
        specialization: CodegenSpecialization,
    ) -> Option<CodegenInstanceKey> {
        let key = CodegenInstanceKey::new(
            template,
            specialization,
            owner.key.witnesses().iter().cloned(),
            owner.key.target().clone(),
        );

        match owner.key.contextual_self_witness().cloned() {
            Some(witness) => key.try_with_contextual_self_witness(witness),
            None => Some(key),
        }
    }
}

impl Compilation {
    pub(super) fn concrete_codegen_static_selection(
        &self,
        owner: &ConcreteCodegenInstance,
        reference: &StaticReferenceSelection,
        cancellation: &CancellationToken,
    ) -> Result<ConcreteStaticSelection, CodegenPreparationError> {
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
                        .map_err(FactQueryError::SemanticValueStore)?,
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
                        let data = values.implementation_instance_data(*witness);

                        let nested = match owner_substitution {
                            Some(owner) => values
                                .substitute_generic_substitution(data.substitution(), owner)
                                .map_err(FactQueryError::SemanticValueStore)?,
                            None => data.substitution(),
                        };

                        let nested = self.realize_codegen_substitution(nested)?;

                        values
                            .intern_implementation_instance(ImplementationInstanceData::new(
                                data.definition(),
                                nested,
                            ))
                            .map_err(|error| FactQueryError::SemanticValueStore(error).into())
                    })
                    .collect::<Result<Vec<_>, CodegenPreparationError>>()?;

                (*template, substitution, witnesses, target.clone())
            }
            StaticReferenceSelection::Closed(instance) => (
                instance.template(),
                instance.substitution(),
                instance.selected_witnesses().to_vec(),
                instance.target().clone(),
            ),
        };

        let substitution = self.realize_codegen_substitution(substitution)?;

        let instance =
            StaticInstanceKey::new(template, substitution, witnesses.iter().copied(), target);

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
        ConcreteCodegenInstance::anonymous_callable(owner, reference, callable_type).ok_or_else(
            || {
                ProductQueryFailure::missing(
                    ProductQueryContext::Instance(owner.key().clone()),
                    ProductDataKind::AnonymousCallableInstance,
                )
                .into()
            },
        )
    }

    pub(super) fn concrete_codegen_bound_helper(
        &self,
        owner: &ConcreteCodegenInstance,
        unit: bray_bound_tree::BoundUnitKey,
    ) -> Result<ConcreteCodegenInstance, CodegenPreparationError> {
        ConcreteCodegenInstance::inherited_helper(owner, MirUnitKey::Bound(unit)).ok_or_else(|| {
            ProductQueryFailure::missing(
                ProductQueryContext::Instance(owner.key().clone()),
                ProductDataKind::BoundHelperInstance,
            )
            .into()
        })
    }

    pub(super) fn concrete_codegen_lifecycle(
        &self,
        reference: MirHelperReference,
        target: &CodegenTarget,
    ) -> Result<ConcreteCodegenInstance, CodegenPreparationError> {
        let role = MirGeneratedLifecycleRole::from_reference(&reference).unwrap_or_else(|| {
            panic!(
                "checked native MissingHelperInstance contract failed: {:?}",
                reference
            );
        });

        let ty = reference.lifecycle_type().unwrap_or_else(|| {
            panic!(
                "checked native MissingHelperInstance contract failed: {:?}",
                reference
            );
        });

        let binding_context = self.binding_context(&self.state.cancellation)?;

        let identity =
            structural_type_identity(self.semantic_value_store()?, &binding_context, ty)?;

        let key = CodegenInstanceKey::new(
            MirUnitKey::GeneratedLifecycle(
                MirGeneratedLifecycleKey::new(role, identity).with_execution(
                    if matches!(reference, MirHelperReference::LifecycleFrame { .. }) {
                        bray_symbols::CallableExecution::Asynchronous
                    } else {
                        bray_symbols::CallableExecution::Synchronous
                    },
                ),
            ),
            CodegenSpecialization::NonGeneric,
            [],
            MirTargetContract::new(
                target.profile().clone(),
                self.selected_target().target().runtime_abi(),
            ),
        );

        let context = ProductQueryContext::Instance(key.clone());

        ConcreteCodegenInstance::try_generated_lifecycle(key, reference).ok_or_else(|| {
            panic!(
                "product Conflict construction failed: context={:?}, data={:?}",
                context,
                ProductDataKind::GeneratedLifecycleInstance
            );
        })
    }

    pub(super) fn concrete_codegen_callable(
        &self,
        callable: CallableInstanceData,
        witnesses: impl IntoIterator<Item = ImplementationInstanceId>,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<ConcreteCodegenInstance, CodegenPreparationError> {
        self.concrete_codegen_callable_with_context(
            callable,
            witnesses,
            None,
            None,
            target,
            cancellation,
        )
    }

    fn concrete_codegen_callable_with_context(
        &self,
        callable: CallableInstanceData,
        witnesses: impl IntoIterator<Item = ImplementationInstanceId>,
        contextual_self_witness: Option<ImplementationInstanceId>,
        default_provider: Option<bray_symbols::DefaultValueProvider>,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<ConcreteCodegenInstance, CodegenPreparationError> {
        cancellation.check()?;

        let substitution = self.realize_codegen_substitution(callable.substitution())?;
        let callable = CallableInstanceData::new(callable.definition(), substitution);

        let template = match default_provider {
            Some(provider) => self.runtime_default_template(
                provider.symbol(),
                &MirHelperReference::DefaultValue(provider),
                cancellation,
            )?,
            None => self.codegen_callable_template(callable.definition(), cancellation)?,
        };

        let specialization = self.codegen_specialization(callable.substitution())?;
        let witnesses = self.concrete_codegen_witnesses(witnesses, cancellation)?;

        let target = MirTargetContract::new(
            target.profile().clone(),
            self.selected_target().target().runtime_abi(),
        );

        if default_provider.is_some() {
            return Ok(ConcreteCodegenInstance::specialized_helper(
                template,
                substitution,
                specialization,
                &witnesses,
                contextual_self_witness,
                target,
            ));
        }

        let key = ConcreteCodegenInstance::specialization_key(
            template,
            specialization.clone(),
            &witnesses,
            contextual_self_witness,
            target,
        );

        let context = ProductQueryContext::Instance(key.clone());

        ConcreteCodegenInstance::try_callable(
            key,
            callable,
            specialization,
            witnesses,
            contextual_self_witness,
        )
        .ok_or_else(|| {
            panic!(
                "product Conflict construction failed: context={:?}, data={:?}",
                context,
                ProductDataKind::ConcreteInstance
            );
        })
    }

    pub(super) fn concrete_codegen_callee(
        &self,
        owner: &ConcreteCodegenInstance,
        demand: &DemandedCallableInstance,
        default_provider: Option<bray_symbols::DefaultValueProvider>,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<ConcreteCodegenCallee, CodegenPreparationError> {
        if demand.trait_dispatch().is_some() {
            return self.concrete_codegen_generic_callee(
                owner,
                demand,
                default_provider,
                target,
                cancellation,
            );
        }

        let values = self.semantic_value_store()?;
        let binding_context = self.binding_context(cancellation)?;
        let contextual_self = codegen_instance_contextual_self(&binding_context, owner)?;
        let reference = demand.reference();
        let callable = reference.instance();

        let (substitution, mut witnesses) = match owner.substitution() {
            Some(owner_substitution) => {
                let substitution = values
                    .substitute_generic_substitution(callable.substitution(), owner_substitution)
                    .map_err(FactQueryError::SemanticValueStore)?;

                let witnesses = self.concrete_codegen_demand_witnesses(
                    demand.witnesses(),
                    owner_substitution,
                    contextual_self,
                )?;

                (substitution, witnesses)
            }
            None => (callable.substitution(), demand.witnesses().to_vec()),
        };

        let substitution =
            substitute_contextual_self_in_substitution(values, substitution, contextual_self)?;

        let callable = CallableInstanceData::new(callable.definition(), substitution);
        let callable = self.normalize_codegen_callable_arguments(callable, owner, cancellation)?;

        witnesses.extend(self.concrete_codegen_forwarded_constraint_witnesses(
            owner,
            &callable,
            &witnesses,
            cancellation,
        )?);

        self.concrete_codegen_callable_with_context(
            callable,
            witnesses,
            None,
            default_provider,
            target,
            cancellation,
        )
        .map(ConcreteCodegenCallee::Instance)
    }

    fn concrete_codegen_generic_callee(
        &self,
        owner: &ConcreteCodegenInstance,
        demand: &DemandedCallableInstance,
        default_provider: Option<bray_symbols::DefaultValueProvider>,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<ConcreteCodegenCallee, CodegenPreparationError> {
        let dispatch = demand
            .trait_dispatch()
            .expect("generic callable demand retains checked dispatch");

        let owner_substitution = owner
            .substitution()
            .expect("generic dispatch retains its owner substitution");

        let values = self.semantic_value_store()?;
        let binding_context = self.binding_context(cancellation)?;
        let contextual_self = codegen_instance_contextual_self(&binding_context, owner)?;

        let requirement =
            super::super::implementation::dispatch_requirement(&binding_context, dispatch)
                .map_err(super::realization::codegen_checker_error)?;

        let subject = requirement.subject();
        let application = requirement.trait_application();

        let subject = values
            .substitute_type(subject, owner_substitution)
            .map_err(FactQueryError::SemanticValueStore)?;

        let application = values
            .substitute_trait_application(application, owner_substitution)
            .map_err(FactQueryError::SemanticValueStore)?;

        let subject = substitute_contextual_self(values, subject, contextual_self)?;

        let application =
            substitute_contextual_self_in_application(values, application, contextual_self)?;

        let requirement = ImplementationRequirementKey::new(subject, application);

        let mut witnesses = self.concrete_codegen_demand_witnesses(
            demand.witnesses(),
            owner_substitution,
            contextual_self,
        )?;

        let witness = self.concrete_codegen_matching_witness(
            witnesses
                .iter()
                .copied()
                .chain(owner.implementation_witnesses().iter().copied()),
            requirement,
            cancellation,
        )?;

        let Some(witness) = witness else {
            if dispatch.trait_default_requirement().is_some() {
                return Err(ProductQueryFailure::missing(
                    ProductQueryContext::ImplementationRequirement(requirement),
                    ProductDataKind::ImplementationWitness,
                )
                .into());
            }

            let intrinsic = demand.intrinsic().ok_or_else(|| {
                ProductQueryFailure::missing(
                    ProductQueryContext::CallSite(demand.site()),
                    ProductDataKind::Intrinsic,
                )
            })?;

            let context = super::super::checker::CompilationCheckerContext::new(binding_context);

            let outcome =
                bray_checker::built_in_trait_constraint_outcome(&context, subject, application);

            if outcome != Some(ProofOutcome::Proven) {
                panic!(
                    "product BuiltInProofMismatch construction failed: requirement={:?}, actual={:?}",
                    requirement, outcome
                );
            }

            let intrinsic = match intrinsic {
                bray_ir::MirCallIntrinsic::Unary(operator) => IntrinsicCall::Unary(operator),
                bray_ir::MirCallIntrinsic::Binary(operator) => {
                    let data = values.type_data(subject);

                    let operand = match data.as_ref() {
                        bray_symbols::TypeData::Borrow { target, .. } => *target,
                        _ => subject,
                    };

                    let complex = self
                        .available_compiler_known_symbols()
                        .type_representation(values, operand)
                        .is_some_and(|role| role.complex_component().is_some());

                    IntrinsicCall::Binary(if complex {
                        operator.for_complex()
                    } else {
                        operator
                    })
                }
                bray_ir::MirCallIntrinsic::Comparison {
                    less,
                    equal,
                    greater,
                } => IntrinsicCall::Comparison {
                    less,
                    equal,
                    greater,
                },
                bray_ir::MirCallIntrinsic::Conversion(target) => {
                    let target = values
                        .substitute_type(target, owner_substitution)
                        .map_err(FactQueryError::SemanticValueStore)?;

                    let target = substitute_contextual_self(values, target, contextual_self)?;

                    let conversion = bray_checker::built_in_conversion_plan_for_context(
                        &context, subject, target,
                    )
                    .ok_or_else(|| {
                        ProductQueryFailure::missing(
                            ProductQueryContext::ImplementationRequirement(requirement),
                            ProductDataKind::ConversionPlan,
                        )
                    })?;

                    IntrinsicCall::Conversion(conversion)
                }
            };

            return Ok(ConcreteCodegenCallee::Intrinsic(intrinsic));
        };

        witnesses.extend(
            self.concrete_codegen_implementation_constraint_witnesses(witness, cancellation)?,
        );

        witnesses.push(witness);

        let declaration = substitute_callable_instance(
            values,
            demand.reference().instance(),
            Some(owner_substitution),
            contextual_self,
        )?;

        let declaration =
            self.normalize_codegen_callable_arguments(declaration, owner, cancellation)?;

        if default_provider.is_some() {
            return self
                .concrete_codegen_callable_with_context(
                    declaration,
                    witnesses,
                    Some(witness),
                    default_provider,
                    target,
                    cancellation,
                )
                .map(ConcreteCodegenCallee::Instance);
        }

        let member_symbol = declaration.definition().symbol();

        let member = TraitCallableMemberSymbolId::try_from_any(member_symbol)
            .expect("checked generic dispatch selects a trait callable member");

        let implementation = values.implementation_instance_data(witness);

        let fulfillments =
            implementation_fulfillments(&binding_context, implementation.definition())?;

        let application = values.trait_application_data(application);

        let callable = implementation_callable_instance(
            &binding_context,
            fulfillments.callables,
            member,
            application.substitution(),
            implementation.substitution(),
        )?
        .ok_or_else(|| {
            ProductQueryFailure::missing(
                ProductQueryContext::Implementation(witness),
                ProductDataKind::CallableFulfillment,
            )
        })?;

        let selected = super::super::implementation::instantiate_implementation_member(
            &binding_context,
            callable.instance(),
            declaration,
        )?;

        let selected = self.normalize_codegen_callable_arguments(selected, owner, cancellation)?;

        self.concrete_codegen_callable_with_context(
            selected,
            witnesses,
            callable.uses_trait_default().then_some(witness),
            None,
            target,
            cancellation,
        )
        .map(ConcreteCodegenCallee::Instance)
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

        let instance = values.implementation_instance_data(implementation);

        let owner = GenericOwnerId::try_new(instance.definition().into_any()).ok_or_else(|| {
            ProductQueryFailure::missing(
                ProductQueryContext::Implementation(implementation),
                ProductDataKind::GenericOwner,
            )
        })?;

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

    pub(super) fn concrete_codegen_requirement_witness(
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
            panic!(
                "product ImplementationSelectionMismatch construction failed: requirement={:?}, actual={:?}",
                requirement,
                selection.value()
            );
        };

        Ok(*witness)
    }

    pub(super) fn concrete_codegen_constraint_requirements(
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
                .map_err(FactQueryError::SemanticValueStore)?;

            let application = values
                .substitute_trait_application(application, substitution)
                .map_err(FactQueryError::SemanticValueStore)?;

            let application_data = values.trait_application_data(application);

            if bray_checker::built_in_trait_constraint_outcome(&context, subject, application)
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
        contextual_self: Option<(bray_symbols::SelfTypeContext, bray_symbols::TypeId)>,
    ) -> Result<Vec<ImplementationInstanceId>, CodegenPreparationError> {
        let values = self.semantic_value_store()?;

        witnesses
            .iter()
            .map(|witness| -> Result<_, CodegenPreparationError> {
                let data = values.implementation_instance_data(*witness);

                let substitution = values
                    .substitute_generic_substitution(data.substitution(), owner_substitution)
                    .map_err(FactQueryError::SemanticValueStore)?;

                let substitution = substitute_contextual_self_in_substitution(
                    values,
                    substitution,
                    contextual_self,
                )?;

                let substitution = self.realize_codegen_substitution(substitution)?;

                values
                    .intern_implementation_instance(ImplementationInstanceData::new(
                        data.definition(),
                        substitution,
                    ))
                    .map_err(|error| FactQueryError::SemanticValueStore(error).into())
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
        let binding_context = self.binding_context(cancellation)?;
        let contextual_self = codegen_instance_contextual_self(&binding_context, owner)?;

        let callable = substitute_callable_instance(
            values,
            *callable,
            owner.substitution(),
            contextual_self,
        )?;

        let callable = self.normalize_codegen_callable_arguments(callable, owner, cancellation)?;

        let witnesses = self.concrete_codegen_forwarded_constraint_witnesses(
            owner,
            &callable,
            &[],
            cancellation,
        )?;

        self.concrete_codegen_callable(callable, witnesses, target, cancellation)
    }

    pub(super) fn realize_codegen_substitution(
        &self,
        substitution: GenericSubstitutionId,
    ) -> Result<GenericSubstitutionId, CodegenPreparationError> {
        let values = self.semantic_value_store()?;

        let data = values.generic_substitution_data(substitution);

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
        .map_err(|cause| ProductQueryFailure::GenericSubstitution {
            substitution: Some(substitution),
            cause,
        })?;

        let realized = values
            .intern_generic_substitution(realized)
            .map_err(FactQueryError::SemanticValueStore)?;

        Ok(realized)
    }

    pub(super) fn realize_codegen_constant_argument(
        &self,
        term: bray_symbols::ConstantTermId,
    ) -> Result<bray_symbols::ConstantTermId, CodegenPreparationError> {
        let values = self.semantic_value_store()?;

        let data = values.constant_term_data(term);

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
            .ok_or_else(|| {
                ProductQueryFailure::missing(
                    ProductQueryContext::CompilerKnownRepresentation(role),
                    ProductDataKind::CompilerKnownRepresentation,
                )
            })?;

        let ty = named_type(values, NamedTypeSymbolId::Struct(definition));

        let value = values
            .intern_constant_value(ConstantValueData::new(
                ty?,
                ConstantValueKind::Integer(value.clone()),
            ))
            .map_err(FactQueryError::SemanticValueStore)?;

        values
            .intern_constant_term(ConstantTermData::Value(value))
            .map_err(|error| FactQueryError::SemanticValueStore(error).into())
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
            let data = values.implementation_instance_data(witness);

            let definition =
                self.portable_codegen_symbol_key(&binding_context, data.definition().into_any())?;

            let specialization = self.codegen_specialization(data.substitution())?;

            let identity =
                CodegenImplementationWitness::try_new(definition.clone(), specialization).unwrap_or_else(|| {
                    let actual = definition.kind();

                    panic!("implementation witness requires an implementation key: key={definition:?}, actual={actual:?}");
                },
                );

            match concrete.insert(identity.clone(), witness) {
                Some(existing) if existing != witness => {
                    panic!(
                        "product ConflictingImplementationWitness construction failed: identity={:?}, existing={:?}, actual={:?}",
                        identity, existing, witness
                    );
                }
                Some(_) | None => {}
            }
        }

        Ok(concrete.into_iter().collect())
    }
}
