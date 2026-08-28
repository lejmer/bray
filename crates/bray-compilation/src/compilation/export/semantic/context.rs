use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{CheckedTemplateConstantUsage, CheckedTemplateKind, CheckedTemplateNodeId};
use bray_checker::resolve_type_expression_template;
use bray_package_interface::{
    InterfaceCallableContract, InterfaceCallableContractClause, InterfaceCallableInstance,
    InterfaceCallableInstanceId, InterfaceCallableParameter, InterfaceCallablePhaseBehavior,
    InterfaceCallableReceiver, InterfaceCallableSignature, InterfaceCheckedTemplate,
    InterfaceCheckedTemplateBehavior, InterfaceCheckedTemplateExecution,
    InterfaceCheckedTemplateNode, InterfaceCheckedTemplateOperation, InterfaceConstantTerm,
    InterfaceConstantTermId, InterfaceConstantValue, InterfaceConstantValueId, InterfaceConstraint,
    InterfaceDependencyContract, InterfaceDependencyContractId, InterfaceGenericDeclaration,
    InterfaceGenericSubstitution, InterfaceGenericSubstitutionId, InterfaceImplementationInstance,
    InterfaceImplementationInstanceId, InterfacePredicateSummary, InterfaceTraitApplication,
    InterfaceTraitApplicationId, InterfaceTrustedCapabilityRequirement, InterfaceType,
    InterfaceTypeId, PackageInterfaceSurface,
};
use bray_symbols::{
    AnySymbolId, CallableContractClauseValue, CallableInstanceId, CallablePhaseBehavior,
    CheckedConstraintKind, ConstantTermId, ConstantValueId, CurrentRunCancellation,
    ExternalSymbolKey, GenericSubstitutionId, ImplementationInstanceId, SemanticValueStore,
    TraitApplicationId, TypeData, TypeExpressionTemplate, TypeId,
};

use super::super::PackageInterfaceExportError;
use crate::compilation::Compilation;

use super::implementation::incomplete_type;
use super::templates::{incomplete, index};

macro_rules! export_acyclic_semantic_value {
    ($exporter:ident, $active:ident, $id:ident, $table:expr, $body:block) => {{
        if !$exporter.$active.insert($id) {
            Err($exporter
                .cyclic_semantic_value_error($table, $id.slot())
                .map_err(PackageInterfaceExportError::FragmentCoordination)?)
        } else {
            let result = (|| $body)();

            let removed = $exporter.$active.remove(&$id);
            debug_assert!(removed, "active semantic value must be released");

            result
        }
    }};
}

pub(super) use export_acyclic_semantic_value;

#[derive(Clone, Copy)]
pub(super) struct CheckedConstantExpression {
    pub(super) term: ConstantTermId,
    pub(super) ty: TypeId,
}

pub(in crate::compilation::export) struct SemanticExporter<'a> {
    pub(super) compilation: &'a Compilation,
    pub(super) graph: &'a bray_symbols::SymbolGraph,
    pub(super) surface: &'a PackageInterfaceSurface,
    pub(super) keys: &'a BTreeMap<AnySymbolId, ExternalSymbolKey>,
    pub(super) values: &'a SemanticValueStore,
    declaration: Option<AnySymbolId>,
    pub(super) type_ids: BTreeMap<TypeId, InterfaceTypeId>,
    pub(super) substitution_ids: BTreeMap<GenericSubstitutionId, InterfaceGenericSubstitutionId>,
    pub(super) trait_application_ids: BTreeMap<TraitApplicationId, InterfaceTraitApplicationId>,
    pub(super) callable_instance_ids: BTreeMap<CallableInstanceId, InterfaceCallableInstanceId>,
    pub(super) implementation_instance_ids:
        BTreeMap<ImplementationInstanceId, InterfaceImplementationInstanceId>,
    pub(super) dependency_contract_ids:
        BTreeMap<bray_symbols::DependencyContractTemplateId, InterfaceDependencyContractId>,
    pub(super) constant_term_ids: BTreeMap<ConstantTermId, InterfaceConstantTermId>,
    pub(super) constant_value_ids: BTreeMap<ConstantValueId, InterfaceConstantValueId>,
    pub(super) active_types: BTreeSet<TypeId>,
    pub(super) active_substitutions: BTreeSet<GenericSubstitutionId>,
    pub(super) active_constant_terms: BTreeSet<ConstantTermId>,
    pub(super) active_constant_values: BTreeSet<ConstantValueId>,
    pub(super) types: Vec<InterfaceType>,
    pub(super) substitutions: Vec<InterfaceGenericSubstitution>,
    pub(super) trait_applications: Vec<InterfaceTraitApplication>,
    pub(super) callable_instances: Vec<InterfaceCallableInstance>,
    pub(super) implementation_instances: Vec<InterfaceImplementationInstance>,
    pub(super) dependency_contracts: Vec<InterfaceDependencyContract>,
    pub(super) constant_terms: Vec<InterfaceConstantTerm>,
    pub(super) constant_values: Vec<InterfaceConstantValue>,
}

