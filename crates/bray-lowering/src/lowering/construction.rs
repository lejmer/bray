use bray_bound_tree::{AsyncCleanupPhases, AsyncStorageCleanupRequirement, BoundExpressionId};
use bray_compiler_known::RepresentationRole;
use bray_ir::{MirBlockId, MirCleanupPhase, MirOperand, MirPlace, MirSourceAnchor};
use bray_symbols::TypeId;

use super::LoweringError;
use super::initialization::InitializationState;
use super::lowerer::Lowerer;

#[derive(Clone)]
pub(super) struct ConstructionTemporary {
    pub(super) place: MirPlace,
    pub(super) scope_depth: usize,
    catch_depth: usize,
    phases: Option<AsyncCleanupPhases>,
}

#[derive(Clone, Copy)]
pub(super) enum ConstructionExit {
    Scope(usize),
    Catch(usize),
    All,
}

impl ConstructionExit {
    pub(super) fn crosses(self, temporary: &ConstructionTemporary) -> bool {
        match self {
            Self::Scope(depth) => temporary.scope_depth > depth,
            Self::Catch(depth) => temporary.catch_depth >= depth,
            Self::All => true,
        }
    }
}

impl Lowerer<'_> {
    pub(super) fn materialize_construction_input(
        &mut self,
        expression: BoundExpressionId,
        block: MirBlockId,
        source: &MirSourceAnchor,
        value: MirOperand,
        ty: TypeId,
        cleanup: Option<AsyncStorageCleanupRequirement>,
    ) -> Result<MirOperand, LoweringError> {
        let lowered =
            self.materialize_synthetic_temporary(block, Self::retained_source(source), value, ty)?;

        let Some(MirOperand::Move(place)) = lowered.value else {
            return Err(LoweringError::MissingOperationResult(expression));
        };

        let cleanup = cleanup
            .or_else(|| {
                self.input
                    .lowering_plans()
                    .cleanup_type(ty)
                    .map(|shape| shape.cleanup())
            })
            .ok_or(LoweringError::MissingCleanupExecution(place.storage()))?;

        let phases = match cleanup {
            AsyncStorageCleanupRequirement::None => None,
            AsyncStorageCleanupRequirement::Cleanup(phases) => Some(phases),
            AsyncStorageCleanupRequirement::Recovered(_) => {
                return Err(LoweringError::MissingCleanupExecution(place.storage()));
            }
        };

        if phases.is_some() {
            let boolean = self.representation_type(RepresentationRole::ScalarBool)?;
            let guard = self.new_initialization_guard(block, source, boolean, &[], true)?;

            self.initialization_guards.insert(
                place.storage(),
                InitializationState {
                    guard,
                    parts: Vec::new(),
                },
            );
        }

        self.construction_temporaries.push(ConstructionTemporary {
            place: Self::retained_place(&place),
            scope_depth: self.active_scopes.len(),
            catch_depth: self.catch_targets.len(),
            phases,
        });

        Ok(MirOperand::Move(place))
    }

    pub(super) fn construction_cleanup(
        &self,
        exit: ConstructionExit,
    ) -> Vec<ConstructionTemporary> {
        self.construction_temporaries
            .iter()
            .filter(|temporary| temporary.phases.is_some() && exit.crosses(temporary))
            // Cleanup mutates the builder while retaining these shallow storage paths.
            .cloned()
            .collect()
    }

    pub(super) fn push_construction_cleanup(
        &mut self,
        mut block: MirBlockId,
        source: &MirSourceAnchor,
        phase: MirCleanupPhase,
        temporaries: &mut std::iter::Peekable<
            std::iter::Rev<std::slice::Iter<'_, ConstructionTemporary>>,
        >,
        scope_depth: usize,
    ) -> Result<MirBlockId, LoweringError> {
        while temporaries
            .peek()
            .is_some_and(|temporary| temporary.scope_depth >= scope_depth)
        {
            let Some(temporary) = temporaries.next() else {
                break;
            };

            let Some(phases) = temporary.phases else {
                continue;
            };

            let required = match phase {
                MirCleanupPhase::TaskCancellation => phases.includes_cancellation(),
                MirCleanupPhase::LifecycleResolution => phases.includes_lifecycle(),
            };

            if !required {
                continue;
            }

            let guard = self
                .initialization_guards
                .get(&temporary.place.storage())
                .map(|state| Self::retained_place(&state.guard));

            self.destructor_remainder = false;

            block = self
                .push_guarded_cleanup(
                    block,
                    source,
                    phase,
                    Self::retained_place(&temporary.place),
                    guard,
                    None,
                    None,
                    false,
                )?
                .0;
        }

        Ok(block)
    }
}

#[cfg(test)]
mod tests {
    use super::{ConstructionExit, ConstructionTemporary};
    use bray_bound_tree::AsyncCleanupPhases;
    use bray_ir::{MirPlace, MirStorageId, MirUnitId};
    use bray_symbols::{SemanticValueStore, TypeData};

    #[test]
    fn crossed_boundaries_preserve_inputs_outside_an_inner_catch_or_scope() {
        let values = SemanticValueStore::try_new().unwrap();
        let ty = values.intern_type(TypeData::tuple([])).unwrap();

        let temporary = ConstructionTemporary {
            place: MirPlace::new(MirStorageId::from_slot(MirUnitId::new(1), 0), [], ty),
            scope_depth: 2,
            catch_depth: 1,
            phases: Some(AsyncCleanupPhases::Lifecycle),
        };

        assert!(!ConstructionExit::Scope(2).crosses(&temporary));
        assert!(ConstructionExit::Scope(1).crosses(&temporary));
        assert!(!ConstructionExit::Catch(2).crosses(&temporary));
        assert!(ConstructionExit::Catch(1).crosses(&temporary));
        assert!(ConstructionExit::All.crosses(&temporary));
    }
}
