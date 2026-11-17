use std::sync::Arc;
use bray_bound_tree::{StorageCapturedValue, StorageProjection};
use crate::execution_guarantees::ExecutionDeferredValue;
use bray_symbols::CapturedValueOperation as Operation;

use super::{ExecutionCondition, ExecutionConditionKind as K, ExecutionOperands};

impl ExecutionCondition {
    /// Retains the captured environment on the actual value rather than in an owner lookup table.
    pub(crate) fn with_deferred_value(self, frame: std::sync::Arc<crate::execution_guarantees::ExecutionDeferredValue>) -> Self {
        if let K::CapturedValue { value, storage } = self.kind() {
            return value.as_ref().clone().with_deferred_value(frame).captured_value(storage.as_ref().clone());
        }

        let value = match self.kind() {
            K::DeferredValue { value, .. } => std::sync::Arc::clone(value),
            _ => std::sync::Arc::new(self),
        };

        Self::new(K::DeferredValue { value, frame })
    }

    /// The same latent environment carried through moves and owning wrappers.
    pub(crate) fn deferred_value(&self) -> Option<&std::sync::Arc<crate::execution_guarantees::ExecutionDeferredValue>> {
        match self.kind() {
            K::DeferredValue { frame, .. } => Some(frame),
            K::CapturedValue { value, .. } | K::Observation { contents: value, .. } | K::Borrowed(value) => value.deferred_value(),
            _ => None,
        }
    }

    /// Captures physical information with this evaluated value, sharing its data and carried graph.
    pub(crate) fn captured_value(self, storage: bray_bound_tree::StorageCapturedValue) -> Self {
        if self.captured_storage() == Some(&storage) { return self; }

        let value = match self.kind() {
            K::CapturedValue { value, .. } => std::sync::Arc::clone(value),
            _ => std::sync::Arc::new(self),
        };

        Self::new(K::CapturedValue { value, storage: std::sync::Arc::new(storage) })
    }

    pub(crate) fn captured_storage(&self) -> Option<&bray_bound_tree::StorageCapturedValue> {
        match self.kind() { K::CapturedValue { storage, .. } => Some(storage), _ => None }
    }

    /// The inline snapshot under a data observation, stopping at reference descriptors.
    pub(crate) fn inline_captured_storage(&self) -> Option<&bray_bound_tree::StorageCapturedValue> {
        let mut value = self;

        loop {
            match value.inline_contents().kind() {
                K::CapturedValue { storage, .. } => return Some(storage),
                K::Tagged(_, contents) => value = contents,
                _ => return None,
            }
        }
    }

    /// Proof data observes the value, independently of physical transport metadata.
    /// CurrentValues retains the paired snapshot for storage propagation and fixed-point equality.
    pub(crate) fn without_captured_storage(&self) -> Self {
        Self::map_value_metadata(|_| None, |_| false, |_, frame| Some(Arc::clone(frame)))(self)
    }

    /// Actual owning subvalues and their prepared observation roles, expressed as value selectors.
    pub(crate) fn captured_observations(&self) -> Vec<(Vec<StorageProjection>, bray_bound_tree::StorageObservationKind)> {
        let mut pending = vec![(Vec::new(), self)];
        let mut observations = Vec::new();

        while let Some((path, value)) = pending.pop() {
            if let Some(storage) = value.inline_captured_storage() {
                observations.extend(storage.observations().map(|(suffix, kind)|
                    (path.iter().copied().chain(suffix.iter().copied()).collect(), kind)));
            }

            if value.is_reference() { continue; }

            match value.computed_value().kind() {
                K::Tagged(_, contents) => pending.push((path, contents)),
                K::Constructed(_, fields) => pending.extend(fields.iter().map(|(field, value)|
                    (path.iter().copied().chain([*field]).collect(), value))),
                K::Present(contents) => pending.push((path.iter().copied().chain([StorageProjection::NullableValue]).collect(), contents)),
                K::StorageValue(Operation::OwningStorage(_), contents) => {
                    let mut prefix = &contents[0];
                    let mut index = 0u32;

                    while let K::StorageValue(Operation::Append, values) = prefix.computed_value().kind() {
                        pending.push((path.iter().copied().chain([StorageProjection::ElementFromEnd(bray_symbols::SymbolOrdinal::new(index))]).collect(), &values[1]));
                        prefix = &values[0];
                        index = index.checked_add(1).expect("captured initialized prefix fits storage ordinals");
                    }
                }
                _ => {}
            }
        }

        observations
    }