impl<'a> SemanticExporter<'a> {
    pub(super) fn new(
        compilation: &'a Compilation,
        graph: &'a bray_symbols::SymbolGraph,
        surface: &'a PackageInterfaceSurface,
        keys: &'a BTreeMap<AnySymbolId, ExternalSymbolKey>,
        values: &'a SemanticValueStore,
    ) -> Self {
        Self {
            compilation,
            graph,
            surface,
            keys,
            values,
            declaration: None,
            type_ids: BTreeMap::new(),
            substitution_ids: BTreeMap::new(),
            trait_application_ids: BTreeMap::new(),
            callable_instance_ids: BTreeMap::new(),
            implementation_instance_ids: BTreeMap::new(),
            dependency_contract_ids: BTreeMap::new(),
            constant_term_ids: BTreeMap::new(),
            constant_value_ids: BTreeMap::new(),
            active_types: BTreeSet::new(),
            active_substitutions: BTreeSet::new(),
            active_constant_terms: BTreeSet::new(),
            active_constant_values: BTreeSet::new(),
            types: Vec::new(),
            substitutions: Vec::new(),
            trait_applications: Vec::new(),
            callable_instances: Vec::new(),
            implementation_instances: Vec::new(),
            dependency_contracts: Vec::new(),
            constant_terms: Vec::new(),
            constant_values: Vec::new(),
        }
    }

    pub(super) fn with_declaration(mut self, declaration: AnySymbolId) -> Self {
        self.declaration = Some(declaration);

        self
    }

    pub(super) fn cyclic_semantic_value_error(
        &self,
        table: bray_package_interface::InterfaceSemanticTableKind,
        reference: u32,
    ) -> Result<PackageInterfaceExportError, crate::fact::FactQueryError> {
        let declaration = self
            .declaration
            .map(|declaration| {
                crate::compilation::diagnostics::symbol_diagnostic_identity(
                    self.graph,
                    None,
                    declaration,
                )
            })
            .transpose()?;

        Ok(PackageInterfaceExportError::CyclicSemanticFragment {
            declaration,
            table,
            reference,
        })
    }

    pub(super) fn callable_signature(
        &mut self,
        owner: AnySymbolId,
        template: &bray_symbols::CallableSignatureTemplate,
    ) -> Result<InterfaceCallableSignature, PackageInterfaceExportError> {
        let callable_type = self.resolve_type_template(owner, template.callable_type())?;
        let result = self.resolve_type_template(owner, template.result())?;

        let receiver = template
            .receiver()
            .map(|receiver| {
                Ok(InterfaceCallableReceiver::new(
                    self.symbol_reference(receiver.parameter().into())?,
                    self.type_id(receiver.ty())?,
                    receiver.mode(),
                ))
            })
            .transpose()?;

        let parameters = template
            .parameters()
            .iter()
            .copied()
            .map(|parameter| self.symbol_reference(parameter.into()))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(InterfaceCallableSignature::new(
            self.symbol_reference(owner)?,
            self.type_id(callable_type)?,
            receiver,
            parameters,
            self.type_id(result)?,
        )
        .with_body(template.has_body()))
    }

    pub(super) fn generic_declaration(
        &self,
        owner: AnySymbolId,
        template: &bray_symbols::GenericDeclarationTemplate,
    ) -> Result<InterfaceGenericDeclaration, PackageInterfaceExportError> {
        let parameters = template
            .parameters()
            .iter()
            .copied()
            .map(|parameter| self.symbol_reference(parameter.into_any()))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(InterfaceGenericDeclaration::new(
            self.symbol_reference(owner)?,
            parameters,
        ))
    }

    pub(in crate::compilation::export) fn resolve_type_template(
        &self,
        owner: AnySymbolId,
        template: &TypeExpressionTemplate,
    ) -> Result<TypeId, PackageInterfaceExportError> {
        let constants = self
            .compilation
            .checked_constant_terms(template)
            .map_err(|_| incomplete(owner))?;

        if constants.diagnostics().has_errors() {
            return Err(incomplete(owner));
        }

        resolve_type_expression_template(self.values, template, constants.value())
            .map_err(|_| incomplete(owner))?
            .ok_or_else(|| incomplete(owner))
    }

