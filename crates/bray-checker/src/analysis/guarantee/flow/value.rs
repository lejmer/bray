use super::model::ExecutionFlowDomain;

use bray_bound_tree::{AnyBoundNodeId, BoundExpressionId};

use crate::CheckerRequestContext;
use crate::execution_guarantees::{ExecutionCondition, expression_condition};

use super::super::state::ExecutionState;

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(in crate::analysis) fn value(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
    ) -> ExecutionCondition {
        let value = expression_condition(
            self.request.unit(),
            self.semantics,
            self.request.semantic_values(),
            self.literals,
            self.pattern_conditions,
            &state.current,
            &state.expressions,
            expression,
            &mut { crate::ExecutionCondition::WORK_LIMIT },
            self.request
                .trusted_contracts()
                .map(|input| &input.predicates),
        );

        if value != ExecutionCondition::Unknown {
            return value;
        }

        self.aggregate_snapshot(state, expression).unwrap_or(value)
    }

    pub(in crate::analysis::guarantee) fn borrowed_value(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
    ) -> ExecutionCondition {
        // Borrow checking keeps the evaluated access path valid through this call.
        // Completion reads its current contents, without the cached entry snapshot.
        expression_condition(
            self.request.unit(),
            self.semantics,
            self.request.semantic_values(),
            self.literals,
            self.pattern_conditions,
            &state.current,
            &Default::default(),
            expression,
            &mut { ExecutionCondition::WORK_LIMIT },
            self.request
                .trusted_contracts()
                .map(|input| &input.predicates),
        )
    }

    fn aggregate_snapshot(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
    ) -> Option<ExecutionCondition> {
        let ty = self.semantics.types().expression(expression)?.ty();

        if matches!(
            self.request.semantic_values().type_data(ty).as_ref(),
            bray_symbols::TypeData::Borrow { .. }
        ) {
            return None;
        }

        let place = crate::execution_guarantees::expression_place(
            self.request.unit(),
            self.semantics,
            self.request.semantic_values(),
            expression,
        )?;

        let fields = state
            .current
            .range(place.clone()..)
            .take_while(|(observed, _)| place.contains(observed))
            .filter_map(|(observed, value)| {
                let [projection] = &observed.projections[place.projections.len()..] else {
                    return None;
                };

                let projection = match projection {
                    bray_symbols::ConstantProjectionKind::ProductField(field) => {
                        bray_bound_tree::StorageProjection::ProductField(*field)
                    }
                    bray_symbols::ConstantProjectionKind::TupleElement(index) => {
                        bray_bound_tree::StorageProjection::TupleElement(*index)
                    }
                    _ => return None,
                };

                Some((projection, value.clone()))
            })
            .collect::<Vec<_>>();

        // Mutation retires the old whole-value identity. The new snapshot carries
        // only observed component values, never the retired owner's witnesses.
        (!fields.is_empty()).then(|| {
            ExecutionCondition::Constructed(
                std::sync::Arc::new(ExecutionCondition::Unknown),
                fields.into(),
            )
        })
    }

    pub(in crate::analysis::guarantee) fn invocation_value(
        &self,
        state: &ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
    ) -> ExecutionCondition {
        match invocation {
            bray_bound_tree::BoundExecutionSite::Node(
                bray_bound_tree::AnyBoundNodeId::Expression(expression),
            ) => self.value(state, expression),
            bray_bound_tree::BoundExecutionSite::ScopedEnter(expression) => {
                ExecutionCondition::ScopedCapability(expression)
            }
            bray_bound_tree::BoundExecutionSite::ScopedExit(_) => ExecutionCondition::Unknown,
            bray_bound_tree::BoundExecutionSite::Cleanup(node)
            | bray_bound_tree::BoundExecutionSite::Node(node) => {
                panic!("invocation value requires an expression owner, got {node:?}")
            }
        }
    }

    pub(in crate::analysis::guarantee) fn unsigned_value(
        &self,
        value: &ExecutionCondition,
    ) -> bool {
        let ty = match value {
            ExecutionCondition::Input(place) | ExecutionCondition::Joined(_, _, place) => {
                self.observed_types.get(place).copied().flatten()
            }
            ExecutionCondition::Expression(expression) => self
                .semantics
                .types()
                .expression(*expression)
                .map(|entry| entry.ty()),
            ExecutionCondition::Literal(value) => Some(value.ty()),
            _ => None,
        };

        ty.and_then(|ty| crate::representation::type_representation(self.request, ty))
            .and_then(bray_compiler_known::RepresentationRole::integer_representation)
            .is_some_and(|representation| {
                matches!(
                    representation,
                    bray_compiler_known::IntegerRepresentation::Unsigned(_)
                        | bray_compiler_known::IntegerRepresentation::TargetUnsigned
                )
            })
    }

    pub(in crate::analysis::guarantee) fn storage_identity_value(
        &self,
        state: &ExecutionState,
        identity: bray_bound_tree::StorageIdentityId,
    ) -> ExecutionCondition {
        if let Some(place) = crate::ExecutionPlace::storage_identity(self.storage, identity) {
            return place
                .value_in(&state.current)
                .unwrap_or(ExecutionCondition::Input(place));
        }

        match self
            .storage
            .identity(identity)
            .expect("checked storage retains its identity")
        {
            bray_bound_tree::StorageIdentity::Temporary(expression)
            | bray_bound_tree::StorageIdentity::Allocation(expression)
            | bray_bound_tree::StorageIdentity::Result(AnyBoundNodeId::Expression(expression)) => {
                self.value(state, expression)
            }
            bray_bound_tree::StorageIdentity::ScopedCapability { expression, .. } => {
                ExecutionCondition::ScopedCapability(expression)
            }
            _ => ExecutionCondition::Unknown,
        }
    }

    pub(in crate::analysis::guarantee) fn storage_path_value(
        &self,
        state: &ExecutionState,
        path: &bray_bound_tree::StoragePath,
    ) -> ExecutionCondition {
        if let Some(value) = crate::ExecutionPlace::storage_path(self.storage, path)
            .and_then(|place| place.value_in(&state.current))
        {
            return value;
        }

        path.projections.iter().fold(
            self.storage_identity_value(state, path.root),
            |value, projection| value.project(*projection),
        )
    }

    pub(in crate::analysis::guarantee) fn storage_value(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
        access: bray_bound_tree::StorageAccessId,
    ) -> ExecutionCondition {
        let (mut value, scoped) = match self
            .storage
            .root_identity(access)
            .and_then(|identity| self.storage.identity(identity))
        {
            Some(bray_bound_tree::StorageIdentity::ScopedCapability { expression, .. }) => {
                (ExecutionCondition::ScopedCapability(expression), true)
            }
            _ => (self.value(state, expression), false),
        };

        let source = self
            .storage
            .expression_plans(expression)
            .filter(|source| {
                source.node() == AnyBoundNodeId::Expression(expression)
                    && self.storage.access_contains(source.access(), access)
            })
            .max_by_key(|source| {
                self.storage
                    .resolved_projections(source.access())
                    .map_or(0, <[_]>::len)
            });

        // A scoped root names the whole retained capability, even when this occurrence
        // observes one of its fields. Ordinary expression values already include their source path.
        let source_path = if scoped {
            &[][..]
        } else {
            source
                .and_then(|source| self.storage.resolved_projections(source.access()))
                .unwrap_or(&[])
        };

        let path = self
            .storage
            .resolved_projections(access)
            .and_then(|path| path.strip_prefix(source_path));

        if let Some(path) = path {
            for projection in path {
                value = value.project(*projection);
            }
        } else {
            value = ExecutionCondition::Unknown;
        }

        value
    }
}
