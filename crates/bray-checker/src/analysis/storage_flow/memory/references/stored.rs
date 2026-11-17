use super::super::model::DependencyCompleteness;
use super::super::model::{AddressValue, Place, Retirement, State, Target, Transfer, ValueBinding};
use super::slots::take_subtree;

#[derive(Clone, Copy, Eq, PartialEq)]
enum WholeValueKind {
    Dependencies,
    Value,
}

impl State {
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

    pub(in crate::analysis::storage_flow) fn stage_no_payload(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        destination: Target,
    ) {
        self.stage_replacement(call, destination, ValueBinding::no_payload());
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
        take_subtree(&mut pending.deferred, &destination, Retirement::Storage).for_each(drop);
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

        self.stage_captured_whole(call, destination, incoming, complete, preserve_previous, WholeValueKind::Dependencies);
    }

    /// Stage an intrinsic buffer insertion, including the actual value's storage state.
    pub(in crate::analysis::storage_flow) fn stage_buffer_push(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        destination: Target,
        sources: impl IntoIterator<Item = super::Source>,
        complete: bool,
    ) {
        let incoming = sources.into_iter().map(|source| self.capture_source(&source, &destination)).collect::<Vec<_>>();

        self.stage_captured_whole(call, destination, incoming, complete, true, WholeValueKind::Value);
    }

    /// Capture a base-addressed element and indexed elements as alternative values.
    pub(in crate::analysis::storage_flow) fn stage_buffer_elements(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        destination: Target,
        regions: &[Target],
    ) {
        let mut incoming = Vec::new();

        for region in regions {
            let direct = self.capture_base_element(region, &destination);

            if !direct.values.is_empty() || !direct.payloads.is_empty() || !direct.deferred.is_empty() {
                incoming.push(direct);
            }

            let selected = region.project(&[bray_bound_tree::StorageProjection::AnyElement]);

            incoming.push(self.capture_value(&selected, &destination));
        }

        self.stage_captured_whole(call, destination, incoming, !regions.is_empty(), false, WholeValueKind::Value);
    }

    fn stage_captured_whole(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        destination: Target,
        incoming: impl IntoIterator<Item = Transfer>,
        complete: bool,
        preserve_previous: bool,
        kind: WholeValueKind,
    ) {
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

            captured.payloads.clear();
            captured.deferred.clear();
        }

        if kind == WholeValueKind::Dependencies {
            captured.payloads.clear();
            captured.deferred.clear();
        }

        let mut missing = false;

        for incoming in incoming {
            // A deferred frame alone does not describe the source's ordinary
            // stored subtree. Reference-free sources are omitted by the caller.
            missing |= incoming.values.is_empty();

            for (slot, value) in incoming.values {
                captured.values.entry(slot).and_modify(|current| {
                    current.include_value(&value);
                }).or_insert(value);
            }

            if kind == WholeValueKind::Value {
                for (slot, mut contents) in incoming.payloads {
                    if weak {
                        contents.initialized.clear();
                    }

                    captured.payloads.entry(slot.clone()).and_modify(|previous| {
                        previous.merge(&contents, &slot);
                    }).or_insert(contents);
                }

                super::merge_deferred(&mut captured.deferred, &incoming.deferred);
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
            match kind {
                WholeValueKind::Value => { pending.values.insert(slot, value); }
                WholeValueKind::Dependencies => {
                    let bounds = value.into_dependency_bounds();

                    pending.values.entry(slot).and_modify(|current| current.include_carried(&bounds)).or_insert(bounds);
                }
            }
        }

        for (slot, contents) in captured.payloads {
            pending.payloads.entry(slot.clone()).and_modify(|previous| { previous.merge(&contents, &slot); }).or_insert(contents);
        }

        super::merge_deferred(&mut pending.deferred, &captured.deferred);
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

        super::merge_deferred(&mut pending.deferred, &captured.deferred);
    }
}
