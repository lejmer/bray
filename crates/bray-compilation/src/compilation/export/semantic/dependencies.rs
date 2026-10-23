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
use super::implementation::dependency_requirement_kind;
use super::templates::index;
use crate::compilation::product::StructuralValueEncoder;

impl<'a> SemanticExporter<'a> {
    pub(in crate::compilation::export) fn dependency_contract_id(
        &mut self,
        id: bray_symbols::DependencyContractTemplateId,
    ) -> Result<InterfaceDependencyContractId, PackageInterfaceExportError> {
        if let Some(id) = self.dependency_contract_ids.get(&id) {
            return Ok(*id);
        }

        let data = self.values.dependency_contract_template_data(id);

        let requirements = self.dependency_requirements(data.requirements())?;

        let borrows = self
            .dependency_order(data.borrows(), |encoder, item| {
                encoder.dependency_borrow(item)
            })?
            .into_iter()
            .map(|relation| {
                Ok(bray_symbols::DependencyBorrowRelation::new(
                    relation
                        .entry()
                        .map(|entry| self.dependency_subject(entry))
                        .transpose()?,
                    self.dependency_subject(relation.exit())?,
                    relation.kind(),
                )
                .with_dereferences(relation.dereferences())
                .with_exactness(relation.is_exact())
                .with_target_projections(
                    relation
                        .target_projections()
                        .iter()
                        .map(|projection| self.dependency_projection(projection))
                        .collect::<Result<Vec<_>, _>>()?,
                ))
            })
            .collect::<Result<Vec<_>, PackageInterfaceExportError>>()?;

        let carriers = self
            .dependency_order(data.carriers(), |encoder, item| {
                encoder.dependency_carrier(item)
            })?
            .into_iter()
            .map(|carrier| {
                Ok(bray_symbols::DependencyWitnessCarrier::new(
                    self.constant_term_id(carrier.guarantee())?,
                    carrier
                        .path()
                        .iter()
                        .map(|projection| self.dependency_projection(projection))
                        .collect::<Result<Vec<_>, _>>()?,
                ))
            })
            .collect::<Result<Vec<_>, PackageInterfaceExportError>>()?;

        let effects = self
            .dependency_order(data.effects(), |encoder, item| {
                encoder.dependency_effect(item)
            })?
            .into_iter()
            .map(|effect| {
                use bray_symbols::DependencyStorageEffect;

                Ok(match effect {
                    DependencyStorageEffect::Atomic => DependencyStorageEffect::Atomic,
                    DependencyStorageEffect::Ambient => DependencyStorageEffect::Ambient,
                    DependencyStorageEffect::Unknown => DependencyStorageEffect::Unknown,
                    DependencyStorageEffect::Escape => DependencyStorageEffect::Escape,
                    DependencyStorageEffect::Cleanup(ty) => {
                        DependencyStorageEffect::Cleanup(self.type_id(*ty)?)
                    }
                    DependencyStorageEffect::Direct { subject, kind } => {
                        DependencyStorageEffect::Direct {
                            subject: self.dependency_subject(subject)?,
                            kind: kind.try_map_type(|ty| self.type_id(ty))?,
                        }
                    }
                    DependencyStorageEffect::Indirect {
                        reference,
                        projections,
                        kind,
                    } => DependencyStorageEffect::Indirect {
                        reference: self.dependency_subject(reference)?,
                        projections: projections
                            .iter()
                            .map(|projection| self.dependency_projection(projection))
                            .collect::<Result<_, PackageInterfaceExportError>>()?,
                        kind: kind.try_map_type(|ty| self.type_id(ty))?,
                    },
                })
            })
            .collect::<Result<Vec<_>, PackageInterfaceExportError>>()?;

        let returned_input = data
            .returned_input()
            .map(|input| self.constant_term_id(input))
            .transpose()?;

        let mut contract = InterfaceDependencyContract::new(requirements);

        if let Some(requirements) = data.completion_reports() {
            contract =
                contract.with_completion_reports(self.dependency_requirements(requirements)?);
        }

        if let Some(requirements) = data.abnormal_reports() {
            contract = contract.with_abnormal_reports(self.dependency_requirements(requirements)?);
        }

        let contract = contract
            .with_borrows(borrows)
            .with_carriers(carriers)
            .with_returned_input(returned_input)
            .with_observed_result(
                data.observed_result()
                    .map(|term| self.constant_term_id(term))
                    .transpose()?,
            )
            .with_effects(effects)
            .with_normal_completion(data.normal_completion())
            .with_normal_storage_preservation(data.preserves_storage_on_normal_return());

        let exported = InterfaceDependencyContractId::new(index(self.dependency_contracts.len())?);

        self.dependency_contracts.push(contract);

        self.dependency_contract_ids.insert(id, exported);

        Ok(exported)
    }