    /// Retires one storage incarnation throughout a forest of shared evaluated values.
    pub(crate) fn retiring_storage(target: &bray_bound_tree::StorageTarget,
        retirement: bray_bound_tree::StorageValueRetirement,
    ) -> impl FnMut(&Self) -> Self {
        Self::map_value_metadata(move |storage| Some(if storage.references(target, retirement) {
            Arc::new(storage.retired(target, retirement))
        } else { Arc::clone(storage) }), |_| true, move |_, frame| {
            if !frame.storage_references(target, retirement) { return Some(Arc::clone(frame)); }

            let mut retired = frame.as_ref().clone();

            retired.retire_storage(target, retirement);

            Some(Arc::new(retired))
        })
    }

    /// Shares transformed records across all holders in a single state transition.
    pub(crate) fn mapping_frames(
        keep: impl Fn(&Self) -> bool,
        map: impl FnMut(&Arc<ExecutionDeferredValue>, &Arc<ExecutionDeferredValue>) -> Option<Arc<ExecutionDeferredValue>>,
    ) -> impl FnMut(&Self) -> Self {
        Self::map_value_metadata(|storage| Some(Arc::clone(storage)), keep, map)
    }

    fn map_value_metadata(
        mut storage: impl FnMut(&Arc<StorageCapturedValue>) -> Option<Arc<StorageCapturedValue>>,
        keep: impl Fn(&Self) -> bool,
        mut deferred: impl FnMut(&Arc<ExecutionDeferredValue>, &Arc<ExecutionDeferredValue>) -> Option<Arc<ExecutionDeferredValue>>,
    ) -> impl FnMut(&Self) -> Self {
        use std::collections::HashMap;

        // Retain each original while its address indexes this traversal.
        let mut mapped = HashMap::<*const K, (Self, Self)>::new();
        let mut frames = HashMap::new();

        move |root| {
            let mut pending = vec![(root, false)];

            while let Some((value, expanded)) = pending.pop() {
                let key = std::ptr::from_ref(value.kind());

                if mapped.contains_key(&key) { continue; }

                if !expanded {
                    pending.push((value, true));

                    if let K::DeferredValue { value: data, .. } = value.kind() && !keep(value) {
                        pending.push((data.as_ref(), false));
                    } else {
                        pending.extend(value.structural_children().map(|child| (child, false)));
                    }

                    continue;
                }

                let child = |child: &Self| mapped[&std::ptr::from_ref(child.kind())].1.clone();
                let changed = |original: &Self| !std::ptr::eq(child(original).kind(), original.kind());

                let rewritten = match value.kind() {
                    K::CapturedValue { value: data, storage: original } => match storage(original) {
                        None => child(data),
                        Some(snapshot) if !changed(data) && Arc::ptr_eq(&snapshot, original) => value.clone(),
                        Some(snapshot) => Self::new(K::CapturedValue { value: Arc::new(child(data)), storage: snapshot }),
                    },
                    K::DeferredValue { value: data, .. } if !keep(value) => child(data),
                    K::DeferredValue { value: data, frame } => {
                        let updated = frames.entry(Arc::as_ptr(frame)).or_insert_with(|| {
                            let mapped_frame = if frame.conditions().any(changed) {
                                Arc::new(frame.try_map_conditions(|condition| Ok::<_, std::convert::Infallible>(child(condition)))
                                    .expect("mapping existing conditions is infallible"))
                            } else { Arc::clone(frame) };

                            deferred(frame, &mapped_frame)
                        });

                        match updated {
                            None => child(data),
                            Some(updated) if !changed(data) && Arc::ptr_eq(updated, frame) => value.clone(),
                            Some(updated) => child(data).with_deferred_value(Arc::clone(updated)),
                        }
                    }
                    _ if value.structural_children().any(changed) => value.map_children(child),
                    _ => value.clone(),
                };

                mapped.insert(key, (value.clone(), rewritten));
            }

            mapped[&std::ptr::from_ref(root.kind())].1.clone()
        }
    }