    pub(super) fn generic_constraints(
        &mut self,
        owner: AnySymbolId,
        constraints: &bray_symbols::GenericConstraintSet,
    ) -> Result<Vec<InterfaceConstraint>, PackageInterfaceExportError> {
        let owner = self.symbol_reference(owner)?;

        constraints
            .constraints()
            .iter()
            .copied()
            .map(|constraint| match constraint.kind() {
                CheckedConstraintKind::Predicate(predicate) => Ok(InterfaceConstraint::new(
                    owner.clone(),
                    constraint.ordinal(),
                    self.predicate_summary(predicate)?,
                )),
                CheckedConstraintKind::TraitSatisfaction {
                    subject,
                    application,
                } => Ok(InterfaceConstraint::trait_satisfaction(
                    owner.clone(),
                    constraint.ordinal(),
                    self.type_id(subject)?,
                    self.trait_application_id(application)?,
                )),
                CheckedConstraintKind::TypeEquality { left, right } => {
                    Ok(InterfaceConstraint::type_equality(
                        owner.clone(),
                        constraint.ordinal(),
                        self.type_id(left)?,
                        self.type_id(right)?,
                    ))
                }
            })
            .collect()
    }

    pub(super) fn callable_contract(
        &mut self,
        owner: AnySymbolId,
        contract: &bray_symbols::CallableContractSet,
    ) -> Result<InterfaceCallableContract, PackageInterfaceExportError> {
        let clauses = contract
            .invocation_preconditions()
            .iter()
            .chain(contract.static_constraints())
            .chain(contract.normal_completion_postconditions())
            .copied()
            .map(|clause| match clause.value() {
                CallableContractClauseValue::Predicate(predicate) => {
                    Ok(InterfaceCallableContractClause::new(
                        clause.ordinal(),
                        clause.kind(),
                        self.predicate_summary(predicate)?,
                    ))
                }
                CallableContractClauseValue::TraitSatisfaction {
                    subject,
                    application,
                } => Ok(InterfaceCallableContractClause::trait_satisfaction(
                    clause.ordinal(),
                    self.type_id(subject)?,
                    self.trait_application_id(application)?,
                )),
            })
            .collect::<Result<Vec<_>, _>>()?;

        let invocation = self.phase_behavior(contract.invocation_behavior())?;

        let deferred = contract
            .deferred_execution_behavior()
            .map(|behavior| self.phase_behavior(behavior))
            .transpose()?;

        Ok(InterfaceCallableContract::new(
            self.symbol_reference(owner)?,
            clauses,
            invocation,
            deferred,
        ))
    }

    pub(super) fn predicate_summary(
        &mut self,
        predicate: bray_symbols::PredicateSemanticSummary,
    ) -> Result<InterfacePredicateSummary, PackageInterfaceExportError> {
        Ok(InterfacePredicateSummary::new(
            self.dependency_contract_id(predicate.dependency_contract())?,
        ))
    }

    pub(super) fn checked_constant_template(
        &mut self,
        kind: CheckedTemplateKind,
        expression: CheckedConstantExpression,
        dependency_contract: bray_symbols::DependencyContractTemplateId,
    ) -> Result<InterfaceCheckedTemplate, PackageInterfaceExportError> {
        self.checked_constant_template_with_usage(
            kind,
            expression,
            dependency_contract,
            CheckedTemplateConstantUsage::default(),
        )
    }

    pub(super) fn checked_constant_template_with_usage(
        &mut self,
        kind: CheckedTemplateKind,
        expression: CheckedConstantExpression,
        dependency_contract: bray_symbols::DependencyContractTemplateId,
        usage: CheckedTemplateConstantUsage,
    ) -> Result<InterfaceCheckedTemplate, PackageInterfaceExportError> {
        let result = CheckedTemplateNodeId::new(0);

        let node = InterfaceCheckedTemplateNode::new(
            InterfaceCheckedTemplateOperation::Constant {
                term: self.constant_term_id(expression.term)?,
                usage,
            },
            self.type_id(expression.ty)?,
        );

        let behavior = InterfaceCheckedTemplateBehavior::new(
            [],
            [],
            [],
            InterfaceCheckedTemplateExecution::new([], CurrentRunCancellation::NotEntered),
            [],
            self.dependency_contract_id(dependency_contract)?,
            [],
        );

        Ok(InterfaceCheckedTemplate::new(
            kind,
            [],
            [node],
            [],
            result,
            behavior,
        ))
    }