    fn dependency_order<'item, T>(
        &self,
        items: &'item [T],
        encode: impl Fn(
            &mut StructuralValueEncoder<'_, '_>,
            &T,
        ) -> Result<(), crate::fact::FactQueryError>,
    ) -> Result<Vec<&'item T>, PackageInterfaceExportError> {
        if items.len() <= 1 {
            return Ok(items.iter().collect());
        }

        let context = self
            .compilation
            .binding_context(&self.compilation.state.cancellation)
            .map_err(super::super::fact_query_export_error)?;

        crate::compilation::product::structural_dependency_order(
            self.values,
            &context,
            items,
            encode,
        )
        .map_err(super::super::fact_query_export_error)
    }

    fn dependency_requirements(
        &mut self,
        requirements: &[DependencyRequirement],
    ) -> Result<Vec<InterfaceDependencyRequirement>, PackageInterfaceExportError> {
        // Assign portable references in semantic order, before interning their children.
        self.dependency_order(requirements, |encoder, item| {
            encoder.dependency_requirement(item)
        })?
        .into_iter()
        .map(|requirement| self.dependency_requirement(requirement))
        .collect()
    }

    fn dependency_call_inputs(
        &mut self,
        inputs: &[bray_symbols::DependencyCallInput],
    ) -> Result<
        Vec<bray_package_interface::InterfaceDependencyCallInput>,
        PackageInterfaceExportError,
    > {
        inputs
            .iter()
            .map(|input| {
                let subject = self.dependency_subject(&DependencySubject::root(input.root()))?;

                let root = subject.subject_root().clone();

                Ok(bray_package_interface::InterfaceDependencyCallInput::new(
                    root,
                    self.dependency_requirements(input.values())?,
                    self.dependency_requirements(input.storage())?,
                ))
            })
            .collect::<Result<Vec<_>, PackageInterfaceExportError>>()
    }

    pub(super) fn dependency_requirement(
        &mut self,
        requirement: &DependencyRequirement,
    ) -> Result<InterfaceDependencyRequirement, PackageInterfaceExportError> {
        match requirement {
            DependencyRequirement::Variable { depth, ordinal } => {
                Ok(InterfaceDependencyRequirement::variable(*depth, *ordinal))
            }
            DependencyRequirement::FixedPoint {
                definitions,
                result,
            } => {
                let definitions = definitions
                    .iter()
                    .map(|definition| self.dependency_requirements(definition))
                    .collect::<Result<Vec<_>, _>>()?;

                let result = self.dependency_requirements(result)?;

                Ok(InterfaceDependencyRequirement::fixed_point(
                    definitions,
                    result,
                ))
            }
            DependencyRequirement::ResultCall {
                output,
                callable,
                requirement,
                inputs,
            } => {
                let callable = self.callable_instance_id(*callable)?;

                let requirement = match requirement {
                    Some(requirement) => Some((
                        self.type_id(requirement.subject())?,
                        self.trait_application_id(requirement.trait_application())?,
                    )),
                    None => None,
                };

                let inputs = self.dependency_call_inputs(inputs)?;

                Ok(InterfaceDependencyRequirement::result_call(
                    callable,
                    requirement,
                    inputs,
                    *output,
                ))
            }
            DependencyRequirement::Direct { subject, kind } => {
                Ok(InterfaceDependencyRequirement::new(
                    self.dependency_subject(subject)?,
                    dependency_requirement_kind(*kind),
                ))
            }
            DependencyRequirement::Guarded(guarded) => {
                let guard = self.dependency_guard(guarded.guard())?;

                let requirements = self.dependency_requirements(guarded.requirements())?;

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
            DependencySubjectRoot::EvaluationStorage => {
                InterfaceDependencySubjectRoot::EvaluationStorage
            }
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
            .map(|projection| self.dependency_projection(projection))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(InterfaceDependencySubject::new(root, projections))
    }

    fn dependency_projection(
        &mut self,
        projection: &DependencyProjection,
    ) -> Result<InterfaceDependencyProjection, PackageInterfaceExportError> {
        match projection {
            DependencyProjection::ProductField(field) => self
                .symbol_reference((*field).into())
                .map(InterfaceDependencyProjection::ProductField),
            DependencyProjection::TupleElement(ordinal) => {
                Ok(InterfaceDependencyProjection::TupleElement(*ordinal))
            }
            DependencyProjection::Element(term) => self
                .constant_term_id(*term)
                .map(InterfaceDependencyProjection::Element),
            DependencyProjection::NullableValue => Ok(InterfaceDependencyProjection::NullableValue),
            DependencyProjection::UnionPayloadField(field) => self
                .symbol_reference((*field).into())
                .map(InterfaceDependencyProjection::UnionPayloadField),
            DependencyProjection::OwnedTarget => Ok(InterfaceDependencyProjection::OwnedTarget),
            DependencyProjection::UninitPayload => Ok(InterfaceDependencyProjection::UninitPayload),
        }
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
                let data = self.values.generic_substitution_data(id);

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

        let data = self.values.trait_application_data(id);

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

        let data = self.values.callable_instance_data(id);

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

        let data = self.values.implementation_instance_data(id);

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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use bray_package_interface::{InterfaceDependencyContract, InterfaceDependencyContractId};
    use bray_symbols::{
        CallableAbi, CallableConstness, CallableDependencyContracts, CallableTrust,
        CallableTypeData, ConstantTermData, DependencyBorrowRelation,
        DependencyContractTemplateData, DependencyGuard, DependencyProjection,
        DependencyReferenceKind, DependencyRequirement, DependencyRequirementKind,
        DependencyStorageEffect, DependencySubject, DependencySubjectRoot,
        DependencyWitnessCarrier, SymbolOrdinal, TypeData,
    };

    use super::super::context::SemanticExporter;

    #[test]
    fn dependency_export_order_ignores_source_value_allocation_order() {
        for category in ["requirements", "borrows", "carriers", "effects"] {
            let mut exported = Vec::new();

            for ordinals in [[1, 2], [2, 1]] {
                let compilation = crate::test_support::compilation("module app;");
                let values = compilation.semantic_value_store().unwrap();
                let graph = compilation.symbol_graph().unwrap();

                let bundle =
                    bray_package_interface::test_support::package_interface_export_bundle();

                let keys = BTreeMap::new();

                let mut export =
                    SemanticExporter::new(&compilation, graph, bundle.surface(), &keys, values);

                let unit = values.intern_type(TypeData::Tuple([].into())).unwrap();
                let mut requirements = Vec::new();
                let mut borrows = Vec::new();
                let mut carriers = Vec::new();
                let mut effects = Vec::new();

                for ordinal in ordinals {
                    let index = values
                        .intern_constant_term(ConstantTermData::CallableArgument(
                            SymbolOrdinal::new(ordinal),
                        ))
                        .unwrap();

                    let subject = DependencySubject::new(
                        DependencySubjectRoot::Parameter(SymbolOrdinal::new(0)),
                        [DependencyProjection::Element(index)],
                    );

                    let ty = values
                        .intern_type(TypeData::Tuple(
                            vec![unit; usize::try_from(ordinal).unwrap()].into(),
                        ))
                        .unwrap();

                    requirements.push(DependencyRequirement::direct(
                        subject.clone(),
                        DependencyRequirementKind::StorageAlive,
                    ));

                    borrows.push(DependencyBorrowRelation::new(
                        Some(subject),
                        DependencySubject::root(DependencySubjectRoot::Result),
                        DependencyReferenceKind::Borrow,
                    ));

                    carriers.push(DependencyWitnessCarrier::new(index, []));
                    effects.push(DependencyStorageEffect::Cleanup(ty));
                }

                let data = match category {
                    "requirements" => {
                        let guarded = DependencyRequirement::guarded(
                            DependencyGuard::NullablePresent(DependencySubject::root(
                                DependencySubjectRoot::Parameter(SymbolOrdinal::new(3)),
                            )),
                            requirements.iter().cloned(),
                        );

                        let equations = DependencyRequirement::fixed_point(
                            [requirements.iter().cloned()],
                            requirements.iter().cloned(),
                        );

                        DependencyContractTemplateData::new(requirements)
                            .with_completion_reports([guarded])
                            .with_abnormal_reports([equations])
                    }
                    "borrows" => DependencyContractTemplateData::new([]).with_borrows(borrows),
                    "carriers" => DependencyContractTemplateData::new([]).with_carriers(carriers),
                    "effects" => DependencyContractTemplateData::new([]).with_effects(effects),
                    _ => unreachable!(),
                };

                let contract = values.intern_dependency_contract_template(data).unwrap();

                export.dependency_contract_id(contract).unwrap();

                exported.push((
                    export.types,
                    export.constant_terms,
                    export.dependency_contracts,
                ));
            }

            assert_eq!(exported[0], exported[1], "{category}");
        }
    }

    #[test]
    fn nested_callable_contracts_receive_distinct_export_indices() {
        let compilation = crate::test_support::compilation("module app;");
        let values = compilation.semantic_value_store().unwrap();
        let graph = compilation.symbol_graph().unwrap();
        let bundle = bray_package_interface::test_support::package_interface_export_bundle();
        let keys = BTreeMap::new();

        let mut export =
            SemanticExporter::new(&compilation, graph, bundle.surface(), &keys, values);

        let nested = values.empty_dependency_contract_template().unwrap();
        let unit = values.intern_type(TypeData::Tuple([].into())).unwrap();

        let callable = values
            .intern_type(TypeData::Callable(CallableTypeData::new(
                [],
                unit,
                CallableConstness::Runtime,
                CallableTrust::Safe,
                CallableAbi::Bray,
                CallableDependencyContracts::synchronous(nested),
            )))
            .unwrap();

        let outer = values
            .intern_dependency_contract_template(
                DependencyContractTemplateData::new([])
                    .with_effects([DependencyStorageEffect::Cleanup(callable)])
                    .with_normal_completion(false),
            )
            .unwrap();

        let outer_id = export.dependency_contract_id(outer).unwrap();
        let nested_id = export.dependency_contract_id(nested).unwrap();
        let callable_id = export.type_id(callable).unwrap();

        assert_eq!(
            (nested_id, outer_id, export.dependency_contracts),
            (
                InterfaceDependencyContractId::new(0),
                InterfaceDependencyContractId::new(1),
                vec![
                    InterfaceDependencyContract::new([]),
                    InterfaceDependencyContract::new([])
                        .with_effects([DependencyStorageEffect::Cleanup(callable_id)])
                        .with_normal_completion(false),
                ],
            )
        );
    }
}