    /// Private production supplies the identity of fresh data, without replacing captured inputs.
    pub(crate) fn with_producer(self, producer: Self) -> Self {
        match self.kind() {
            K::CapturedValue { value, storage } => value.as_ref().clone().with_producer(producer).captured_value(storage.as_ref().clone()),
            K::DeferredValue { value, frame } => value.as_ref().clone().with_producer(producer).with_deferred_value(std::sync::Arc::clone(frame)),
            K::Unknown => producer,
            K::Constructed(base, fields) => {
                let fields = fields.iter().map(|(field, value)|
                    (*field, value.clone().with_producer(producer.clone().project(*field)))).collect();

                let base = if base.is_unknown() { producer } else { base.as_ref().clone() };

                Self::constructed(base, fields)
            }
            K::Observation { owner, contents, kind } => Self::observation(
                if owner.is_unknown() { producer.clone() } else { owner.as_ref().clone() },
                contents.as_ref().clone().with_producer(producer), *kind),
            K::Present(value) => value.as_ref().clone().with_producer(producer.project(StorageProjection::NullableValue)).present(),
            K::Tagged(variant, value) => Self::new(K::Tagged(*variant, std::sync::Arc::new(value.as_ref().clone().with_producer(producer)))),
            _ => self,
        }
    }
    /// Applies a checked value operation. Bounds never enter this constructor.
    pub(crate) fn storage_value(operation: Operation<StorageProjection>, operands: ExecutionOperands) -> Self {
        assert_eq!(operation.operand_count(), operands.len(), "checked captured-value operation retains its operands");

        match (operation, operands.as_ref()) {
            (Operation::Present, [value]) => return Self::new(K::Present(std::sync::Arc::new(value.clone()))),
            (Operation::Inline, [value]) if value.is_unknown()
                || matches!(value.computed_value().kind(), K::StorageValue(Operation::Inline, _)) => return value.clone(),
            (Operation::Append, [prefix, value]) => {
                if let K::StorageValue(Operation::OwningStorage(pointer), values) = prefix.computed_value().kind() {
                    return Self::storage_value(Operation::OwningStorage(*pointer), vec![Self::storage_value(Operation::Append,
                        vec![values[0].clone(), value.clone()].into())].into());
                }
            }
            (Operation::Read, [value]) => {
                let mut reference = value;

                let contents = loop {
                    match reference.kind() {
                        K::CapturedValue { value, .. } | K::Observation { contents: value, kind: super::ExecutionObservationKind::Scalar, .. } => reference = value,
                        K::Borrowed(contents) | K::Observation { contents, kind: super::ExecutionObservationKind::Reference, .. } => break contents.as_ref().clone(),
                        K::Unknown => break Self::unknown(),
                        _ => panic!("a checked captured read receives one reference layer: {reference:?}"),
                    }
                };

                return Self::scalar_observation(value.owner_identity().clone(), contents);
            }
            (Operation::Add | Operation::Subtract, [left, right]) => return Self::operation(
                if operation == Operation::Add { bray_bound_tree::BoundOperator::Add } else { bray_bound_tree::BoundOperator::Subtract },
                vec![left.computed_value().clone(), right.computed_value().clone()]),
            (Operation::Reference, [owner, contents]) => {
                let value = Self::reference_value(owner.owner_identity().clone(), contents.clone());

                return match owner.inline_captured_storage() {
                    Some(storage) => value.captured_value(storage.clone()),
                    None => value,
                };
            }
            (Operation::Owned, [owner, contents]) => return Self::owned_value(owner.copied_value().clone(), contents.copied_value().clone()),
            (Operation::Scalar, [owner, contents]) => return Self::scalar_observation(owner.clone(), contents.clone()),
            (Operation::Select(projection), [value]) => {
                if let K::Constructed(_, fields) = value.computed_value().kind()
                    && let Some((_, value)) = fields.iter().find(|(field, _)| *field == projection)
                {
                    return value.clone();
                }

                if projection != StorageProjection::OwnedTarget {
                    return value.clone().project(projection);
                }
            }
            (Operation::Last | Operation::Remainder, [prefix]) => {
                if let K::StorageValue(Operation::OwningStorage(pointer), values) = prefix.computed_value().kind() {
                    let selected = Self::storage_value(operation, vec![values[0].clone()].into());

                    return if operation == Operation::Remainder { Self::storage_value(Operation::OwningStorage(*pointer), vec![selected].into()) }
                        else { selected };
                }

                if let K::StorageValue(Operation::Append, values) = prefix.computed_value().kind() {
                    return values[usize::from(operation == Operation::Last)].clone();
                }

                if let Some(bounds) = prefix.inline_captured_storage() {
                    return Self::new(K::StorageValue(operation, operands.clone())).captured_value(bounds.bounded());
                }
            }
            _ => {}
        }

        Self::new(K::StorageValue(operation, operands))
    }

    /// Owning links explicitly installed by checked storage transitions.
    /// Ordinary reference contents never become independently owned values.
    pub(crate) fn owning_storage_components(&self) -> Vec<&Self> {
        let mut pending = vec![self];
        let mut owned = Vec::new();

        while let Some(value) = pending.pop() {
            match value.kind() {
                K::Observation { contents, .. } | K::Borrowed(contents)
                | K::CapturedValue { value: contents, .. }
                | K::Present(contents) | K::Tagged(_, contents) => pending.push(contents),
                K::Constructed(_, fields) => {
                    for (field, value) in fields.iter() {
                        if *field == StorageProjection::OwnedTarget { owned.push(value); }
                        else { pending.push(value); }
                    }
                }
                _ => {}
            }
        }

        owned
    }
}
