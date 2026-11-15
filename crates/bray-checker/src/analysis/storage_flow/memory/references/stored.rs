use super::super::model::{State, Target, Transfer};

impl State {
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

        self.stage_captured_whole(call, destination, incoming, complete, preserve_previous, false);
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

        self.stage_captured_whole(call, destination, incoming, complete, true, true);
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

        self.stage_captured_whole(call, destination, incoming, !regions.is_empty(), false, true);
    }

    fn stage_captured_whole(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        destination: Target,
        incoming: impl IntoIterator<Item = Transfer>,
        complete: bool,
        preserve_previous: bool,
        transport_value: bool,
    ) {
        let weak = preserve_previous || !complete;
        let previous_complete = self.values.get(&destination).is_some_and(|value| value.tracked && value.complete);

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
                value.tracked = false;
                value.complete = false;

                value.references.is_some()
            });

            captured.payloads.clear();
            captured.deferred.clear();
        }

        if !transport_value {
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
                    let complete_whole = current.tracked && current.complete && !value.tracked;
                    let incoming_whole = !weak && !current.tracked && value.tracked && value.complete;

                    current.merge(&value);

                    if complete_whole || incoming_whole {
                        current.complete = true;
                    }
                }).or_insert(value);
            }

            if transport_value {
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

        let captured_complete = captured.values.values().all(|value| !value.tracked || value.complete);
        let root = captured.values.entry(destination).or_default();

        root.tracked = true;
        root.complete = complete && (!weak || previous_complete) && captured_complete && !missing;

        for value in captured.values.values_mut() {
            if let Some(references) = &mut value.references {
                references.inexact = true;
            }

            if weak && !previous_complete {
                value.complete = false;
            }
        }

        let pending = self.pending.entry(call).or_default();

        for (slot, value) in captured.values {
            pending.values.entry(slot).and_modify(|current| current.merge_staged(&value)).or_insert(value);
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
            pending.values.entry(slot).and_modify(|current| current.merge_staged(&value)).or_insert(value);
        }

        pending.payloads.extend(captured.payloads);

        super::merge_deferred(&mut pending.deferred, &captured.deferred);
    }
}
