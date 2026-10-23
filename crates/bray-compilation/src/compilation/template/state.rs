use bray_binder::BindingQueryContext;
use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundAssignmentExpression, BoundAssignmentOperator, BoundExpressionId, BoundReferenceTarget,
    CheckedTemplateNodeId, CheckedTemplateOperation, OperatorTarget, SelectedOperation,
    SemanticSelection, StorageBinding, StorageBindingTarget, StorageIdentityId,
};
use bray_symbols::TypeId;

use super::build::SourceTemplateBuilder;
use crate::fact::FactQueryError;

pub(super) enum StateChange {
    Introduced(StorageIdentityId),
    Assigned(StorageIdentityId, CheckedTemplateNodeId),
}

#[derive(Default)]
pub(super) struct StateUpdates {
    pub(super) outer: BTreeMap<StorageIdentityId, CheckedTemplateNodeId>,
    pub(super) local: BTreeMap<StorageIdentityId, CheckedTemplateNodeId>,
}

impl SourceTemplateBuilder<'_, '_, '_> {
    pub(super) fn materialize(&mut self, value: CheckedTemplateNodeId) -> CheckedTemplateNodeId {
        let ty =
            self.nodes[usize::try_from(value.raw()).expect("checked initializer fits host")].ty();

        let id = bray_bound_tree::CheckedTemplateTemporaryId::new(super::build::index(
            self.temporaries.len(),
        ));

        self.temporaries
            .push(bray_bound_tree::CheckedTemplateTemporary::new(value, ty));

        self.push_node(CheckedTemplateOperation::Temporary(id), ty)
    }

    pub(super) fn prepare_state_path(
        &mut self,
        path: &bray_bound_tree::StoragePath,
    ) -> Result<(CheckedTemplateNodeId, CheckedTemplateNodeId), FactQueryError> {
        self.state_root(path.root)?;

        for projection in path.projections.iter() {
            if let bray_bound_tree::StorageProjection::Element(index) = projection {
                self.expression(*index)?;
            }
        }

        // Selectors may themselves update a sibling. Read the current root after their evaluation.
        let base = self.state_values[&path.root];
        let component = self.project_path(base, &path.projections)?;

        Ok((base, component))
    }

    pub(super) fn reference_value(
        &mut self,
        target: BoundReferenceTarget,
    ) -> Option<CheckedTemplateNodeId> {
        let current = StorageBindingTarget::from_reference_target(target)
            .and_then(|target| self.storage.binding(target))
            .and_then(|binding| match binding {
                StorageBinding::Identity(root) => self.current_state(root),
                StorageBinding::Access(_) => None,
            });

        current.or_else(|| self.bindings.get(&target).copied())
    }

    pub(super) fn state_root(
        &mut self,
        root: StorageIdentityId,
    ) -> Result<CheckedTemplateNodeId, FactQueryError> {
        if let Some(value) = self.current_state(root) {
            if matches!(
                self.nodes[usize::try_from(value.raw()).expect("checked state fits host")]
                    .operation(),
                CheckedTemplateOperation::Input(_)
            ) {
                let value = self.materialize(value);

                self.state_values.insert(root, value);

                return Ok(value);
            }

            return Ok(value);
        }

        let reference = self
            .storage
            .binding_targets(StorageBinding::Identity(root))
            .find_map(StorageBindingTarget::reference_target);

        let value = if let Some(reference) = reference {
            if let Some(value) = self.bindings.get(&reference) {
                *value
            } else {
                let input = *self
                    .input_ids
                    .get(&reference)
                    .expect("checked value state retains its formal input");

                let ty = self.inputs
                    [usize::try_from(input.raw()).expect("checked input fits host")]
                .ty();

                self.push_node(CheckedTemplateOperation::Input(input), ty)
            }
        } else {
            match self
                .storage
                .identity(root)
                .expect("checked state root exists")
            {
                bray_bound_tree::StorageIdentity::Temporary(expression)
                | bray_bound_tree::StorageIdentity::Result(
                    bray_bound_tree::AnyBoundNodeId::Expression(expression),
                ) => self.place_expression(expression)?,
                identity => {
                    panic!("state value is established at its ownership birth: {identity:?}")
                }
            }
        };

        let value = self.materialize(value);

        self.introduce_state(root, value);

        Ok(value)
    }

    pub(super) fn project_path(
        &mut self,
        mut value: CheckedTemplateNodeId,
        projections: &[bray_bound_tree::StorageProjection],
    ) -> Result<CheckedTemplateNodeId, FactQueryError> {
        for projection in projections {
            let ty =
                self.nodes[usize::try_from(value.raw()).expect("checked owner fits host")].ty();

            let projected = bray_checker::projected_storage_type(self.checker, ty, *projection)?;

            assert!(
                !projected.diagnostics().has_errors(),
                "checked state projection retains valid declarations"
            );

            let ty = projected
                .value()
                .expect("checked inline state projection retains its component type");

            let operation = self.value_projection(value, *projection)?;

            value = self.push_node(operation, ty);
        }

        Ok(value)
    }

    pub(super) fn introduce_state(
        &mut self,
        root: StorageIdentityId,
        value: CheckedTemplateNodeId,
    ) {
        assert!(
            self.state_values.insert(root, value).is_none(),
            "source owner has one state birth"
        );

        self.state_changes.push(StateChange::Introduced(root));
    }

    pub(super) fn retained_updates(
        &self,
        checkpoint: usize,
    ) -> BTreeMap<StorageIdentityId, CheckedTemplateNodeId> {
        let introduced = self.state_changes[checkpoint..]
            .iter()
            .filter_map(|change| match change {
                StateChange::Introduced(root) => Some(*root),
                StateChange::Assigned(..) => None,
            })
            .collect::<std::collections::BTreeSet<_>>();

        self.state_changes[checkpoint..]
            .iter()
            .filter_map(|change| match change {
                StateChange::Assigned(root, _) if !introduced.contains(root) => {
                    Some((*root, self.state_values[root]))
                }
                _ => None,
            })
            .collect()
    }

    pub(super) fn assign_state(&mut self, target: StorageIdentityId, value: CheckedTemplateNodeId) {
        let previous = self
            .state_values
            .insert(target, value)
            .expect("a mutable template binding has its entry value");

        self.state_changes
            .push(StateChange::Assigned(target, previous));
    }

    /// Restores the enclosing state and distinguishes surviving owners from branch-local births.
    pub(super) fn take_state_updates(&mut self, checkpoint: usize) -> StateUpdates {
        let mut updates = StateUpdates::default();

        while self.state_changes.len() > checkpoint {
            match self
                .state_changes
                .pop()
                .expect("state checkpoint lies within its change log")
            {
                StateChange::Assigned(root, previous) => {
                    let value = self
                        .state_values
                        .insert(root, previous)
                        .expect("assigned owner retains its state");

                    updates.outer.entry(root).or_insert(value);
                }
                StateChange::Introduced(root) => {
                    let initial = self
                        .state_values
                        .remove(&root)
                        .expect("introduced owner retains its state");

                    let value = updates.outer.remove(&root).unwrap_or(initial);

                    updates.local.insert(root, value);
                }
            }
        }

        updates
    }

    /// Commits the result and surviving owner states through one guarded operation.
    pub(super) fn commit_choice(
        &mut self,
        condition: CheckedTemplateNodeId,
        when_true: CheckedTemplateNodeId,
        when_false: CheckedTemplateNodeId,
        ty: TypeId,
        true_updates: BTreeMap<StorageIdentityId, CheckedTemplateNodeId>,
        false_updates: BTreeMap<StorageIdentityId, CheckedTemplateNodeId>,
    ) -> CheckedTemplateNodeId {
        let roots = true_updates
            .keys()
            .chain(false_updates.keys())
            .copied()
            .collect::<std::collections::BTreeSet<_>>();

        let mut outputs = Vec::new();
        let mut committed = Vec::new();

        for root in roots {
            let baseline = self.state_values.get(&root).copied();
            let left = true_updates.get(&root).copied().or(baseline);
            let right = false_updates.get(&root).copied().or(baseline);

            if left == right {
                if let Some(value) = left {
                    self.commit_state(root, value);
                }
            } else {
                let value = left
                    .or(right)
                    .expect("changed owner exists on at least one branch");

                let ty = self.nodes
                    [usize::try_from(value.raw()).expect("checked output fits host")]
                .ty();

                committed.push((root, ty));
                outputs.push((left, right));
            }
        }

        if outputs.is_empty() {
            return self.select_value(condition, when_true, when_false, ty);
        }

        let operation = self.push_node(
            CheckedTemplateOperation::Conditional {
                condition,
                when_true,
                when_false,
                state_outputs: outputs.into(),
            },
            ty,
        );

        for (ordinal, (root, ty)) in committed.into_iter().enumerate() {
            let value = self.push_node(
                CheckedTemplateOperation::StateAfter {
                    operation,
                    ordinal: u32::try_from(ordinal).expect("state output fits its ordinal"),
                },
                ty,
            );

            self.commit_state(root, value);
        }

        operation
    }

    fn commit_state(&mut self, root: StorageIdentityId, value: CheckedTemplateNodeId) {
        if self.state_values.contains_key(&root) {
            self.assign_state(root, value);
        } else {
            self.introduce_state(root, value);
        }
    }

    pub(super) fn assignment(
        &mut self,
        expression: BoundExpressionId,
        assignment: &BoundAssignmentExpression,
        ty: TypeId,
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        let [_, source] = assignment.operands() else {
            panic!("checked assignment has its destination and source");
        };

        let target_path = self
            .flow
            .assignment_target(expression)
            .expect("admitted assignment retains its checked target");

        assert_ne!(
            target_path.mutation_scope(self.storage, self.context.semantic_values()),
            bray_bound_tree::StorageMutationScope::Unbounded,
            "admitted assignment stays within its observation frame"
        );

        let path = target_path
            .inline_state_path()
            .expect("admitted template assignment has its exact inline state");

        let target = path.root;

        // Capture the actual destination, including indices, before evaluating the RHS.
        let (base, component) = self.prepare_state_path(&path)?;

        let previous = self.snapshot(component);
        let source = self.expression(*source)?;

        let value = if assignment.operator() == BoundAssignmentOperator::Assign {
            source
        } else {
            let Some(SemanticSelection::Operation(SelectedOperation::CompoundAssignment(selected))) =
                self.selections.expression(expression)
            else {
                panic!("checked compound assignment retains its exact operator");
            };

            let OperatorTarget::BuiltIn(operator) = selected.target() else {
                panic!("admitted compound assignment retains its scalar operator");
            };

            self.push_node(
                CheckedTemplateOperation::MachineBinary {
                    operation: super::build::binary_operator(operator)
                        .expect("admitted compound assignment has a binary operation"),
                    left: previous,
                    right: source,
                },
                selected.value_type(),
            )
        };

        let target_type = self.nodes
            [usize::try_from(previous.raw()).expect("checked assignment node fits host")]
        .ty();

        let value = self.push_node(
            CheckedTemplateOperation::Convert {
                value,
                target: target_type,
            },
            target_type,
        );

        let current = self.state_values[&target];

        let root_type =
            self.nodes[usize::try_from(current.raw()).expect("current root fits host")].ty();

        let value = self.push_node(
            CheckedTemplateOperation::Update {
                subject: current,
                base,
                component,
                value,
            },
            root_type,
        );

        self.assign_state(target, value);

        let unit = self.unit_node(ty);

        Ok(CheckedTemplateOperation::Sequence {
            before: value,
            value: unit,
        })
    }
}
