use bray_symbols::{DependencyBorrowRelation, DependencyReferenceProvenance, DependencyStoredValue, DependencyStoredValuePath, DependencySubject, DependencyProjection};
use std::collections::{BTreeMap, BTreeSet};
use bray_bound_tree::StorageProjection;
use super::super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::{Origin, Place, ReferenceBounds, ReferenceExit, ReferenceExitValue};
use crate::storage::{dependency_projection, dependency_projections};
use bray_symbols::{
    ConstantBinaryOperation, ConstantTermData, ConstantUnaryOperation, DependencySubjectRoot,
    TypeData,
};

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(in crate::analysis::storage_flow::check::analysis) fn result_dependencies(
        &mut self,
    ) -> crate::CheckerQueryResult<bray_symbols::DependencyContractTemplateData, C::UpstreamError>
    {
        let mut guards = BTreeMap::new();
        let observed_result = self.trusted.observed_result.flatten();
        let mut captured_values = Vec::new();

        let exits = std::mem::take(&mut self.normal_exits);

        let result_slots = exits.iter().flat_map(|exit| exit.storage.observed_slots())
            .filter(|path| self.formal_root(path.root) == Some(DependencySubjectRoot::Result))
            .cloned().collect::<BTreeSet<_>>();

        for super::super::core::NormalExit { storage: physical, value, returned, report, effects } in exits {
            let (stored_values, embedded) = self.stored_values(&physical)?;

            let exit_references = self.borrow_relations(&physical)?.into_iter().chain(embedded);

            let inactive_payloads = physical.absent_payloads().iter().filter_map(|absent| {
                let root = self.formal_root(absent.root)?;
                let path = dependency_projections(&absent.projections)?;

                Some(DependencySubject::new(root, path))
            }).chain(result_slots.iter().filter_map(|slot| self.inactive_result_payload(slot, &value)))
                .collect::<Vec<_>>();

            let Some(crate::analysis::guarantee::NormalReturn { captured: outcome, carriers, transfers: ownership }) = returned else {
                captured_values.push(bray_symbols::CapturedValueOutcome::new(None, []).with_report(report).with_effects(effects.iter().cloned())
                    .with_storage(exit_references, stored_values).with_inactive_payloads(inactive_payloads));

                continue;
            };

            let mut published = Vec::with_capacity(carriers.len());

            for carrier in carriers {
                let guard = if let Some(guard) = guards.get(carrier.path()) {
                    *guard
                } else {
                    // Payload steps own their activity guards. A complete nullable
                    // carrier can establish a protocol even when its value is absent.
                    let guard = self.result_path_condition(carrier.path())?;

                    guards.insert(carrier.path().to_vec(), guard);

                    guard
                };

                let condition = if let Some(guard) = guard {
                    // Both terms already retain their checked portable subjects.
                    // Compose them directly rather than reconstructing local places.
                    let values = self.request.semantic_values();

                    let inactive = values
                        .intern_constant_term(ConstantTermData::Unary {
                            operation: ConstantUnaryOperation::LogicalNot,
                            operand: guard,
                        })
                        .expect("a checked result guard belongs to this semantic store");

                    values
                        .intern_constant_term(ConstantTermData::Binary {
                            operation: ConstantBinaryOperation::LogicalOr,
                            left: inactive,
                            right: carrier.guarantee(),
                        })
                        .expect("checked returned witness terms belong to this semantic store")
                } else {
                    carrier.guarantee()
                };

                let support = carrier.support().and_then(|support| support.try_map(
                    &mut (),
                    |_, place| self.witness_owner_subject(place).ok_or(()),
                    |_, term| Ok(*term),
                ).ok());

                published.push(bray_symbols::DependencyWitnessCarrier::new(
                    condition,
                    crate::analysis::guarantee::portable_carrier_path(self.request, carrier.path()),
                ).with_support(support));
            }

            captured_values.push(outcome.with_report(report).with_effects(effects.iter().cloned()).with_ownership(self.owned_result(ownership)?)
                .with_carriers(published).with_storage(exit_references, stored_values)
                .with_inactive_payloads(inactive_payloads));
        }

        self.diagnostics
            .add_range(std::mem::take(&mut self.trusted).finish(self.execution)?);

        if let Some(error) = self.query_failure.take() {
            return Err(error);
        }

        let relations = bray_symbols::CapturedValueOutcome::reference_origins(&captured_values, DependencySubject::contains);
        let normal = !captured_values.is_empty();

        let summary = std::mem::take(&mut self.results).summary();

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
            .with_normal_completion(normal)
            .with_captured_values(captured_values)
            .with_observed_result(observed_result))
    }

    fn witness_owner_subject(&self, place: &crate::ExecutionPlace) -> Option<DependencySubject> {
        use crate::execution_guarantees::ExecutionInput;
        use bray_symbols::ConstantProjectionKind;

        let (root, base) = match &place.root {
            ExecutionInput::Reference(reference) => (crate::dependency::parameter_root(self.request, *reference)?, Vec::new()),
            ExecutionInput::Storage(identity) => (self.formal_root(*identity)?, Vec::new()),
            ExecutionInput::Static(selection) => {
                let declaration = selection.template().declaration();
                let duration = self.request.static_dependencies().get(&declaration)?.static_duration()?;

                let reference = self.request.semantic_values().intern_static_reference(selection.as_ref().clone())
                    .expect("a checked static selection belongs to the semantic store");

                (DependencySubjectRoot::StaticInstance(reference, duration), Vec::new())
            }
            ExecutionInput::Pointee { target, .. } if target.is_exact() => {
                match &target.origin {
                    Origin::Storage(path) => (self.formal_root(path.root)?, dependency_projections(&path.projections)?),
                    Origin::Static { root, projections, .. } if root.is_static() =>
                        (*root, dependency_projections(projections)?),
                    _ => return None,
                }
            }
            _ => return None,
        };

        let tail = place.projections.iter().copied().map(|projection| Some(match projection {
            ConstantProjectionKind::ProductField(field) => DependencyProjection::ProductField(field),
            ConstantProjectionKind::UnionPayloadField(field) => DependencyProjection::UnionPayloadField(field),
            ConstantProjectionKind::TupleElement(index) => DependencyProjection::TupleElement(index),
            ConstantProjectionKind::NullableValue => DependencyProjection::NullableValue,
            ConstantProjectionKind::ArrayElement(index) => DependencyProjection::Element(index),
            ConstantProjectionKind::ArrayElementOrdinal(_) | ConstantProjectionKind::ArraySlice { .. } => return None,
        })).collect::<Option<Vec<_>>>()?;

        Some(DependencySubject::new(root, base.into_iter().chain(tail)))
    }

    fn inactive_result_payload(&self, slot: &Place, value: &crate::ExecutionCondition) -> Option<DependencySubject> {
        let mut value = value.clone();

        for (depth, projection) in slot.projections.iter().copied().enumerate() {
            let inactive = match projection {
                StorageProjection::ActiveUnionPayloadField { variant, .. } =>
                    value.known_variant().is_some_and(|active| active != variant),
                StorageProjection::NullableValue => value.known_presence() == Some(false),
                _ => false,
            };

            if inactive {
                return Some(DependencySubject::new(DependencySubjectRoot::Result, dependency_projections(&slot.projections[..=depth])?));
            }

            value = value.project(projection);
        }

        None
    }

    fn borrow_relations(&self, references: &ReferenceExit)
        -> crate::CheckerQueryResult<Vec<DependencyBorrowRelation>, C::UpstreamError>
    {
        let mut relations = Vec::new();
        let mut unbounded = std::collections::BTreeSet::new();
        let mut allocations = std::collections::BTreeMap::new();

        for (slot, target, exact, bounds) in references.targets() {
            if self.formal_root(slot.root).is_none() {
                continue;
            }

            let Some(ty) = self.storage.identity_type(slot.root) else {
                continue;
            };

            let allocation_relation = if let Origin::Allocation { allocation, projections } = target
                && bounds != ReferenceBounds::InputRegions && bounds != ReferenceBounds::Unknown
                && !projections.contains(&bray_bound_tree::StorageProjection::OwnedTarget)
                && self.reference_kind(slot, ty)? == Some(bray_symbols::DependencyReferenceKind::Address)
                && let Some(exit) = dependency_projections(&slot.projections)
                && let Some(target_projections) = dependency_projections(projections)
            {
                let exit = bray_symbols::DependencySubject::new(
                    self.formal_root(slot.root).expect("allocation output has a formal root"), exit);

                let anchor = allocations.entry(allocation.clone()).or_insert_with(|| exit.clone());

                Some(bray_symbols::DependencyBorrowRelation::new_allocation(
                    anchor.clone(), exit, bray_symbols::DependencyReferenceKind::Address)
                    .with_target_projections(target_projections))
            } else { None };

            let relation = if allocation_relation.is_some() { allocation_relation }
                else { self.result_reference(slot, target, ty, exact, false)? };

            if let Some(relation) = relation {
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

                    let relation = if bounds == ReferenceBounds::InputRegions
                        || relation.target_projections().contains(&bray_symbols::DependencyProjection::AnyElement) {
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

        let empty = references.empty_slots().map(|slot| (slot, false));
        let absent = references.no_payload_slots().map(|slot| (slot, true));

        for (slot, absent) in empty.chain(absent) {
            let Some(root) = self.formal_root(slot.root) else { continue; };

            let ty = self.storage.identity_type(slot.root).expect("published reference has its storage type");

            let Some(kind) = self.reference_kind(slot, ty)? else { continue; };

            let Some(projections) = dependency_projections(&slot.projections) else { continue; };

            let exit = bray_symbols::DependencySubject::new(root, projections);

            let relation = if absent { DependencyBorrowRelation::new_no_payload(exit, kind) }
                else { DependencyBorrowRelation::new_empty(exit, kind) };

            relations.push((slot.clone(), relation));
        }

        Ok(relations.into_iter()
            .filter_map(|(slot, relation)| (!unbounded.contains(&slot)).then_some(relation))
            .collect())
    }

    // Map storage within the producing exit while its allocation anchors are still distinct.
    fn stored_values(&self, exit: &ReferenceExit) -> crate::CheckerQueryResult<(Vec<DependencyStoredValue>, Vec<DependencyBorrowRelation>), C::UpstreamError> {
        type Paths = BTreeMap<DependencyStoredValuePath, (BTreeSet<DependencyStoredValuePath>, bool)>;
        type Physical = BTreeMap<DependencyStoredValuePath, (BTreeSet<DependencyBorrowRelation>, bool)>;

        let mut anchors = BTreeMap::<Place, Vec<(Place, Vec<bray_bound_tree::StorageProjection>, bool)>>::new();

        for (slot, origin, exact, bounds) in exit.targets() {
            let Origin::Allocation { allocation, projections } = origin else { continue; };

            if !exact || bounds != ReferenceBounds::Known { continue; }

            let Some(ty) = self.storage.identity_type(slot.root) else { continue; };

            let Some(root) = self.formal_root(slot.root) else { continue; };

            let observable = root == DependencySubjectRoot::Result
                || root.is_static()
                || matches!(self.request.semantic_values().type_data(ty).as_ref(), TypeData::Borrow { .. })
                || slot.projections.contains(&bray_bound_tree::StorageProjection::AddressTarget);

            if observable && self.reference_kind(slot, ty)? == Some(bray_symbols::DependencyReferenceKind::Address) {
                let mut anchor = slot.clone();
                let mut owned = false;

                if let Some(bray_bound_tree::StorageProjection::ProductField(pointer)) = slot.projections.last() {
                    let owner = &slot.projections[..slot.projections.len() - 1];

                    if let Some((owner_ty, _)) = crate::storage::reference_path(self.request, ty, owner.into(), None)? {
                        let owner_ty = self.request.semantic_values().unborrowed_type(owner_ty);

                        if self.input.raw_buffers.get(&owner_ty).is_some_and(|fields| fields.pointer == *pointer) {
                            anchor.projections = owner.into();
                            owned = true;
                        }
                    }
                }

                anchors.entry(allocation.clone()).or_default().push((anchor, projections.to_vec(), owned));
            }
        }

        let mut paths = Paths::new();

        for (destination, sources, complete) in exit.whole_sources() {
            let Some((destination, _, precise)) = self.stored_endpoint(destination, false, &anchors)? else { continue; };

            let mut mapped = BTreeSet::new();
            let mut complete = complete && precise;

            for source in sources {
                if let Some((source, _, precise)) = self.stored_endpoint(source, true, &anchors)? {
                    mapped.insert(source);
                    complete &= precise;
                } else { complete = false; }
            }

            let entry = paths.entry(destination).or_insert_with(|| (BTreeSet::new(), true));

            entry.0.extend(mapped);
            entry.1 &= complete;
        }

        // Physical reference payloads keep their actual reference origins.
        // These bounds do not forward borrow authority to the caller.
        let mut physical = Physical::new();

        for (destination, value) in exit.stored_bindings() {
            if !matches!(&destination.origin, Origin::Allocation { .. })
                && !destination.projections().contains(&bray_bound_tree::StorageProjection::OwnedTarget) {
                continue;
            }

            let Some((endpoint, place, precise)) = self.stored_endpoint(destination, false, &anchors)? else { continue; };

            let entry = physical.entry(endpoint.clone()).or_insert_with(|| (BTreeSet::new(), true));

            let Some(ty) = self.storage.identity_type(place.root) else { entry.1 = false; continue; };

            let Some(kind) = self.physical_reference_kind(&place, ty)? else { entry.1 = false; continue; };

            if !precise || endpoint.dereferences() != 0 {
                entry.1 = false;
                continue;
            }

            let ReferenceExitValue::Address { targets, nullable, bounds } = value else {
                entry.0.insert(DependencyBorrowRelation::new_no_payload(endpoint.subject().clone(), kind));
                continue;
            };

            if bounds != ReferenceBounds::Known { entry.1 = false; continue; }

            if nullable {
                if destination.is_exact() {
                    entry.0.insert(DependencyBorrowRelation::new_empty(endpoint.subject().clone(), kind));
                } else { entry.1 = false; }
            }

            for target in targets {
                if target.previous { entry.1 = false; continue; }

                let precise_source = match &target.origin {
                    Origin::Entry { reference, projections, .. } =>
                        self.stored_projections(&reference.projections).is_some_and(|(_, precise)| precise)
                            && self.stored_projections(projections).is_some_and(|(_, precise)| precise),
                    Origin::Storage(place) => self.stored_projections(&place.projections).is_some_and(|(_, precise)| precise),
                    _ => true,
                };

                if !precise_source { entry.1 = false; continue; }

                let Some(relation) = self.result_reference(&place, &target.origin, ty, false, true)? else {
                    entry.1 = false;
                    continue;
                };

                if relation.entry().is_none() {
                    entry.1 = false;
                } else {
                    entry.0.insert(relation.with_provenance(DependencyReferenceProvenance::InputRegions));
                }
            }
        }

        let mut values = paths.into_iter().map(|(destination, (sources, complete))|
            DependencyStoredValue::new(destination, sources, complete)).collect::<Vec<_>>();

        let mut references = Vec::new();

        for (destination, (relations, complete)) in physical {
            if complete {
                references.extend(relations);
            } else {
                values.push(DependencyStoredValue::new(destination, [], false));
            }
        }

        Ok((values, references))
    }

    fn stored_endpoint(
        &self,
        target: &crate::analysis::storage_flow::memory::Target,
        source: bool,
        anchors: &std::collections::BTreeMap<Place, Vec<(Place, Vec<bray_bound_tree::StorageProjection>, bool)>>,
    ) -> crate::CheckerQueryResult<Option<(bray_symbols::DependencyStoredValuePath, Place, bool)>, C::UpstreamError> {

        if target.previous { return Ok(None); }

        let (place, terminal) = match &target.origin {
            Origin::Storage(place) => {
                let Some(root) = self.formal_root(place.root) else { return Ok(None); };

                if source == (root == DependencySubjectRoot::Result) { return Ok(None); }

                (place.clone(), 0)
            }
            Origin::Entry { reference, layer, kind, projections } => {
                let Some(root) = self.formal_root(reference.root) else { return Ok(None); };

                if root == DependencySubjectRoot::Result { return Ok(None); }

                let mut projections = projections.to_vec();
                let mut terminal = *layer;

                if *kind == bray_symbols::DependencyReferenceKind::Address {
                    projections.insert(0, StorageProjection::AddressTarget);
                    terminal = terminal.checked_sub(1).expect("entry reference has a positive layer");
                }

                let mut combined = reference.projections.to_vec();

                combined.extend(projections.iter().copied());

                if !projections.is_empty() {
                    let Some(ty) = self.storage.identity_type(reference.root) else { return Ok(None); };

                    let Some((_, prepared)) = crate::storage::reference_path(self.request, ty, combined.clone().into(), None)? else { return Ok(None); };

                    let implied = prepared.reads().filter(|(_, index, _, _)| *index == reference.projections.len()).count();
                    let expected = if *kind == bray_symbols::DependencyReferenceKind::Address { *layer } else { terminal };

                    if implied != expected { return Ok(None); }
                    terminal = 0;
                }

                (Place { root: reference.root, projections: combined.into() }, terminal)
            }
            Origin::Allocation { allocation, projections } if !source => {
                let Some((anchor, prefix, owned)) = anchors.get(allocation).into_iter().flatten()
                    .find(|(_, prefix, _)| projections.starts_with(prefix)) else { return Ok(None); };

                let mut path = anchor.projections.to_vec();

                path.push(if *owned { StorageProjection::OwnedTarget } else { StorageProjection::AddressTarget });
                path.extend_from_slice(&projections[prefix.len()..]);

                let Some(ty) = self.storage.identity_type(anchor.root) else { return Ok(None); };

                if crate::storage::reference_path(self.request, ty, path.clone().into(), None)?.is_none() {
                    return Ok(None);
                }

                (Place { root: anchor.root, projections: path.into() }, 0)
            }
            _ => return Ok(None),
        };

        let Some(root) = self.formal_root(place.root) else { return Ok(None); };

        let Some((projections, precise)) = self.stored_projections(&place.projections) else { return Ok(None); };

        Ok(Some((DependencyStoredValuePath::new(DependencySubject::new(root, projections),
            u32::try_from(terminal).expect("checked borrow depth fits the portable path")), place, precise)))
    }

    // These paths describe may-carry storage, so dynamic selectors may widen
    // without changing the authority-bearing projection conversion.
    fn stored_projections(&self, projections: &[bray_bound_tree::StorageProjection]) -> Option<(Vec<bray_symbols::DependencyProjection>, bool)> {
        let mut precise = true;
        let mut result = Vec::with_capacity(projections.len());

        for projection in projections.iter().copied() {
            result.push(match projection {
                StorageProjection::AnyElement => DependencyProjection::AnyElement,
                StorageProjection::ElementFromStart(_) => crate::analysis::guarantee::portable_carrier_path(self.request, &[projection])
                    .into_iter().next().expect("one checked array selector has one portable projection"),
                StorageProjection::Element(_) | StorageProjection::ElementFromEnd(_) => {
                    precise = false;

                    DependencyProjection::AnyElement
                }
                _ => dependency_projection(projection)?,
            });
        }

        Some((result, precise))
    }

    fn physical_reference_kind(&self, slot: &Place, mut ty: bray_symbols::TypeId)
        -> crate::CheckerQueryResult<Option<bray_symbols::DependencyReferenceKind>, C::UpstreamError> {
        for projection in slot.projections.iter().copied() {
            let projected = crate::storage::projected_storage_type(self.request, ty, projection)?;

            let Some(projected) = *projected.value() else { return Ok(None); };

            ty = projected;
        }

        Ok(crate::representation::reference_kind(self.request, ty))
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
            bray_bound_tree::StorageIdentity::Static(selection) => {
                let declaration = selection.template().declaration();
                let values = self.request.semantic_values();

                let duration = self.request.static_dependencies()[&declaration].static_duration()
                    .expect("prepared static roots retain their ownership domain");

                let reference = values.intern_static_reference(selection.as_ref().clone())
                    .expect("planned static references fit the semantic store");

                return Some(DependencySubjectRoot::StaticInstance(reference, duration));
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
        physical: bool,
    ) -> crate::CheckerQueryResult<Option<bray_symbols::DependencyBorrowRelation>, C::UpstreamError>
    {
        let static_source = match origin {
            Origin::Storage(path) => self.formal_root(path.root).filter(|root| root.is_static())
                .map(|root| (root, path.projections.as_ref())),
            Origin::Static { root, projections, .. } => Some((*root, projections.as_ref())),
            _ => None,
        };

        if let Some((root, projections)) = static_source {
            let Some(entry) = dependency_projections(projections) else { return Ok(None); };

            let Some(exit) = dependency_projections(&slot.projections) else { return Ok(None); };

            let kind = if physical { self.physical_reference_kind(slot, ty)? } else { self.reference_kind(slot, ty)? };

            let Some(kind) = kind else { return Ok(None); };

            let exit_root = self.formal_root(slot.root).expect("static borrow exits through a formal subject");

            return Ok(Some(DependencyBorrowRelation::new(
                Some(DependencySubject::new(root, entry)),
                DependencySubject::new(exit_root, exit),
                kind,
            )));
        }

        let source = match origin {
            Origin::Entry {
                reference: path, ..
            } => Some((path, true)),
            Origin::Storage(path) => Some((path, false)),
            Origin::Allocation { .. }
            | Origin::Returned { .. }
            | Origin::Static { .. } => None,
        };

        let Some((target, stored_reference)) = source else {
            let Some(exit) = (if physical { self.stored_projections(&slot.projections).map(|(path, _)| path) } else { dependency_projections(&slot.projections) }) else {
                return Ok(None);
            };

            let kind = if physical { self.physical_reference_kind(slot, ty)? } else { self.reference_kind(slot, ty)? };

            // An ordinary borrow's lifetime is established separately. Static and
            // forwarded external origins need no invented alias to a call input.
            return Ok(kind
                .filter(|kind| {
                    *kind == bray_symbols::DependencyReferenceKind::Address
                        || !matches!(origin, Origin::Allocation { .. })
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

        let Some(entry) = (if physical { self.stored_projections(&target.projections).map(|(path, _)| path) } else { dependency_projections(&target.projections) }) else {
            return Ok(None);
        };

        let Some(exit) = (if physical { self.stored_projections(&slot.projections).map(|(path, _)| path) } else { dependency_projections(&slot.projections) }) else {
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

        let kind = if physical { self.physical_reference_kind(slot, ty)? } else { self.reference_kind(slot, ty)? };

        let Some(kind) = kind else { return Ok(None); };

        let target_projections = match origin {
            Origin::Entry { projections, .. } if physical => self.stored_projections(projections).map(|(path, _)| path),
            Origin::Entry { projections, .. } if !exact =>
                projections
                    .iter()
                    .copied()
                    .map(|projection| match projection {
                        bray_bound_tree::StorageProjection::Element(_)
                        | bray_bound_tree::StorageProjection::AnyElement
                        | bray_bound_tree::StorageProjection::ElementFromStart(_)
                        | bray_bound_tree::StorageProjection::ElementFromEnd(_) => Some(bray_symbols::DependencyProjection::AnyElement),
                        projection => dependency_projection(projection),
                    })
                    .collect::<Option<Vec<_>>>(),
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
