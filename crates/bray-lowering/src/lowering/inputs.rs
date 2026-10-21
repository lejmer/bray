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
    Array {
        phases: Option<AsyncCleanupPhases>,
        element: TypeId,
        first: MirOperand,
        end: MirOperand,
    },
    ScopeAdmission(BoundExpressionId),
    ScopedUse(BoundExpressionId),
}

impl InputTemporary {
    pub(super) const fn requires_cleanup(&self, phase: MirCleanupPhase) -> bool {
        let Some(phases) = self.action.phases() else {
            return false;
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
    pub(super) fn borrow_staged_inputs<'input>(
        &mut self,
        expression: BoundExpressionId,
        block: MirBlockId,
        source: &MirSourceAnchor,
        receiver: Option<bray_symbols::ReceiverMode>,
        inputs: impl IntoIterator<Item = (Option<u32>, &'input MirOperand)>,
    ) -> Result<Vec<MirOperand>, LoweringError> {
        let mut inputs = inputs.into_iter().collect::<Vec<_>>();

        inputs.sort_by_key(|(ordinal, _)| *ordinal);

        inputs
            .into_iter()
            .map(|(ordinal, input)| {
                let MirOperand::Move(place) = input else {
                    panic!(
                        "lowering contract violation: MissingOperationResult {value:?}",
                        value = expression
                    );
                };

                let mut place = Self::retained_place(place);

                if ordinal.is_none()
                    && matches!(
                        receiver,
                        Some(
                            bray_symbols::ReceiverMode::Shared
                                | bray_symbols::ReceiverMode::Mutable
                        )
                    )
                {
                    let data = self.input.semantic_values().type_data(place.ty());

                    let bray_symbols::TypeData::Borrow { target, .. } = data.as_ref() else {
                        panic!("checked receiver input retains its selected borrow");
                    };

                    place = place.project(bray_ir::MirProjectionKind::Dereference, *target);
                }

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
                    bray_ir::MirOperationKind::Borrow { kind, place },
                    ty,
                )
            })
            .collect()
    }

    pub(super) fn materialize_call_input(
        &mut self,
        input: bray_bound_tree::BoundExecutionSite,
        block: MirBlockId,
        source: &MirSourceAnchor,
        value: MirOperand,
        ty: TypeId,
    ) -> Result<MirOperand, LoweringError> {
        let bray_bound_tree::BoundExecutionSite::CallInput { expression, ordinal } = input else {
            panic!("selected input materialization has its exact occurrence");
        };
        let Some(access) = self.input.storage_plan().invocation_input(expression, ordinal) else {
            return self.materialize_owned_input(block, source, value, ty);
        };
        let identity = self.input.storage_plan().root_identity(access)
            .expect("staged input owns a storage root");
        let origin = self.input.unit().view().expression(expression)
            .expect("selected input retains its source owner").origin();
        let place = self.place_for_identity(identity, ty, origin)?;
        self.push_operation(block, Self::retained_source(source), bray_ir::MirOperationKind::Store {
            kind: bray_ir::MirStoreKind::Initialize,
            destination: Self::retained_place(&place),
            value,
        }, None)?;
        let phases = self.input_cleanup_phases(ty);
        self.track_input(block, source, Self::retained_place(&place), InputCleanup::Value(phases))?;
        self.initialized_temporaries.insert(identity);
        Ok(MirOperand::Move(place))
    }

    pub(super) fn input_cleanup_owns(
        &self,
        access: bray_bound_tree::StorageAccessId,
        phase: MirCleanupPhase,
        temporaries: &[InputTemporary],
    ) -> bool {
        let storage = self.input.storage_plan();
        let Some(identity) = storage.root_identity(access).filter(|identity|
            matches!(storage.identity(*identity), Some(bray_bound_tree::StorageIdentity::InvocationInput { .. })))
        else {
            return false;
        };
        let Some(storage) = self.storages.get(&identity) else {
            return false;
        };
        temporaries.iter().any(|temporary|
            temporary.requires_cleanup(phase) && temporary.place.storage() == *storage)
    }

    pub(super) fn materialize_owned_input(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        value: MirOperand,
        ty: TypeId,
    ) -> Result<MirOperand, LoweringError> {
        let place =
            self.materialize_synthetic_place(block, Self::retained_source(source), value, ty)?;

        let phases = self.input_cleanup_phases(ty);

        self.track_input(
            block,
            source,
            Self::retained_place(&place),
            InputCleanup::Value(phases),
        )?;

        Ok(MirOperand::Move(place))
    }

    pub(super) fn track_array_input(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        place: MirPlace,
        element: TypeId,
        range: [MirOperand; 2],
    ) -> Result<(), LoweringError> {
        let [first, end] = range;

        let phases = self.input_cleanup_phases(place.ty());

        self.track_input(
            block,
            source,
            place,
            InputCleanup::Array {
                phases,
                element,
                first,
                end,
            },
        )
    }

    fn track_input(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        place: MirPlace,
        action: InputCleanup,
    ) -> Result<(), LoweringError> {
        // A partial array is owned exactly by its range. It has no whole-value
        // initialization flag. Ordinary inputs need a guard because a call can
        // consume their complete value before a later input fails.
        if matches!(action, InputCleanup::Value(Some(_))) && self.initialization_guards.contains_key(&place.storage()) {
            self.set_storage_initialized(block, source, &place, true)?;
        } else if matches!(action, InputCleanup::Value(Some(_))) {
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
            place,
            source: Self::retained_source(source),
            scope_depth: self.active_scopes.len(),
            catch_depth: self.catch_targets.len(),
            action,
        });

        Ok(())
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
            .unwrap_or_else(|| {
                panic!(
                    "checked temporary {ty:?} ({:?}) retains its cleanup shape in {:?}",
                    self.input.semantic_values().type_data(ty),
                    self.input.unit().key()
                )
            })
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
                    InputCleanup::Value(_) | InputCleanup::Array { .. } => {
                        temporary.action.phases().is_some()
                    }
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
            InputCleanup::Value(_) | InputCleanup::Array { .. } => {}
        }