    pub(in crate::compilation::export) fn type_id(
        &mut self,
        id: TypeId,
    ) -> Result<InterfaceTypeId, PackageInterfaceExportError> {
        if let Some(id) = self.type_ids.get(&id) {
            return Ok(*id);
        }

        export_acyclic_semantic_value!(
            self,
            active_types,
            id,
            bray_package_interface::InterfaceSemanticTableKind::Type,
            {
                let data = self.values.type_data(id).map_err(|_| incomplete_type())?;

                let ty = match data.as_ref() {
                    TypeData::Error => return Err(incomplete_type()),
                    TypeData::Named {
                        definition,
                        substitution,
                    } => InterfaceType::Named {
                        definition: self.symbol_reference(definition.into_any())?,
                        substitution: self.substitution_id(*substitution)?,
                    },
                    TypeData::TypeParameter(parameter) => {
                        InterfaceType::TypeParameter(self.symbol_reference((*parameter).into())?)
                    }
                    TypeData::ContextualSelf(context) => {
                        InterfaceType::ContextualSelf(self.symbol_reference(context.symbol())?)
                    }
                    TypeData::TypeValuedMemberProjection {
                        subject,
                        application,
                        member,
                    } => InterfaceType::TypeValuedMemberProjection {
                        subject: self.type_id(*subject)?,
                        application: self.trait_application_id(*application)?,
                        member: self.symbol_reference((*member).into())?,
                    },
                    TypeData::Tuple(elements) => InterfaceType::Tuple(
                        elements
                            .iter()
                            .copied()
                            .map(|element| self.type_id(element))
                            .collect::<Result<Vec<_>, _>>()?
                            .into(),
                    ),
                    TypeData::Array { element, length } => InterfaceType::Array {
                        element: self.type_id(*element)?,
                        length: self.constant_term_id(*length)?,
                    },
                    TypeData::FlexibleArray(element) => {
                        InterfaceType::FlexibleArray(self.type_id(*element)?)
                    }
                    TypeData::Slice(element) => InterfaceType::Slice(self.type_id(*element)?),
                    TypeData::Generator(element) => {
                        InterfaceType::Generator(self.type_id(*element)?)
                    }
                    TypeData::Nullable(target) => InterfaceType::Nullable(self.type_id(*target)?),
                    TypeData::Borrow { kind, target } => InterfaceType::Borrow {
                        kind: *kind,
                        target: self.type_id(*target)?,
                    },
                    TypeData::TraitView(application) => {
                        InterfaceType::TraitView(self.trait_application_id(*application)?)
                    }
                    TypeData::OwnedIndirection { storage, target } => {
                        InterfaceType::OwnedIndirection {
                            storage: self.type_id(*storage)?,
                            target: self.type_id(*target)?,
                        }
                    }
                    TypeData::Callable(callable) => self.callable_type(callable)?,
                };

                let exported = InterfaceTypeId::new(index(self.types.len())?);
                self.types.push(ty);
                self.type_ids.insert(id, exported);

                Ok(exported)
            }
        )
    }

    pub(super) fn callable_type(
        &mut self,
        callable: &bray_symbols::CallableTypeData,
    ) -> Result<InterfaceType, PackageInterfaceExportError> {
        let parameters = callable
            .parameters()
            .iter()
            .map(|parameter| {
                Ok(InterfaceCallableParameter::new(
                    parameter.name().as_str(),
                    parameter.position(),
                    parameter.mode(),
                    self.type_id(parameter.ty())?,
                ))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let invocation_behavior = self.phase_behavior(callable.phase_behaviors().invocation())?;

        let deferred_execution_behavior = callable
            .phase_behaviors()
            .deferred_execution()
            .map(|behavior| self.phase_behavior(behavior))
            .transpose()?;

        Ok(InterfaceType::Callable {
            parameters: parameters.into(),
            variadic: callable.is_variadic(),
            result: self.type_id(callable.result())?,
            constness: callable.constness(),
            trust: callable.trust(),
            abi: callable.abi(),
            invocation_behavior,
            deferred_execution_behavior,
        })
    }

    pub(super) fn phase_behavior(
        &mut self,
        behavior: &CallablePhaseBehavior,
    ) -> Result<InterfaceCallablePhaseBehavior, PackageInterfaceExportError> {
        let effects = behavior
            .effects()
            .iter()
            .map(|requirement| self.symbol_reference(requirement.declaration()))
            .collect::<Result<Vec<_>, _>>()?;

        let capabilities = behavior
            .capabilities()
            .iter()
            .map(|requirement| self.symbol_reference(requirement.declaration()))
            .collect::<Result<Vec<_>, _>>()?;

        let trusted_capabilities = behavior
            .trusted_capabilities()
            .iter()
            .map(|requirement| {
                Ok(InterfaceTrustedCapabilityRequirement::new(
                    requirement.ordinal(),
                    self.symbol_reference(requirement.capability().into())?,
                ))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let execution_requirements = behavior
            .execution_requirements()
            .iter()
            .map(|requirement| self.symbol_reference(requirement.declaration()))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(InterfaceCallablePhaseBehavior::new(
            effects,
            capabilities,
            trusted_capabilities,
            execution_requirements,
            behavior.lifecycle_obligations().iter().copied(),
            self.dependency_contract_id(behavior.dependency_contract())?,
            behavior.current_run_cancellation(),
        ))
    }
}
