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
    action: InputCleanup,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum InputCleanup {
    Value(Option<AsyncCleanupPhases>),
    ScopeAdmission(BoundExpressionId),
    ScopedUse(BoundExpressionId),
}

impl InputTemporary {
    pub(super) const fn requires_cleanup(&self, phase: MirCleanupPhase) -> bool {
        let phases = match self.action {
            InputCleanup::Value(Some(phases)) => phases,
            InputCleanup::Value(None) => return false,
            InputCleanup::ScopeAdmission(_) | InputCleanup::ScopedUse(_) => {
                return matches!(phase, MirCleanupPhase::LifecycleResolution);
            }
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

        let phases = self.input_cleanup_phases(ty);

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
            action: InputCleanup::Value(phases),
        });

        Ok(MirOperand::Move(place))
    }

    pub(super) fn track_scope_admission(
        &mut self,
        expression: BoundExpressionId,
        source: &MirSourceAnchor,
        admitted: MirPlace,
    ) {
        self.input_temporaries.push(InputTemporary {
            place: admitted,
            source: Self::retained_source(source),
            scope_depth: self.active_scopes.len() + 1,
            catch_depth: self.catch_targets.len(),
            action: InputCleanup::ScopeAdmission(expression),
        });
    }

    pub(super) fn mark_scope_entered(&mut self, expression: BoundExpressionId) {
        let temporary = self
            .input_temporaries
            .iter_mut()
            .rev()
            .find(|temporary| temporary.action == InputCleanup::ScopeAdmission(expression))
            .expect("successful entry owns its admitted exit duty");

        temporary.action = InputCleanup::ScopedUse(expression);
    }

    pub(super) fn input_cleanup_phases(&self, ty: TypeId) -> Option<AsyncCleanupPhases> {
        let cleanup = self
            .input
            .cleanup_type(ty)
            .expect("checked temporary type retains its cleanup shape")
            .cleanup();

        match cleanup {
            AsyncStorageCleanupRequirement::None => None,
            AsyncStorageCleanupRequirement::Cleanup(phases) => Some(phases),
            AsyncStorageCleanupRequirement::Recovered(_) => {
                panic!("checked temporary type {ty:?} has unrecovered cleanup")
            }
        }
    }

    pub(super) fn input_cleanup(&self, exit: InputExit) -> Vec<InputTemporary> {
        self.input_temporaries
            .iter()
            .filter(|temporary| {
                if !exit.crosses(temporary) {
                    return false;
                }

                match temporary.action {
                    InputCleanup::Value(phases) => phases.is_some(),
                    InputCleanup::ScopeAdmission(_) => true,
                    InputCleanup::ScopedUse(expression) => {
                        let bray_bound_tree::BoundExpression::Structured(bound) = self
                            .input
                            .unit()
                            .tree()
                            .expression(expression)
                            .expect("scope temporary has its bound expression")
                        else {
                            panic!("scoped cleanup retains its with expression");
                        };

                        // The checked body plan owns exit once the body scope is active.
                        !self.active_scopes.contains(&bound.blocks()[0])
                    }
                }
            })
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

        match temporary.action {
            InputCleanup::ScopeAdmission(expression) => {
                return self
                    .discharge_scoped_exit(block, &temporary.source, expression, None)
                    .map(|(block, _)| block);
            }
            InputCleanup::ScopedUse(expression) => {
                let previous = self
                    .cleanup_occurrence
                    .replace(bray_bound_tree::BoundExecutionSite::ScopedExit(expression));

                let result = self.push_scope_exit(block, &temporary.source, expression, None, None);

                self.cleanup_occurrence = previous;

                return result;
            }
            InputCleanup::Value(_) => {}
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
    use super::{InputCleanup, InputExit, InputTemporary};
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
            action: InputCleanup::Value(Some(AsyncCleanupPhases::Lifecycle)),
        };

        assert!(!InputExit::Scope(2).crosses(&temporary));
        assert!(InputExit::Scope(1).crosses(&temporary));
        assert!(!InputExit::Catch(2).crosses(&temporary));
        assert!(InputExit::Catch(1).crosses(&temporary));
        assert!(InputExit::All.crosses(&temporary));
    }
}
