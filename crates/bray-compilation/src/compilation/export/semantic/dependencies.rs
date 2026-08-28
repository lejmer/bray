use bray_package_interface::{
    InterfaceCallableInstance, InterfaceCallableInstanceId, InterfaceDependencyContract,
    InterfaceDependencyContractId, InterfaceDependencyGuard, InterfaceDependencyProjection,
    InterfaceDependencyRequirement, InterfaceDependencySubject, InterfaceDependencySubjectRoot,
    InterfaceGenericArgument, InterfaceGenericBinding, InterfaceGenericSubstitution,
    InterfaceGenericSubstitutionId, InterfaceImplementationInstance,
    InterfaceImplementationInstanceId, InterfaceTraitApplication, InterfaceTraitApplicationId,
};
use bray_symbols::{
    CallableInstanceId, DependencyGuard, DependencyProjection, DependencyRequirement,
    DependencySubject, DependencySubjectRoot, GenericArgument, GenericSubstitutionId,
    ImplementationInstanceId, TraitApplicationId,
};

use super::super::PackageInterfaceExportError;

use super::context::{SemanticExporter, export_acyclic_semantic_value};
use super::implementation::{dependency_requirement_kind, incomplete_type};
use super::templates::index;

impl<'a> SemanticExporter<'a> {
    pub(in crate::compilation::export) fn dependency_contract_id(
        &mut self,
        id: bray_symbols::DependencyContractTemplateId,
    ) -> Result<InterfaceDependencyContractId, PackageInterfaceExportError> {
        if let Some(id) = self.dependency_contract_ids.get(&id) {
            return Ok(*id);
        }

        let data = self
            .values
            .dependency_contract_template_data(id)
            .map_err(|_| incomplete_type())?;

        let requirements = data
            .requirements()
            .iter()
            .map(|requirement| self.dependency_requirement(requirement))
            .collect::<Result<Vec<_>, _>>()?;

        let exported = InterfaceDependencyContractId::new(index(self.dependency_contracts.len())?);

        self.dependency_contracts
            .push(InterfaceDependencyContract::new(requirements));

        self.dependency_contract_ids.insert(id, exported);

        Ok(exported)
    }

    pub(super) fn dependency_requirement(
        &mut self,
        requirement: &DependencyRequirement,
    ) -> Result<InterfaceDependencyRequirement, PackageInterfaceExportError> {
        match requirement {
            DependencyRequirement::Direct { subject, kind } => {
                Ok(InterfaceDependencyRequirement::new(
                    self.dependency_subject(subject)?,
                    dependency_requirement_kind(*kind),
                ))
            }
            DependencyRequirement::Guarded(guarded) => {
                let guard = self.dependency_guard(guarded.guard())?;

                let requirements = guarded
                    .requirements()
                    .iter()
                    .map(|requirement| self.dependency_requirement(requirement))
                    .collect::<Result<Vec<_>, _>>()?;

                Ok(InterfaceDependencyRequirement::guarded(guard, requirements))
            }
        }
    }

    pub(super) fn dependency_guard(
        &mut self,
        guard: &DependencyGuard,
    ) -> Result<InterfaceDependencyGuard, PackageInterfaceExportError> {
        match guard {
            DependencyGuard::NullablePresent(subject) => Ok(
                InterfaceDependencyGuard::NullablePresent(self.dependency_subject(subject)?),
            ),
            DependencyGuard::ActiveUnionVariant { subject, variant } => {
                Ok(InterfaceDependencyGuard::ActiveUnionVariant {
                    subject: self.dependency_subject(subject)?,
                    variant: self.symbol_reference((*variant).into())?,
                })
            }
        }
    }

