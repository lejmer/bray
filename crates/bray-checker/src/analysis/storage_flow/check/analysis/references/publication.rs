use super::super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::{Origin, Place};
use crate::analysis::storage_flow::model::StorageFlowState;
use bray_symbols::{DependencySubjectRoot, TypeData};

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(in crate::analysis::storage_flow::check::analysis) fn collect_exit_references(
        &mut self,
        state: &StorageFlowState,
    ) {
        let Some(bray_bound_tree::StorageBinding::Identity(result)) = self
            .storage
            .binding(bray_bound_tree::StorageBindingTarget::Result)
        else {
            panic!("planned body has result storage");
        };

        let incoming = state
            .memory
            .exit_references(result, &state.definitely_absent);

        match &mut self.references {
            Some(previous) => previous.merge(&incoming),
            None => self.references = Some(incoming),
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn result_dependencies(
        &mut self,
    ) -> crate::CheckerQueryResult<bray_symbols::DependencyContractTemplateData, C::UpstreamError>
    {
        let carriers = self.trusted.result_carriers(self.request);

        let observed_result = self.trusted.observed_result.flatten();

        let returned_input = (!self.has_normal_effects)
            .then_some(self.trusted.returned_input)
            .flatten()
            .flatten();

        self.diagnostics
            .add_range(std::mem::take(&mut self.trusted).finish(self.execution));

        if let Some(error) = self.query_failure.take() {
            return Err(error);
        }

        let mut relations = Vec::new();
        let summary = std::mem::take(&mut self.results).summary();
        let mut requirements = summary.requirements().to_vec();

        for (slot, target, exact) in self
            .references
            .iter()
            .flat_map(|references| references.targets())
        {
            let Some(root) = self.formal_root(slot.root) else {
                continue;
            };

            let ty = if root == DependencySubjectRoot::Result {
                self.result_type()
            } else {
                self.storage.identity_type(slot.root)
            };

            let Some(ty) = ty else {
                continue;
            };

            if let Some(relation) = self.result_reference(slot, target, ty)? {
                if root == DependencySubjectRoot::Result
                    && relation.kind() == bray_symbols::DependencyReferenceKind::Borrow
                {
                    // Every possible returned reference retains its input, even when no exact origin holds.
                    requirements.push(bray_symbols::DependencyRequirement::direct(
                        relation.entry().clone(),
                        bray_symbols::DependencyRequirementKind::ValueDependencies,
                    ));
                }

                if exact {
                    relations.push(relation);
                }
            }
        }

        Ok(summary
            .with_requirements(requirements)
            .with_borrows(relations)
            .with_normal_completion(self.references.is_some())
            .with_carriers(carriers)
            .with_returned_input(returned_input)
            .with_observed_result(observed_result)
            .with_normal_storage_preservation(!self.has_normal_effects))
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn result_type(
        &self,
    ) -> Option<bray_symbols::TypeId> {
        self.execution
            .semantics
            .types()
            .callable_result_type()
            .or_else(|| match self.request.root() {
                crate::CheckerUnitRoot::Expression(expression) => self
                    .execution
                    .semantics
                    .types()
                    .expression(expression)
                    .map(|ty| ty.ty()),
                _ => None,
            })
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn reference_kind(
        &self,
        slot: &Place,
        mut ty: bray_symbols::TypeId,
    ) -> crate::CheckerQueryResult<Option<bray_symbols::DependencyReferenceKind>, C::UpstreamError>
    {
        let Some(projections) = dependency_projections(&slot.projections) else {
            return Ok(None);
        };

        for projection in projections {
            let projected = crate::storage::projected_value_type(self.request, ty, projection)?;

            let Some(projected) = *projected.value() else {
                return Ok(None);
            };

            ty = projected;
        }

        Ok(crate::representation::reference_kind(self.request, ty))
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn formal_root(
        &self,
        identity: bray_bound_tree::StorageIdentityId,
    ) -> Option<DependencySubjectRoot> {
        let reference = match self.storage.identity(identity)? {
            bray_bound_tree::StorageIdentity::Parameter(parameter) => {
                bray_bound_tree::BoundReferenceTarget::Surface(parameter.into())
            }
            bray_bound_tree::StorageIdentity::ContractParameter(parameter) => {
                bray_bound_tree::BoundReferenceTarget::Local(parameter.into())
            }
            bray_bound_tree::StorageIdentity::AnonymousParameter(parameter) => {
                bray_bound_tree::BoundReferenceTarget::Local(parameter.into())
            }
            bray_bound_tree::StorageIdentity::Receiver(receiver) => {
                bray_bound_tree::BoundReferenceTarget::Surface(receiver.into())
            }
            bray_bound_tree::StorageIdentity::Result(_) => {
                return Some(DependencySubjectRoot::Result);
            }
            _ => return None,
        };

        crate::dependency::parameter_root(self.request, reference)
    }

    fn result_reference(
        &self,
        slot: &Place,
        origin: &Origin,
        ty: bray_symbols::TypeId,
    ) -> crate::CheckerQueryResult<Option<bray_symbols::DependencyBorrowRelation>, C::UpstreamError>
    {
        let (target, stored_reference) = match origin {
            Origin::Entry {
                reference: path, ..
            } => (path, true),
            Origin::Storage(path) => (path, false),
            Origin::Allocation(_) | Origin::Returned { .. } => return Ok(None),
        };

        let Some(root) = self.formal_root(target.root) else {
            return Ok(None);
        };

        if root == DependencySubjectRoot::Result {
            return Ok(None);
        }

        let Some(entry) = dependency_projections(&target.projections) else {
            return Ok(None);
        };

        let Some(exit) = dependency_projections(&slot.projections) else {
            return Ok(None);
        };

        let Some(mut entry_type) = self.storage.identity_type(target.root) else {
            return Ok(None);
        };

        if !stored_reference
            && !matches!(
                self.request
                    .semantic_values()
                    .type_data(entry_type)
                    .as_ref(),
                TypeData::Borrow { .. }
            )
        {
            return Ok(None);
        }

        for projection in &entry {
            let projected =
                crate::storage::projected_value_type(self.request, entry_type, *projection)?;

            let Some(projected) = *projected.value() else {
                return Ok(None);
            };

            entry_type = projected;
        }

        // This target is the field's storage, not a reference value held in that field.
        if !stored_reference
            && !entry.is_empty()
            && crate::representation::reference_kind(self.request, entry_type).is_some()
        {
            return Ok(None);
        }

        let Some(kind) = self.reference_kind(slot, ty)? else {
            return Ok(None);
        };

        let target_projections = match origin {
            Origin::Entry { projections, .. } => dependency_projections(projections),
            _ => Some(Vec::new()),
        };

        let Some(target_projections) = target_projections else {
            return Ok(None);
        };

        Ok(Some(
            bray_symbols::DependencyBorrowRelation::new(
                bray_symbols::DependencySubject::new(root, entry),
                bray_symbols::DependencySubject::new(
                    self.formal_root(slot.root)
                        .expect("reference exit has a formal root"),
                    exit,
                ),
                kind,
            )
            .with_dereferences(match origin {
                Origin::Entry { layer, .. } => {
                    u32::try_from(layer - 1).expect("borrow layers fit the bounded source type")
                }
                _ => 0,
            })
            .with_target_projections(target_projections),
        ))
    }
}

pub(in crate::analysis::storage_flow::check::analysis) fn dependency_projections(
    projections: &[bray_bound_tree::StorageProjection],
) -> Option<Vec<bray_symbols::DependencyProjection>> {
    use bray_bound_tree::StorageProjection;
    use bray_symbols::DependencyProjection;

    projections
        .iter()
        .map(|projection| match projection {
            StorageProjection::ProductField(field) => {
                Some(DependencyProjection::ProductField(*field))
            }
            StorageProjection::TupleElement(index) => {
                Some(DependencyProjection::TupleElement(*index))
            }
            StorageProjection::NullableValue => Some(DependencyProjection::NullableValue),
            StorageProjection::ActiveUnionPayloadField { field, .. } => {
                Some(DependencyProjection::UnionPayloadField(*field))
            }
            StorageProjection::OwnedTarget => Some(DependencyProjection::OwnedTarget),
            StorageProjection::UninitPayload => Some(DependencyProjection::UninitPayload),
            _ => None,
        })
        .collect()
}