        let guard = self
            .initialization_guards
            .get(&temporary.place.storage())
            .map(|state| Self::retained_place(&state.guard));

        if let InputCleanup::Array {
            element,
            ref first,
            ref end,
            ..
        } = temporary.action
        {
            return self.push_array_input_cleanup(
                block,
                phase,
                temporary,
                guard,
                element,
                [first, end],
            );
        }

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
    fn push_array_input_cleanup(
        &mut self,
        block: MirBlockId,
        phase: MirCleanupPhase,
        temporary: &InputTemporary,
        guard: Option<MirPlace>,
        element: TypeId,
        range: [&MirOperand; 2],
    ) -> Result<MirBlockId, LoweringError> {
        let source = &temporary.source;

        let (block, _) = self.guarded_cleanup_region(
            block,
            source,
            Self::retained_place(&temporary.place),
            guard,
            None,
            |lowerer, block, place, _| {
                let integer = lowerer.builder.operand_type(range[1]);
                let boolean = lowerer.representation_type(RepresentationRole::ScalarBool)?;

                let one =
                    crate::operand::integer_constant(lowerer.input.semantic_values(), integer, 1)?;

                let cleanup = crate::cleanup_loop::ReverseCleanupLoop::new(
                    &mut lowerer.builder,
                    block,
                    source,
                    Self::retained_operand(range[1]),
                    boolean,
                    [Self::retained_operand(range[0]), one],
                    None,
                )?;

                let projected = bray_ir::MirPlace::new(
                    place.storage(),
                    place
                        .projections()
                        .iter()
                        .cloned()
                        .chain([bray_ir::MirProjection::new(
                            bray_ir::MirProjectionKind::Index(MirOperand::Copy(
                                Self::retained_place(&cleanup.counter),
                            )),
                            place.ty(),
                            element,
                        )]),
                    element,
                );

                let (completed, _) = lowerer.push_guarded_cleanup(
                    cleanup.body,
                    source,
                    phase,
                    projected,
                    None,
                    None,
                    false,
                    None,
                )?;

                cleanup.close(&mut lowerer.builder, completed, source, None)?;

                Ok((cleanup.continuation, None))
            },
        )?;

        Ok(block)
    }
}

impl InputCleanup {
    const fn phases(&self) -> Option<AsyncCleanupPhases> {
        match self {
            Self::Value(phases) | Self::Array { phases, .. } => *phases,
            Self::ScopeAdmission(_) | Self::ScopedUse(_) => Some(AsyncCleanupPhases::Lifecycle),
        }
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