    pub(super) fn dependency_subject(
        &mut self,
        subject: &DependencySubject,
    ) -> Result<InterfaceDependencySubject, PackageInterfaceExportError> {
        let root = match subject.subject_root() {
            DependencySubjectRoot::Receiver => InterfaceDependencySubjectRoot::Receiver,
            DependencySubjectRoot::Parameter(ordinal) => {
                InterfaceDependencySubjectRoot::Parameter(ordinal)
            }
            DependencySubjectRoot::Result => InterfaceDependencySubjectRoot::Result,
            DependencySubjectRoot::ScopedCapability(ordinal) => {
                InterfaceDependencySubjectRoot::ScopedCapability(ordinal)
            }
            DependencySubjectRoot::ImplementationWitness(instance) => {
                InterfaceDependencySubjectRoot::ImplementationWitness(
                    self.implementation_instance_id(instance)?,
                )
            }
            DependencySubjectRoot::ProductStatic(id) => {
                InterfaceDependencySubjectRoot::ProductStatic(self.symbol_reference(id.into())?)
            }
            DependencySubjectRoot::ExactThreadStatic(id) => {
                InterfaceDependencySubjectRoot::ExactThreadStatic(self.symbol_reference(id.into())?)
            }
        };

        let projections = subject
            .projections()
            .iter()
            .map(|projection| match projection {
                DependencyProjection::ProductField(field) => self
                    .symbol_reference((*field).into())
                    .map(InterfaceDependencyProjection::ProductField),
                DependencyProjection::TupleElement(ordinal) => {
                    Ok(InterfaceDependencyProjection::TupleElement(*ordinal))
                }
                DependencyProjection::Element(term) => self
                    .constant_term_id(*term)
                    .map(InterfaceDependencyProjection::Element),
                DependencyProjection::NullableValue => {
                    Ok(InterfaceDependencyProjection::NullableValue)
                }
                DependencyProjection::UnionPayloadField(field) => self
                    .symbol_reference((*field).into())
                    .map(InterfaceDependencyProjection::UnionPayloadField),
                DependencyProjection::OwnedTarget => Ok(InterfaceDependencyProjection::OwnedTarget),
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(InterfaceDependencySubject::new(root, projections))
    }

    pub(in crate::compilation::export) fn substitution_id(
        &mut self,
        id: GenericSubstitutionId,
    ) -> Result<InterfaceGenericSubstitutionId, PackageInterfaceExportError> {
        if let Some(id) = self.substitution_ids.get(&id) {
            return Ok(*id);
        }

        export_acyclic_semantic_value!(
            self,
            active_substitutions,
            id,
            bray_package_interface::InterfaceSemanticTableKind::GenericSubstitution,
            {
                let data = self
                    .values
                    .generic_substitution_data(id)
                    .map_err(|_| incomplete_type())?;

                let bindings = data
                    .bindings()
                    .iter()
                    .map(|binding| {
                        let argument = match binding.argument() {
                            GenericArgument::Type(ty) => {
                                InterfaceGenericArgument::Type(self.type_id(ty)?)
                            }
                            GenericArgument::Constant(term) => {
                                InterfaceGenericArgument::Constant(self.constant_term_id(term)?)
                            }
                        };

                        Ok(InterfaceGenericBinding::new(
                            self.symbol_reference(binding.parameter().into_any())?,
                            argument,
                        ))
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                let exported =
                    InterfaceGenericSubstitutionId::new(index(self.substitutions.len())?);

                self.substitutions.push(InterfaceGenericSubstitution::new(
                    self.symbol_reference(data.owner().symbol())?,
                    bindings,
                ));

                self.substitution_ids.insert(id, exported);

                Ok(exported)
            }
        )
    }

    pub(in crate::compilation::export) fn trait_application_id(
        &mut self,
        id: TraitApplicationId,
    ) -> Result<InterfaceTraitApplicationId, PackageInterfaceExportError> {
        if let Some(id) = self.trait_application_ids.get(&id) {
            return Ok(*id);
        }

        let data = self
            .values
            .trait_application_data(id)
            .map_err(|_| incomplete_type())?;

        let application = InterfaceTraitApplication::new(
            self.symbol_reference(data.definition().into())?,
            self.substitution_id(data.substitution())?,
        );

        let exported = InterfaceTraitApplicationId::new(index(self.trait_applications.len())?);

        self.trait_applications.push(application);
        self.trait_application_ids.insert(id, exported);

        Ok(exported)
    }

    pub(super) fn callable_instance_id(
        &mut self,
        id: CallableInstanceId,
    ) -> Result<InterfaceCallableInstanceId, PackageInterfaceExportError> {
        if let Some(id) = self.callable_instance_ids.get(&id) {
            return Ok(*id);
        }

        let data = self
            .values
            .callable_instance_data(id)
            .map_err(|_| incomplete_type())?;

        let instance = InterfaceCallableInstance::new(
            self.symbol_reference(data.definition().symbol())?,
            self.substitution_id(data.substitution())?,
        );

        let exported = InterfaceCallableInstanceId::new(index(self.callable_instances.len())?);

        self.callable_instances.push(instance);
        self.callable_instance_ids.insert(id, exported);

        Ok(exported)
    }

    pub(in crate::compilation::export) fn implementation_instance_id(
        &mut self,
        id: ImplementationInstanceId,
    ) -> Result<InterfaceImplementationInstanceId, PackageInterfaceExportError> {
        if let Some(id) = self.implementation_instance_ids.get(&id) {
            return Ok(*id);
        }

        let data = self
            .values
            .implementation_instance_data(id)
            .map_err(|_| incomplete_type())?;

        let instance = InterfaceImplementationInstance::new(
            self.symbol_reference(data.definition().into_any())?,
            self.substitution_id(data.substitution())?,
        );

        let exported =
            InterfaceImplementationInstanceId::new(index(self.implementation_instances.len())?);

        self.implementation_instances.push(instance);
        self.implementation_instance_ids.insert(id, exported);

        Ok(exported)
    }
}
