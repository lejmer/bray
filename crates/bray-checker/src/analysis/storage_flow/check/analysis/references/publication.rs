use super::super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::{Origin, Place, ReferenceBounds};
use crate::analysis::storage_flow::model::StorageFlowState;
use crate::storage::{dependency_projection, dependency_projections};
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

        let owned_transfers = self.trusted.owned_transfers.take().unwrap_or_default();
        let owned_transfers = self.owned_transfers(owned_transfers)?;

        self.diagnostics
            .add_range(std::mem::take(&mut self.trusted).finish(self.execution));

        if let Some(error) = self.query_failure.take() {
            return Err(error);
        }

        let mut relations = Vec::new();
        let mut unbounded = std::collections::BTreeSet::new();
        let summary = std::mem::take(&mut self.results).summary();

        for (slot, target, exact, bounds) in self
            .references
            .iter()
            .flat_map(|references| references.targets())
        {
            if self.formal_root(slot.root).is_none() {
                continue;
            }

            let Some(ty) = self.storage.identity_type(slot.root) else {
                continue;
            };

            if let Some(relation) = self.result_reference(slot, target, ty, exact)? {
                let bounded = match bounds {
                    ReferenceBounds::Known => true,
                    ReferenceBounds::Displaced => {
                        relation.kind() == bray_symbols::DependencyReferenceKind::Address
                    }
                    ReferenceBounds::Unknown => false,
                };

                if bounded {
                    let exact = exact && relation.entry().is_some();

                    relations.push((slot.clone(), relation.with_exactness(exact)));
                } else {
                    unbounded.insert(slot.clone());
                }
            } else {
                unbounded.insert(slot.clone());
            }
        }

        let relations = relations
            .into_iter()
            .filter_map(|(slot, relation)| (!unbounded.contains(&slot)).then_some(relation))
            .collect::<Vec<_>>();

        let mut requirements = summary.requirements().to_vec();

        // Complete borrow origins retain references installed by mutation. Raw-address
        // bounds carry no lifetime requirement, and incomplete origins add no certificate.
        requirements.extend(
            relations
                .iter()
                .filter(|relation| {
                    relation.kind() == bray_symbols::DependencyReferenceKind::Borrow
                        && relation.exit().subject_root() == DependencySubjectRoot::Result
                })
                .map(|relation| {
                    bray_symbols::DependencyRequirement::direct(
                        relation
                            .entry()
                            .expect("complete borrow origin has its input")
                            .clone(),
                        bray_symbols::DependencyRequirementKind::ValueDependencies,
                    )
                }),
        );

        Ok(summary
            .with_requirements(requirements)
            .with_borrows(relations)
            .with_normal_completion(self.references.is_some())
            .with_carriers(carriers)
            .with_owned_transfers(owned_transfers)
            .with_observed_result(observed_result)
            .with_normal_storage_preservation(!self.has_normal_effects))
    }

    fn owned_transfers(
        &mut self,
        candidates: std::collections::BTreeSet<bray_symbols::DependencyOwnedTransfer>,
    ) -> crate::CheckerQueryResult<Vec<bray_symbols::DependencyOwnedTransfer>, C::UpstreamError> {
        use bray_symbols::{ConstantProjectionKind, ConstantTermData, DependencyProjection};

        if candidates.is_empty() {
            return Ok(Vec::new());
        }

        let inputs = self.request.trusted_contracts().expect("owned transfer inference retains formal inputs");

        let mut result = Vec::new();
        let mut copyable = std::collections::BTreeMap::new();

        for transfer in candidates {
            let mut term = transfer.input();
            let mut projections = Vec::new();

            let ordinal = loop {
                match self.request.semantic_values().constant_term_data(term).as_ref() {
                    ConstantTermData::CallableArgument(ordinal) => break ordinal.raw(),
                    ConstantTermData::Projection(projection) => {
                        projections.push(projection.kind());
                        term = projection.subject();
                    }
                    _ => unreachable!("inferred ownership source contains only input projections"),
                }
            };

            let input = inputs.inputs[usize::try_from(ordinal).expect("formal input ordinal fits the target")];

            let target = bray_bound_tree::StorageBindingTarget::from_reference_target(input)
                .expect("formal ownership input has a storage binding role");

            let mut ty = match self.storage.binding(target)
                .expect("formal ownership input retains its checked storage") {
                bray_bound_tree::StorageBinding::Identity(identity) => self.storage.storage_type(identity)
                    .expect("formal ownership input retains its checked type"),
                bray_bound_tree::StorageBinding::Access(access) => self.storage.access(access)
                    .expect("formal ownership input retains its checked access").reached_type(),
            };

            for projection in projections.into_iter().rev() {
                let projection = match projection {
                    ConstantProjectionKind::ProductField(field) => DependencyProjection::ProductField(field),
                    ConstantProjectionKind::UnionPayloadField(field) => DependencyProjection::UnionPayloadField(field),
                    ConstantProjectionKind::TupleElement(ordinal) => DependencyProjection::TupleElement(ordinal),
                    ConstantProjectionKind::NullableValue => DependencyProjection::NullableValue,
                    ConstantProjectionKind::ArrayElementOrdinal(_) => {
                        let data = self.request.semantic_values().type_data(ty);

                        let TypeData::Array { element, .. } = data.as_ref() else {
                            panic!("checked owned array projection retains its array type: {ty:?}");
                        };

                        ty = *element;
                        continue;
                    }
                    _ => unreachable!("inferred ownership source uses an owning projection"),
                };

                let projected = crate::storage::projected_value_type(self.request, ty, projection)?;

                let (projected, diagnostics) = projected.into_parts();

                self.diagnostics.add_range(diagnostics);
                ty = projected.expect("checked owned input retains its projected type");
            }

            let copies = if let Some(copies) = copyable.get(&ty) { *copies } else {
                let resolved = match crate::storage::type_is_copyable(self.request, ty) {
                    crate::CheckerOutcome::Complete(result) => result,
                    crate::CheckerOutcome::Cancelled => return Err(crate::CheckerQueryError::Cancelled),
                    crate::CheckerOutcome::UpstreamFailure(error) => return Err(crate::CheckerQueryError::Upstream(error)),
                };

                let (copies, diagnostics) = resolved.into_parts();

                self.diagnostics.add_range(diagnostics);
                copyable.insert(ty, copies);

                copies
            };

            if !copies { result.push(transfer); }
        }

        Ok(result)
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
        exact: bool,
    ) -> crate::CheckerQueryResult<Option<bray_symbols::DependencyBorrowRelation>, C::UpstreamError>
    {
        let (target, stored_reference) = match origin {
            Origin::Entry {
                reference: path, ..
            } => (path, true),
            Origin::Storage(path) => (path, false),
            Origin::Allocation(_) | Origin::Returned { .. } => {
                let Some(exit) = dependency_projections(&slot.projections) else {
                    return Ok(None);
                };

                // Coarse call-input bounds describe raw addresses. They cannot replace
                // the ordinary borrow origin/lifetime contract, including deferred selection.
                let kind = self.reference_kind(slot, ty)?;

                return Ok(kind
                    .filter(|kind| *kind == bray_symbols::DependencyReferenceKind::Address)
                    .map(|kind| {
                        bray_symbols::DependencyBorrowRelation::new(
                            None,
                            bray_symbols::DependencySubject::new(
                                self.formal_root(slot.root)
                                    .expect("reference exit has a formal root"),
                                exit,
                            ),
                            kind,
                        )
                    }));
            }
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
            Origin::Entry { projections, .. } if !exact => Some(
                projections
                    .iter()
                    .copied()
                    .map_while(dependency_projection)
                    .collect(),
            ),
            Origin::Entry { projections, .. } => dependency_projections(projections),
            _ => Some(Vec::new()),
        };

        let Some(target_projections) = target_projections else {
            return Ok(None);
        };

        Ok(Some(
            bray_symbols::DependencyBorrowRelation::new(
                Some(bray_symbols::DependencySubject::new(root, entry)),
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
