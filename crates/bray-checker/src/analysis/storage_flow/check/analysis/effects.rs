use bray_bound_tree::{StorageIdentity, StorageInvalidation, StoragePath, StorageProjection};
use bray_symbols::{
    DependencyMutationKind, DependencyProjection, DependencyStorageEffect, DependencySubject,
    DependencySubjectRoot, SymbolOrdinal, TypeData,
};

use super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::model::{AnalysisCallPhase, AnalysisOperationKind};
use crate::analysis::storage_flow::model::StorageFlowState;
use bray_bound_tree::StorageAccessPurpose;

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(super) fn begin_transition(
        &self,
        state: &mut StorageFlowState,
        operation: AnalysisOperationKind,
        assignment: Option<&StoragePath>,
    ) -> Option<Vec<(StoragePath, DependencyMutationKind)>> {
        self.prepare_assignment_reference(state, operation, assignment);

        let AnalysisOperationKind::Call {
            invocation,
            phase: AnalysisCallPhase::Attempt,
        } = operation
        else {
            return None;
        };

        if invocation
            .expression()
            .is_some_and(|expression| self.memory.operation(expression).is_some())
        {
            return None;
        }

        self.prepare_call_references(state, invocation);

        self.call_effects(state, invocation)
    }

    pub(super) fn record_operation_effects(
        &mut self,
        kind: AnalysisOperationKind,
        invalidation: Option<&StorageInvalidation>,
        call_effects: Option<&[(StoragePath, DependencyMutationKind)]>,
    ) {
        if !self.publish {
            return;
        }

        if let Some(effects) = call_effects {
            for (path, kind) in effects {
                self.record_effects(
                    &bray_bound_tree::StorageInvalidation::Paths(vec![path.clone()]),
                    *kind,
                );
            }
        } else if let Some(effects) =
            invalidation.or_else(|| self.refinements.invalidation_at(kind.occurrence()))
        {
            let effect_kind = if matches!(kind, AnalysisOperationKind::Call { .. }) {
                bray_symbols::DependencyMutationKind::Retire
            } else if self.storage.node_plans(kind.node()).any(|plan| {
                matches!(
                    self.storage.effective_purpose(plan),
                    StorageAccessPurpose::Move | StorageAccessPurpose::ValueTransfer
                )
            }) {
                bray_symbols::DependencyMutationKind::Consume
            } else {
                bray_symbols::DependencyMutationKind::Write
            };

            self.record_effects(effects, effect_kind);
        }
    }

    pub(super) fn apply_call_effects(
        &self,
        state: &mut StorageFlowState,
        kind: AnalysisOperationKind,
        call_effects: Option<&[(StoragePath, DependencyMutationKind)]>,
        invalidation: Option<&StorageInvalidation>,
    ) {
        if matches!(
            kind,
            AnalysisOperationKind::Call {
                phase: AnalysisCallPhase::Attempt,
                ..
            }
        ) && kind
            .occurrence()
            .expression()
            .is_some_and(|expression| self.memory.operation(expression).is_none())
        {
            if let Some(effects) = call_effects {
                for (path, kind) in effects {
                    state.memory.invalidate(
                        &bray_bound_tree::StorageInvalidation::Paths(vec![path.clone()]),
                        self.storage,
                        *kind,
                    );
                }
            } else if let Some(effects) =
                invalidation.or_else(|| self.refinements.invalidation_at(kind.occurrence()))
            {
                state.memory.invalidate(
                    effects,
                    self.storage,
                    bray_symbols::DependencyMutationKind::Retire,
                );
            }
        }
    }

    fn call_effects(
        &self,
        state: &crate::analysis::storage_flow::model::StorageFlowState,
        occurrence: bray_bound_tree::BoundExecutionSite,
    ) -> Option<Vec<(StoragePath, DependencyMutationKind)>> {
        let contracts = self.dependencies.calls().get(&occurrence)?;
        let mut resolved = Vec::new();

        for effect in contracts.effects() {
            let (subject, indirect, projections, kind) = match effect {
                DependencyStorageEffect::Unknown => return None,
                DependencyStorageEffect::Direct { subject, kind } => {
                    (subject, false, &[][..], *kind)
                }
                DependencyStorageEffect::Indirect {
                    reference,
                    projections,
                    kind,
                } => (reference, true, projections.as_ref(), *kind),
            };

            let expression = occurrence.expression()?;

            let bray_bound_tree::SemanticSelection::Call(call) = self
                .execution
                .semantics
                .selections()
                .expression(expression)?
            else {
                return None;
            };

            let argument = crate::dependency::result_argument(call, subject.subject_root())?;

            let places = if indirect && subject.projections().is_empty() {
                self.expression_reference(state, argument)?
                    .located_storage(&state.memory)?
            } else {
                let place = project(
                    self.request,
                    self.argument_storage(argument)?,
                    subject.projections(),
                )?;

                if indirect {
                    state.memory.located_storage(&place)?
                } else {
                    vec![place]
                }
            };

            for place in places {
                resolved.push((project(self.request, place, projections)?, kind));
            }
        }

        Some(resolved)
    }

    pub(super) fn record_effects(
        &mut self,
        invalidation: &StorageInvalidation,
        kind: DependencyMutationKind,
    ) {
        if !self.publish || self.effects.contains(&DependencyStorageEffect::Unknown) {
            return;
        }

        let StorageInvalidation::Paths(paths) = invalidation else {
            self.effects.clear();
            self.effects.insert(DependencyStorageEffect::Unknown);
            return;
        };

        for path in paths {
            match self.formal_effect(path, kind) {
                Some(DependencyStorageEffect::Unknown) => {
                    self.effects.clear();
                    self.effects.insert(DependencyStorageEffect::Unknown);
                    return;
                }
                Some(effect) => {
                    self.effects.insert(effect);
                }
                None => {}
            }
        }
    }

    fn formal_effect(
        &self,
        path: &StoragePath,
        kind: DependencyMutationKind,
    ) -> Option<DependencyStorageEffect> {
        let identity = self
            .storage
            .identity(path.root)
            .expect("flow mutation retains its planned root");

        let borrowed = self.storage.identity_type(path.root).is_some_and(|ty| {
            matches!(
                self.request.semantic_values().type_data(ty).as_ref(),
                TypeData::Borrow { .. }
            )
        });

        if matches!(identity, StorageIdentity::Parameter(_))
            && path.projections.is_empty()
            && !borrowed
        {
            // A by-value parameter slot belongs to this invocation, not to its caller.
            return None;
        }

        let root = match identity {
            StorageIdentity::Parameter(parameter) => {
                DependencySubjectRoot::Parameter(SymbolOrdinal::new(
                    self.request
                        .symbols()
                        .callable_parameter(parameter)
                        .expect("planned parameter retains its declaration")
                        .ordinal(),
                ))
            }
            StorageIdentity::Receiver(_) => DependencySubjectRoot::Receiver,
            StorageIdentity::Static(_)
            | StorageIdentity::AnonymousParameter(_)
            | StorageIdentity::Error(_)
            | StorageIdentity::CustomIndexBorrow(_)
            | StorageIdentity::Alternative { .. } => return Some(DependencyStorageEffect::Unknown),
            _ => {
                // An unresolved local borrow may still reach a caller. Local value slots do not.
                return self
                    .storage
                    .identity_type(path.root)
                    .is_some_and(|ty| {
                        matches!(
                            self.request.semantic_values().type_data(ty).as_ref(),
                            TypeData::Borrow { .. }
                        )
                    })
                    .then_some(DependencyStorageEffect::Unknown);
            }
        };

        let projections = super::references::dependency_projections(&path.projections);

        let Some(projections) = projections else {
            return Some(DependencyStorageEffect::Unknown);
        };

        Some(if borrowed {
            DependencyStorageEffect::Indirect {
                reference: DependencySubject::root(root),
                projections: projections.into(),
                kind,
            }
        } else {
            DependencyStorageEffect::Direct {
                subject: DependencySubject::new(root, projections),
                kind,
            }
        })
    }
}

pub(super) fn project<C: CheckerRequestContext + ?Sized>(
    request: crate::CheckerUnitView<'_, C>,
    mut path: StoragePath,
    projections: &[DependencyProjection],
) -> Option<StoragePath> {
    let suffix = projections
        .iter()
        .map(|projection| match projection {
            DependencyProjection::ProductField(field) => {
                Some(StorageProjection::ProductField(*field))
            }
            DependencyProjection::TupleElement(index) => {
                Some(StorageProjection::TupleElement(*index))
            }
            DependencyProjection::NullableValue => Some(StorageProjection::NullableValue),
            DependencyProjection::OwnedTarget => Some(StorageProjection::OwnedTarget),
            DependencyProjection::UnionPayloadField(field) => {
                let field = request
                    .symbols()
                    .union_payload_field(*field)
                    .expect("checked field projection retains its declaration");

                Some(StorageProjection::ActiveUnionPayloadField {
                    variant: field.variant(),
                    field: field.id(),
                })
            }
            DependencyProjection::Element(_) => None,
        })
        .collect::<Option<Vec<_>>>()?;

    path.projections = path.projections.iter().copied().chain(suffix).collect();

    Some(path)
}
