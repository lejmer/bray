use super::super::model::DependencyCompleteness;
use super::super::model::{AddressValue, Place, Retirement, State, Target, Transfer, ValueBinding};
use super::slots::take_subtree;

impl State {
    pub(in crate::analysis::storage_flow) fn stage_report(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        destination: Target,
        payload: Target,
        report: std::sync::Arc<bray_bound_tree::ReportDependencies>,
        complete: bool,
    ) {
        let pending = self.pending.entry(call).or_default();

        pending.values.entry(destination).or_default();
        pending.values.entry(payload).or_default().attach_report_dependencies(report, complete);
    }

    pub(in crate::analysis::storage_flow) fn report_dependencies(
        &self,
        payload: &Target,
    ) -> Option<(&std::sync::Arc<bray_bound_tree::ReportDependencies>, bool)> {
        self.values.get(payload)?.report_dependencies()
    }

    pub(in crate::analysis::storage_flow) fn carried_reports<'a>(
        &'a self,
        mut live: impl FnMut(&Target) -> bool + 'a,
    ) -> impl Iterator<Item = &'a bray_bound_tree::ReportDependencies> + 'a {
        self.may_have_report_values.then_some(&self.values).into_iter().flatten()
            .filter_map(move |(target, value)| {
                if target.previous && matches!(&target.origin, super::super::model::Origin::Storage(path) if path.is_inline()) {
                    return None;
                }

                let (report, _) = value.report_dependencies()?;

                live(target).then_some(report.as_ref())
            })
    }

    pub(in crate::analysis::storage_flow) fn capture_evaluated_value(&self, source: &super::Source,
        coordinate: Target, ty: bray_symbols::TypeId,
    ) -> bray_bound_tree::StorageCapturedValue {
        let mut captured = self.capture_source(source, &coordinate);

        if self.complete_reference_types.get(&ty) == Some(&true) {
            // A typed scalar or reference-free owner carries no source-address bounds.
            // Its initialization, deferred duties and logical value remain captured.
            for value in captured.values.values_mut() {
                value.sources.clear();
                value.address = None;
                value.address_bounds = None;
            }

            captured.values.entry(coordinate.clone()).or_default().completeness = DependencyCompleteness::Complete;
        }

        bray_bound_tree::StorageCapturedValue::new(coordinate, captured)
    }

    pub(in crate::analysis::storage_flow) fn capture_reached_value(&self, source: &Target) -> bray_bound_tree::StorageCapturedValue {
        bray_bound_tree::StorageCapturedValue::new(source.clone(), self.capture_value(source, source))
    }

    /// Shares an already produced descriptor with the logical value and replay.
    pub(in crate::analysis::storage_flow) fn capture_produced_reference(&self, slot: &Target) -> Option<bray_bound_tree::StorageCapturedValue> {
        let reference = self.values.get(slot)?.reference()?;

        if !reference.is_exact() { return None; }

        let target = reference.targets.first().expect("an exact descriptor has one address");

        (!matches!(target.origin, super::super::model::Origin::Returned { .. })).then(|| self.capture_reached_value(slot))
    }

    pub(in crate::analysis::storage_flow) fn publish_captured_value(&mut self, destination: &Target,
        value: &bray_bound_tree::StorageCapturedValue,
    ) {
        if destination.is_exact() {
            self.clear_value(destination);
            self.commit_value(value.materialize(destination));
        } else {
            self.include_captured_bounds(destination, value);
        }
    }

    /// Completes a fresh result whose physical output rows are already installed.
    pub(in crate::analysis::storage_flow) fn complete_result_capture(
        &mut self,
        destination: &Target,
        value: &bray_bound_tree::StorageCapturedValue,
    ) {
        assert!(destination.is_exact(), "a produced result has an exact storage coordinate");

        let mut additional = value.materialize(destination);
        let reported = &mut self.may_have_report_values;

        additional.values.retain(|slot, value| {
            let Some(current) = self.values.get_mut(slot) else { return true; };

            if !current.is_no_payload() {
                current.include_value(value);
            }

            *reported |= current.report_dependencies().is_some();

            false
        });

        // The first publication already installed these payload incarnations.
        additional.payloads.retain(|slot, _| !self.contents.contains_key(slot));

        self.commit_value(additional);
    }

    pub(in crate::analysis::storage_flow) fn include_captured_bounds(&mut self, region: &Target,
        value: &bray_bound_tree::StorageCapturedValue,
    ) {
        let captured = value.materialize(region);

        for (slot, value) in captured.values {
            let bounds = value.into_dependency_bounds();

            self.may_have_report_values = self.may_have_report_values || bounds.report_dependencies().is_some();

            self.values.entry(slot).and_modify(|value| value.include_carried(&bounds)).or_insert(bounds);
        }

        for (slot, contents) in captured.payloads {
            self.contents.entry(slot.clone()).or_default().merge(&contents.into_bounds(), &slot);
        }
    }

    /// Adds a reached element to a prefix with a runtime index.
    pub(in crate::analysis::storage_flow) fn accumulate_value(&mut self, region: &Target, source: &super::Source) {
        let captured = self.capture_source(source, region);

        self.commit_weak_value(captured);
    }

    pub(in crate::analysis::storage_flow) fn lose_captured_information(&mut self, destination: &Target) {
        let value = self.values.entry(destination.clone()).or_default();

        value.lose_information();
        value.completeness = DependencyCompleteness::Partial;
    }
    pub(in crate::analysis::storage_flow) fn stage_value(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        source: &Target,
        destination: Target,
    ) {
        let captured = self.capture_value(source, &destination);

        self.stage_captured_value(call, destination, captured);
    }

    pub(in crate::analysis::storage_flow) fn stage_preserved_value(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        source: &super::Source,
        destination: Target,
        preservation: &bray_symbols::DependencyValuePreservation<bray_bound_tree::StorageProjection>,
    ) {
        let mut captured = self.capture_source(source, &destination);
        let mask = |slot: &Target| preservation.subtree(&slot.projections()[destination.projections().len()..]);

        captured.values.retain(|slot, value| {
            let mask = mask(slot);

            if !mask.preserves_value() { return false; }

            if !mask.children().is_empty() { value.completeness.lose_completeness(); }

            true
        });

        // Initialization and captured frames belong to the entire unchanged subtree.
        // Partial data preservation cannot recreate either obligation from an entry snapshot.
        let complete = |slot: &Target| {
            let mask = mask(slot);

            mask.preserves_value() && mask.children().is_empty()
        };

        captured.payloads.retain(|slot, _| complete(slot));
        self.stage_captured_value(call, destination, captured);
    }

    pub(super) fn stage_captured_value(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        destination: Target,
        mut staged: Transfer,
    ) {

        // The root marks replacement of the whole value. Only actual reference
        // slots carry reference uncertainty, including fields of an aggregate.
        staged.values.entry(destination.clone()).or_default();

        let pending = self.pending.entry(call).or_default();

        if pending.values.contains_key(&destination) {
            // Alternative sources for one output must agree on its initialized payloads.
            let mut previous = super::super::model::Transfer {
                values: take_subtree(&mut pending.values, &destination, Retirement::Storage).collect(),
                payloads: take_subtree(&mut pending.payloads, &destination, Retirement::Storage).collect(),
                ..Default::default()
            };

            previous.merge(&staged);
            staged = previous;
        }

        pending.values.extend(staged.values);
        pending.payloads.extend(staged.payloads);
        pending.returned_inputs.extend(staged.returned_inputs);
    }

    pub(in crate::analysis::storage_flow) fn null(&mut self, destination: Place) {
        self.replace_value(destination, AddressValue::empty().into());
    }

    pub(in crate::analysis::storage_flow) fn no_payload(&mut self, destination: Place) {
        self.replace_value(destination, ValueBinding::no_payload());
    }

    pub(super) fn replace_value(&mut self, destination: Place, value: ValueBinding) {
        self.clear(&destination);
        self.values.insert(Target::storage(destination), value);
    }

    pub(in crate::analysis::storage_flow) fn stage_null(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        destination: Target,
    ) {
        self.stage_replacement(call, destination, AddressValue::empty().into());
    }

    pub(in crate::analysis::storage_flow) fn stage_no_payload_alternative(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        destination: Target,
    ) {
        self.pending.entry(call).or_default().values.entry(destination).or_insert_with(ValueBinding::no_payload);
    }

    fn stage_replacement(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        destination: Target,
        value: ValueBinding,
    ) {
        let pending = self.pending.entry(call).or_default();

        take_subtree(&mut pending.values, &destination, Retirement::Storage).for_each(drop);
        take_subtree(&mut pending.payloads, &destination, Retirement::Storage).for_each(drop);
        pending.values.insert(destination, value);
    }

    /// Capture all possible stored inputs before the call mutates its destination.
    pub(in crate::analysis::storage_flow) fn stage_whole_sources(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        destination: Target,
        sources: impl IntoIterator<Item = super::Source>,
        complete: bool,
        preserve_previous: bool,
    ) {
        let incoming = sources.into_iter().map(|source| self.capture_source(&source, &destination)).collect::<Vec<_>>();

        let weak = preserve_previous || !complete;
        let previous_complete = self.values.get(&destination).is_some_and(|value| value.whole_complete());

        let mut captured = if weak || !destination.is_exact() {
            self.capture_value(&destination, &destination)
        } else {
            Default::default()
        };

        if !weak && !destination.is_exact() {
            // The complete relation replaces lineage for this summary. Existing
            // descriptors remain possible at an inexact physical location.
            captured.values.retain(|_, value| {
                value.sources.clear();
                value.completeness = DependencyCompleteness::Untracked;

                value.reference().is_some()
            });

        }

        let mut missing = false;

        for incoming in incoming {
            missing |= incoming.values.is_empty();

            for (slot, value) in incoming.values {
                captured.values.entry(slot).and_modify(|current| {
                    current.include_value(&value);
                }).or_insert(value);
            }

        }

        let captured_complete = captured.values.values().all(|value| value.is_no_payload() || !value.completeness.tracked() || value.completeness.complete());
        let root = captured.values.entry(destination).or_default();

        if !root.is_no_payload() {
            root.completeness = DependencyCompleteness::for_tracked(
                complete && (!weak || previous_complete) && captured_complete && !missing);
        }

        for value in captured.values.values_mut() {
            for references in value.references_mut() {
                references.bound();
            }

            if weak && !previous_complete {
                value.completeness.lose_completeness();
            }
        }

        let pending = self.pending.entry(call).or_default();

        for (slot, value) in captured.values {
            let bounds = value.into_dependency_bounds();

            pending.values.entry(slot).and_modify(|current| current.include_carried(&bounds)).or_insert(bounds);
        }
    }

    /// Dependencies and possible duties of an unidentified element in the captured backing regions.
    pub(in crate::analysis::storage_flow) fn capture_element_bounds(&self, roots: &[Target], coordinate: &Target) -> bray_bound_tree::StorageCapturedValue {
        let mut transfer = Transfer::default();

        for root in roots {
            let direct = self.capture_base_element(root, coordinate);
            let indexed = self.capture_value(&root.project(&[bray_bound_tree::StorageProjection::AnyElement]), coordinate);

            transfer.merge(&direct);
            transfer.merge(&indexed);
        }

        bray_bound_tree::StorageCapturedValue::new(coordinate.clone(), transfer).bounded()
    }

    /// Keep the existing possible contents while an inexact operation changes a part.
    pub(in crate::analysis::storage_flow) fn stage_preserved_subtree(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        root: &Target,
    ) {
        let captured = self.capture_value(root, root);
        let pending = self.pending.entry(call).or_default();

        for (slot, value) in captured.values {
            pending.values.entry(slot).and_modify(|current| current.include_carried(&value)).or_insert(value);
        }

        pending.payloads.extend(captured.payloads);

    }
}
