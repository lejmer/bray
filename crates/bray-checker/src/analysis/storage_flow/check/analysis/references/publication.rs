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
        let mut guards = std::collections::BTreeMap::new();
        let mut published = Vec::with_capacity(carriers.len());

        for carrier in carriers {
            let guard = if let Some(guard) = guards.get(carrier.path()) {
                *guard
            } else {
                let guard = self.result_path_condition(carrier.path(), true)?;
                guards.insert(carrier.path().to_vec(), guard);
                guard
            };

            let condition = if let Some(guard) = guard {
                let inputs = self.request.trusted_contracts().expect("returned witness retains formal inputs");
                let decode = |term| crate::execution_condition_from_type_term(self.request.semantic_values(), term, inputs.inputs.len())
                    .expect("checked returned witness decodes in the same store");
                let condition = crate::ExecutionCondition::operation(
                    bray_bound_tree::BoundOperator::LogicalOr,
                    vec![crate::ExecutionCondition::operation(bray_bound_tree::BoundOperator::LogicalNot, vec![decode(guard)]), decode(carrier.guarantee())],
                );
                crate::analysis::guarantee::portable_carrier(self.request, &condition)
                    .expect("guarded returned witness retains its portable inputs")
            } else {
                carrier.guarantee()
            };

            published.push(bray_symbols::DependencyWitnessCarrier::new(condition, crate::analysis::guarantee::portable_carrier_path(self.request, carrier.path())));
        }

        let observed_result = self.trusted.observed_result.flatten();

        let owned_result = self.trusted.owned_result(self.execution);
        let owned_result = self.owned_result(owned_result)?;

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
                    ReferenceBounds::InputRegions => true,
                    ReferenceBounds::Displaced => {
                        relation.kind() == bray_symbols::DependencyReferenceKind::Address
                    }
                    ReferenceBounds::Unknown => false,
                };

                if bounded {
                    let exact = exact && relation.entry().is_some();

                    let relation = relation.with_exactness(exact);

                    let relation = if bounds == ReferenceBounds::InputRegions {
                        relation.with_provenance(
                            bray_symbols::DependencyReferenceProvenance::InputRegions,
                        )
                    } else {
                        relation
                    };

                    relations.push((slot.clone(), relation));
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

        // Established borrow origins retain references installed by mutation.
        // Possible physical input regions do not establish a borrow origin.
        requirements.extend(
            relations
                .iter()
                .filter(|relation| {
                    relation.kind() == bray_symbols::DependencyReferenceKind::Borrow
                        && relation.provenance()
                            != bray_symbols::DependencyReferenceProvenance::InputRegions
                        && relation.entry().is_some()
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
            .with_carriers(published)
            .with_owned_result(owned_result)
            .with_observed_result(observed_result)
            .with_normal_storage_preservation(!self.has_normal_effects))
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
            bray_bound_tree::StorageIdentity::Static(declaration) => {
                return Some(
                    *self
                        .request
                        .static_dependencies()
                        .get(&declaration)
                        .expect("planned static storage retains its prepared dependency root"),
                );
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
        let source = match origin {
            Origin::Entry {
                reference: path, ..
            } => Some((path, true)),
            Origin::Storage(path)
                if !matches!(
                    self.storage.identity(path.root),
                    Some(bray_bound_tree::StorageIdentity::Static(_))
                ) =>
            {
                Some((path, false))
            }
            Origin::Storage(_) | Origin::Allocation(_) | Origin::Returned { .. }
            | Origin::Static { .. } => None,
        };

        let Some((target, stored_reference)) = source else {
            let Some(exit) = dependency_projections(&slot.projections) else {
                return Ok(None);
            };

            let kind = self.reference_kind(slot, ty)?;

            // An ordinary borrow's lifetime is established separately. Static and
            // forwarded external origins need no invented alias to a call input.
            return Ok(kind
                .filter(|kind| {
                    *kind == bray_symbols::DependencyReferenceKind::Address
                        || !matches!(origin, Origin::Allocation(_))
                })
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
