use bray_bound_tree::{AsyncCleanupPhases, AsyncStorageCleanupRequirement, BoundExpressionId};
use bray_compiler_known::RepresentationRole;
use bray_ir::{MirBlockId, MirCleanupPhase, MirOperand, MirPlace, MirSourceAnchor};
use bray_symbols::TypeId;

use super::LoweringError;
use super::initialization::InitializationState;
use super::lowerer::Lowerer;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct InputTemporary {
    pub(super) place: MirPlace,
    source: MirSourceAnchor,
    pub(super) scope_depth: usize,
    catch_depth: usize,
    cleanup: Option<InputCleanup>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum InputCleanup {
    Value(AsyncCleanupPhases),
    Outgoing(bray_ir::MirOutgoingOwner),
}

impl InputTemporary {
    pub(super) const fn requires_cleanup(&self, phase: MirCleanupPhase) -> bool {
        let phases = match self.cleanup {
            None => return false,
            Some(InputCleanup::Value(phases)) => phases,
            Some(InputCleanup::Outgoing(_)) => AsyncCleanupPhases::Lifecycle,
        };

        match phase {
            MirCleanupPhase::TaskCancellation => phases.includes_cancellation(),
            MirCleanupPhase::LifecycleResolution => phases.includes_lifecycle(),
        }
    }
}
#[derive(Clone, Copy)]
pub(super) enum InputExit {
    Scope(usize),
    Catch(usize),
    All,
}

impl InputExit {
    pub(super) fn crosses(self, temporary: &InputTemporary) -> bool {
        match self {
            Self::Scope(depth) => temporary.scope_depth > depth,
            Self::Catch(depth) => temporary.catch_depth >= depth,
            Self::All => true,
        }
    }
}

impl Lowerer<'_> {
    pub(super) fn default_value_arguments<'input>(
        &mut self,
        expression: BoundExpressionId,
        block: MirBlockId,
        source: &MirSourceAnchor,
        inputs: impl IntoIterator<Item = (Option<u32>, &'input MirOperand)>,
        ordinal: u32,
    ) -> Result<Vec<MirOperand>, LoweringError> {
        let mut inputs = inputs
            .into_iter()
            .filter(|(input, _)| input.is_none_or(|input| input < ordinal))
            .collect::<Vec<_>>();

        inputs.sort_by_key(|(ordinal, _)| *ordinal);

        inputs
            .into_iter()
            .map(|(_, input)| {
                let MirOperand::Move(place) = input else {
                    panic!(
                        "lowering contract violation: MissingOperationResult {value:?}",
                        value = expression
                    );
                };

                let kind = bray_symbols::BorrowKind::Shared;

                let ty =
                    self.input
                        .semantic_values()
                        .intern_type(bray_symbols::TypeData::Borrow {
                            kind,
                            target: place.ty(),
                        })?;

                self.push_typed_value_operation(
                    expression,
                    block,
                    Self::retained_source(source),
                    bray_ir::MirOperationKind::Borrow {
                        kind,
                        place: Self::retained_place(place),
                    },
                    ty,
                )
            })
            .collect()
    }

    pub(super) fn materialize_owned_input(
        &mut self,
        expression: BoundExpressionId,
        block: MirBlockId,
        source: &MirSourceAnchor,
        value: MirOperand,
        ty: TypeId,
    ) -> Result<MirOperand, LoweringError> {
        let lowered =
            self.materialize_synthetic_temporary(block, Self::retained_source(source), value, ty)?;

        let Some(MirOperand::Move(place)) = lowered.value else {
            panic!(
                "lowering contract violation: MissingOperationResult {value:?}",
                value = expression
            );
        };

        let cleanup = self
            .input
            .cleanup_type(ty)
            .map(|shape| shape.cleanup())
            .unwrap_or_else(|| {
                panic!(
                    "lowering contract violation: MissingInputCleanup {value:?}",
                    value = expression
                )
            });

        let phases = match cleanup {
            AsyncStorageCleanupRequirement::None => None,
            AsyncStorageCleanupRequirement::Cleanup(phases) => Some(phases),
            AsyncStorageCleanupRequirement::Recovered(_) => {
                panic!(
                    "lowering contract violation: RecoveredBoundNode {value:?}",
                    value = expression
                );
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

        self.input_temporaries.push(InputTemporary {
            place: Self::retained_place(&place),
            source: Self::retained_source(source),
            scope_depth: self.active_scopes.len(),
            catch_depth: self.catch_targets.len(),
            cleanup: phases.map(InputCleanup::Value),
        });

        Ok(MirOperand::Move(place))
    }

    pub(super) fn register_scoped_outgoing(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        owner: bray_ir::MirOutgoingOwner,
    ) -> Result<MirPlace, LoweringError> {
        let boolean = self.representation_type(RepresentationRole::ScalarBool)?;
        let guard = self.new_initialization_guard(block, source, boolean, &[], true)?;

        self.input_temporaries.push(InputTemporary {
            place: Self::retained_place(&guard),
            source: Self::retained_source(source),
            scope_depth: self.active_scopes.len(),
            catch_depth: self.catch_targets.len(),
            cleanup: Some(InputCleanup::Outgoing(owner)),
        });

        Ok(guard)
    }

    pub(super) fn input_cleanup(&self, exit: InputExit) -> Vec<InputTemporary> {
        self.input_temporaries
            .iter()
            .filter(|temporary| temporary.cleanup.is_some() && exit.crosses(temporary))
            // Cleanup mutates the builder while retaining these shallow storage paths.
            .cloned()
            .collect()
    }

    pub(super) fn push_input_cleanup(
        &mut self,
        mut block: MirBlockId,
        phase: MirCleanupPhase,
        temporaries: &mut std::iter::Peekable<std::iter::Rev<std::slice::Iter<'_, InputTemporary>>>,
        scope_depth: usize,
    ) -> Result<MirBlockId, LoweringError> {
        while temporaries
            .peek()
            .is_some_and(|temporary| temporary.scope_depth >= scope_depth)
        {
            let Some(temporary) = temporaries.next() else {
                break;
            };

            block = self.push_input_cleanup_temporary(block, phase, temporary)?;
        }

        Ok(block)
    }

    pub(super) fn push_input_cleanup_temporary(
        &mut self,
        block: MirBlockId,
        phase: MirCleanupPhase,
        temporary: &InputTemporary,
    ) -> Result<MirBlockId, LoweringError> {
        if !temporary.requires_cleanup(phase) {
            return Ok(block);
        }

        if let Some(InputCleanup::Outgoing(owner)) = temporary.cleanup {
            return self.discharge_scoped_outgoing(
                block,
                &temporary.source,
                owner,
                &temporary.place,
            );
        }

        let guard = self
            .initialization_guards
            .get(&temporary.place.storage())
            .map(|state| Self::retained_place(&state.guard));

        self.push_guarded_cleanup(
            block,
            &temporary.source,
            phase,
            Self::retained_place(&temporary.place),
            guard,
            None,
            false,
            None,
        )
        .map(|(block, _)| block)
    }
}

#[cfg(test)]
mod tests {
    use super::{InputExit, InputTemporary};
    use bray_bound_tree::AsyncCleanupPhases;
    use bray_ir::{MirPlace, MirStorageId, MirUnitId};
    use bray_symbols::{SemanticValueStore, TypeData};

    #[test]
    fn crossed_boundaries_preserve_inputs_outside_an_inner_catch_or_scope() {
        let values = SemanticValueStore::try_new().unwrap();
        let ty = values.intern_type(TypeData::tuple([])).unwrap();

        let temporary = InputTemporary {
            place: MirPlace::new(MirStorageId::from_slot(MirUnitId::new(1), 0), [], ty),
            source: bray_ir::MirSourceAnchor::from(bray_testing::test_bound_unit(1).key().source()),
            scope_depth: 2,
            catch_depth: 1,
            cleanup: Some(super::InputCleanup::Value(AsyncCleanupPhases::Lifecycle)),
        };

        assert!(!InputExit::Scope(2).crosses(&temporary));
        assert!(InputExit::Scope(1).crosses(&temporary));
        assert!(!InputExit::Catch(2).crosses(&temporary));
        assert!(InputExit::Catch(1).crosses(&temporary));
        assert!(InputExit::All.crosses(&temporary));
    }
}
