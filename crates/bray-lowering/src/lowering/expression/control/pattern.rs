use bray_bound_tree::{
    BoundPatternEntryKind, BoundPatternId, BoundPatternKind, PatternOperation, PatternPredicate,
    PatternProjection, StorageBinding, StorageBindingTarget,
};
use bray_ir::{
    MirBlockId, MirBlockKind, MirEdge, MirOperand, MirOperationKind, MirPlace, MirStoreKind,
    MirTerminatorKind,
};
use bray_symbols::{BorrowKind, LocalBindingSymbolId};

use super::super::super::LoweringError;
use super::super::super::lowerer::Lowerer;
use super::super::super::projection::projected_pattern_place;

impl Lowerer<'_> {
    pub(super) fn lower_pattern_branch(
        &mut self,
        pattern: BoundPatternId,
        subject: MirOperand,
        current: MirBlockId,
        matched: MirBlockId,
        unmatched: MirBlockId,
    ) -> Result<(), LoweringError> {
        let pattern_node = self.pattern(pattern);
        let subject = self.pattern_test_subject(pattern, subject)?;

        if pattern_node.kind() == BoundPatternKind::Alternative {
            return self.lower_alternative_pattern(pattern, subject, current, matched, unmatched);
        }

        let source = self.source(pattern_node.origin());

        let bindings = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        self.lower_pattern_tests(
            pattern,
            Self::retained_operand(&subject),
            current,
            bindings,
            unmatched,
        )?;

        let completion = self.lower_pattern_bindings(pattern, subject, bindings)?;

        self.set_terminator(
            completion,
            source,
            MirTerminatorKind::Goto(MirEdge::new(matched, [])),
        )?;

        Ok(())
    }

    fn pattern_test_subject(
        &self,
        pattern: BoundPatternId,
        subject: MirOperand,
    ) -> Result<MirOperand, LoweringError> {
        let check = self.input.patterns().pattern(pattern).unwrap_or_else(|| {
            panic!(
                "lowering contract violation: UnsupportedPattern {value:?}",
                value = pattern
            )
        });

        if !matches!(
            check.test(),
            Some(PatternPredicate::NullableAbsent | PatternPredicate::NullablePresent)
        ) {
            return Ok(subject);
        }

        let place = match &subject {
            MirOperand::Copy(place) | MirOperand::Move(place) => place,
            MirOperand::Value(_) | MirOperand::Immediate { .. } | MirOperand::Constant { .. } => {
                return Ok(subject);
            }
        };

        let Some(projection) = place.projections().last() else {
            return Ok(subject);
        };

        if !matches!(projection.kind(), bray_ir::MirProjectionKind::NullableValue)
            || projection.source_type() != check.input_type()
        {
            return Ok(subject);
        }

        let source = MirPlace::new(
            place.storage(),
            place.projections()[..place.projections().len() - 1]
                .iter()
                .cloned(),
            projection.source_type(),
        );

        Ok(MirOperand::Copy(source))
    }

    pub(in crate::lowering) fn lower_pattern_bindings(
        &mut self,
        pattern: BoundPatternId,
        subject: MirOperand,
        current: MirBlockId,
    ) -> Result<MirBlockId, LoweringError> {
        let pattern_node = self.pattern(pattern);

        let operation = if !self.guard_bindings.is_empty() {
            PatternOperation::Observe
        } else {
            self.pattern_operation(pattern)
        };

        let subject = self.project_pattern_subject(pattern, subject, current, operation)?;

        if pattern_node.kind() == BoundPatternKind::Discard
            && operation == PatternOperation::Consume
        {
            self.store_discarded_pattern(pattern, subject, pattern_node.origin(), current)?;

            return Ok(current);
        }

        if self.pattern_introduces_direct_bindings(pattern) {
            for binding in pattern_node.bindings() {
                self.store_pattern_binding(
                    pattern,
                    *binding,
                    Self::retained_operand(&subject),
                    pattern_node.origin(),
                    current,
                    false,
                )?;
            }
        }

        for entry in pattern_node.entries() {
            if let Some(binding) = entry.binding() {
                self.store_pattern_binding(
                    pattern,
                    binding,
                    Self::retained_operand(&subject),
                    pattern_node.origin(),
                    current,
                    true,
                )?;
            }
        }

        for child in pattern_node.children() {
            self.lower_pattern_bindings(*child, Self::retained_operand(&subject), current)?;
        }

        Ok(current)
    }

    fn store_discarded_pattern(
        &mut self,
        pattern: BoundPatternId,
        value: MirOperand,
        origin: bray_bound_tree::BoundNodeOrigin,
        current: MirBlockId,
    ) -> Result<(), LoweringError> {
        let checked = self.input.patterns().pattern(pattern).unwrap_or_else(|| {
            panic!(
                "lowering contract violation: UnsupportedPattern {value:?}",
                value = pattern
            )
        });

        let StorageBinding::Identity(identity) = self
            .input
            .storage_plan()
            .binding(StorageBindingTarget::PatternDiscard(pattern))
            .unwrap_or_else(|| {
                panic!(
                    "lowering contract violation: UnsupportedPattern {value:?}",
                    value = pattern
                )
            })
        else {
            panic!(
                "lowering contract violation: UnsupportedPattern {value:?}",
                value = pattern
            );
        };

        let destination = self.place_for_identity(identity, checked.input_type(), origin)?;

        self.push_operation(
            current,
            self.source(origin),
            MirOperationKind::Store {
                kind: MirStoreKind::Initialize,
                destination,
                value,
            },
            None,
        )?;

        Ok(())
    }

    fn lower_alternative_pattern(
        &mut self,
        pattern: BoundPatternId,
        subject: MirOperand,
        current: MirBlockId,
        matched: MirBlockId,
        unmatched: MirBlockId,
    ) -> Result<(), LoweringError> {
        let pattern_node = self.pattern(pattern);
        let source = self.source(pattern_node.origin());

        let subject =
            self.project_pattern_subject(pattern, subject, current, PatternOperation::Observe)?;

        let mut candidate = current;

        for (index, child) in pattern_node.children().iter().copied().enumerate() {
            let selected = self
                .builder
                .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

            let next = if index + 1 == pattern_node.children().len() {
                unmatched
            } else {
                self.builder
                    .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?
            };

            self.lower_pattern_tests(
                child,
                Self::retained_operand(&subject),
                candidate,
                selected,
                next,
            )?;

            let completion =
                self.lower_pattern_bindings(child, Self::retained_operand(&subject), selected)?;

            self.store_direct_pattern_bindings(
                pattern,
                &pattern_node,
                Self::retained_operand(&subject),
                completion,
            )?;

            self.set_terminator(
                completion,
                Self::retained_source(&source),
                MirTerminatorKind::Goto(MirEdge::new(matched, [])),
            )?;

            candidate = next;
        }

        if pattern_node.children().is_empty() {
            self.set_terminator(
                current,
                source,
                MirTerminatorKind::Goto(MirEdge::new(unmatched, [])),
            )?;
        }

        Ok(())
    }

    fn lower_pattern_tests(
        &mut self,
        pattern: BoundPatternId,
        subject: MirOperand,
        current: MirBlockId,
        matched: MirBlockId,
        unmatched: MirBlockId,
    ) -> Result<(), LoweringError> {
        let pattern_node = self.pattern(pattern);
        let source = self.source(pattern_node.origin());

        if pattern_node.kind() == BoundPatternKind::Alternative {
            let subject =
                self.project_pattern_subject(pattern, subject, current, PatternOperation::Observe)?;

            let mut candidate = current;

            for (index, child) in pattern_node.children().iter().copied().enumerate() {
                let next = if index + 1 == pattern_node.children().len() {
                    unmatched
                } else {
                    self.builder
                        .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?
                };

                self.lower_pattern_tests(
                    child,
                    Self::retained_operand(&subject),
                    candidate,
                    matched,
                    next,
                )?;

                candidate = next;
            }

            if pattern_node.children().is_empty() {
                self.set_terminator(
                    current,
                    source,
                    MirTerminatorKind::Goto(MirEdge::new(unmatched, [])),
                )?;
            }

            return Ok(());
        }

        let check = self.input.patterns().pattern(pattern).unwrap_or_else(|| {
            panic!(
                "lowering contract violation: UnsupportedPattern {value:?}",
                value = pattern
            )
        });

        let projects_after_test = matches!(
            (check.test(), check.projection()),
            (
                Some(PatternPredicate::NullableAbsent | PatternPredicate::NullablePresent),
                Some(PatternProjection::NullableValue)
            )
        );

        let subject = if projects_after_test {
            subject
        } else {
            self.project_pattern_subject(pattern, subject, current, PatternOperation::Observe)?
        };

        let children_entry = match (check.test(), pattern_node.children().is_empty()) {
            (Some(_), true) => matched,
            (Some(_), false) => self
                .builder
                .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?,
            (None, _) => current,
        };

        if let Some(predicate) = check.test() {
            self.set_terminator(
                current,
                Self::retained_source(&source),
                MirTerminatorKind::PatternBranch {
                    subject: Self::retained_operand(&subject),
                    predicate: predicate.into(),
                    matched: MirEdge::new(children_entry, []),
                    unmatched: MirEdge::new(unmatched, []),
                },
            )?;
        }

        let subject = if projects_after_test && !pattern_node.children().is_empty() {
            self.project_pattern_subject(
                pattern,
                subject,
                children_entry,
                PatternOperation::Observe,
            )?
        } else {
            subject
        };

        let mut candidate = children_entry;

        for (index, child) in pattern_node.children().iter().copied().enumerate() {
            let next = if index + 1 == pattern_node.children().len() {
                matched
            } else {
                self.builder
                    .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?
            };

            self.lower_pattern_tests(
                child,
                Self::retained_operand(&subject),
                candidate,
                next,
                unmatched,
            )?;

            candidate = next;
        }

        if check.test().is_none() && pattern_node.children().is_empty() {
            self.set_terminator(
                current,
                source,
                MirTerminatorKind::Goto(MirEdge::new(matched, [])),
            )?;
        }

        Ok(())
    }

    fn project_pattern_subject(
        &mut self,
        pattern: BoundPatternId,
        subject: MirOperand,
        current: MirBlockId,
        operation: PatternOperation,
    ) -> Result<MirOperand, LoweringError> {
        let check = self.input.patterns().pattern(pattern).unwrap_or_else(|| {
            panic!(
                "lowering contract violation: UnsupportedPattern {value:?}",
                value = pattern
            )
        });

        let Some(projection) = check.projection() else {
            return Ok(subject);
        };

        let source = self.source(self.pattern(pattern).origin());

        if projection == PatternProjection::OwnedTarget {
            let (MirOperand::Copy(owner) | MirOperand::Move(owner)) = &subject else {
                panic!(
                    "lowering contract violation: UnsupportedPattern {value:?}",
                    value = pattern
                );
            };

            let mut projections = owner.projections().to_vec();
            let owner_type = self.append_projection_dereferences(owner.ty(), &mut projections);
            let owner = MirPlace::new(owner.storage(), projections, owner_type);

            let kind = match operation {
                PatternOperation::Consume | PatternOperation::MutableBorrow => BorrowKind::Mutable,
                PatternOperation::Observe
                | PatternOperation::Copy
                | PatternOperation::SharedBorrow => BorrowKind::Shared,
                PatternOperation::Recovered => {
                    panic!(
                        "lowering contract violation: UnsupportedPattern {value:?}",
                        value = pattern
                    );
                }
            };

            let call = self.owned_target_call(owner.ty(), kind).unwrap_or_else(|| {
                panic!(
                    "lowering contract violation: UnsupportedPattern {value:?}",
                    value = pattern
                )
            });

            let place =
                self.project_owned_target(current, &source, &owner, call, check.input_type())?;

            return self.pattern_projected_place_operand(
                pattern,
                place,
                current,
                operation,
                source,
                check.input_type(),
            );
        }

        if let Some(place) = projected_pattern_place(&subject, projection, check.input_type()) {
            return self.pattern_projected_place_operand(
                pattern,
                place,
                current,
                operation,
                source,
                check.input_type(),
            );
        }

        let result = self.push_operation(
            current,
            source,
            MirOperationKind::PatternProjection {
                subject,
                projection,
                operation,
            },
            Some(check.input_type()),
        )?;

        Ok(result.result().map(MirOperand::Value).unwrap_or_else(|| {
            panic!(
                "lowering contract violation: UnsupportedPattern {value:?}",
                value = pattern
            )
        }))
    }

    fn pattern_projected_place_operand(
        &mut self,
        pattern: BoundPatternId,
        place: MirPlace,
        current: MirBlockId,
        operation: PatternOperation,
        source: bray_ir::MirSourceAnchor,
        result_type: bray_symbols::TypeId,
    ) -> Result<MirOperand, LoweringError> {
        match operation {
            PatternOperation::Observe | PatternOperation::Copy => Ok(MirOperand::Copy(place)),
            PatternOperation::Consume => Ok(MirOperand::Move(place)),
            PatternOperation::SharedBorrow | PatternOperation::MutableBorrow => {
                let kind = match operation {
                    PatternOperation::SharedBorrow => BorrowKind::Shared,
                    PatternOperation::MutableBorrow => BorrowKind::Mutable,
                    _ => panic!(
                        "lowering contract violation: UnsupportedPattern {value:?}",
                        value = pattern
                    ),
                };

                let result = self.push_operation(
                    current,
                    source,
                    MirOperationKind::Borrow { kind, place },
                    Some(result_type),
                )?;

                Ok(result.result().map(MirOperand::Value).unwrap_or_else(|| {
                    panic!(
                        "lowering contract violation: UnsupportedPattern {value:?}",
                        value = pattern
                    )
                }))
            }
            PatternOperation::Recovered => panic!(
                "lowering contract violation: UnsupportedPattern {value:?}",
                value = pattern
            ),
        }
    }

    fn store_direct_pattern_bindings(
        &mut self,
        pattern_id: BoundPatternId,
        pattern: &bray_bound_tree::BoundPattern,
        subject: MirOperand,
        current: MirBlockId,
    ) -> Result<(), LoweringError> {
        if self.pattern_introduces_direct_bindings(pattern_id) {
            for binding in pattern.bindings() {
                self.store_pattern_binding(
                    pattern_id,
                    *binding,
                    Self::retained_operand(&subject),
                    pattern.origin(),
                    current,
                    false,
                )?;
            }
        }

        for entry in pattern.entries() {
            if let BoundPatternEntryKind::Binding(binding) = entry.kind() {
                self.store_pattern_binding(
                    pattern_id,
                    binding,
                    Self::retained_operand(&subject),
                    pattern.origin(),
                    current,
                    true,
                )?;
            }
        }

        Ok(())
    }

    fn store_pattern_binding(
        &mut self,
        pattern: BoundPatternId,
        binding: LocalBindingSymbolId,
        subject: MirOperand,
        origin: bray_bound_tree::BoundNodeOrigin,
        current: MirBlockId,
        apply_projection: bool,
    ) -> Result<(), LoweringError> {
        let binding_type = self
            .input
            .patterns()
            .binding_type(binding)
            .unwrap_or_else(|| {
                panic!(
                    "lowering contract violation: UnsupportedPattern {value:?}",
                    value = pattern
                )
            });

        let operation = if !self.guard_bindings.is_empty() {
            PatternOperation::Observe
        } else {
            binding_type.operation()
        };

        let storage = self
            .input
            .storage_plan()
            .binding(StorageBindingTarget::Local(binding))
            .unwrap_or_else(|| {
                panic!(
                    "lowering contract violation: UnsupportedPattern {value:?}",
                    value = pattern
                )
            });

        if operation == PatternOperation::Observe && matches!(storage, StorageBinding::Access(_)) {
            // Observed bindings already name the checked source access and acquire no storage.
            return Ok(());
        }

        let value = match binding_type.projection().filter(|_| apply_projection) {
            Some(projection) => {
                if let Some(place) =
                    projected_pattern_place(&subject, projection, binding_type.ty())
                {
                    self.pattern_projected_place_operand(
                        pattern,
                        place,
                        current,
                        operation,
                        self.source(origin),
                        binding_type.ty(),
                    )?
                } else {
                    let commit = self.push_operation(
                        current,
                        self.source(origin),
                        MirOperationKind::PatternProjection {
                            subject,
                            projection,
                            operation,
                        },
                        Some(binding_type.ty()),
                    )?;

                    commit.result().map(MirOperand::Value).unwrap_or_else(|| {
                        panic!(
                            "lowering contract violation: UnsupportedPattern {value:?}",
                            value = pattern
                        )
                    })
                }
            }
            None => subject,
        };

        let destination = match storage {
            StorageBinding::Identity(identity) => {
                if !self.guard_bindings.is_empty() {
                    return self.store_guard_binding(
                        identity,
                        value,
                        binding_type.ty(),
                        origin,
                        current,
                    );
                }

                self.place_for_identity(identity, binding_type.ty(), origin)?
            }
            StorageBinding::Access(access) => self.place_for_access(access, true)?,
        };

        self.push_operation(
            current,
            self.source(origin),
            MirOperationKind::Store {
                kind: MirStoreKind::Initialize,
                destination,
                value,
            },
            None,
        )?;

        Ok(())
    }

    pub(super) fn pattern_place_operand(
        &mut self,
        pattern: BoundPatternId,
        place: MirPlace,
        current: MirBlockId,
    ) -> Result<MirOperand, LoweringError> {
        let check = self.input.patterns().pattern(pattern).unwrap_or_else(|| {
            panic!(
                "lowering contract violation: UnsupportedPattern {value:?}",
                value = pattern
            )
        });

        match check.operation() {
            PatternOperation::Consume => Ok(MirOperand::Move(place)),
            PatternOperation::Observe | PatternOperation::Copy => Ok(MirOperand::Copy(place)),
            PatternOperation::SharedBorrow | PatternOperation::MutableBorrow => {
                let kind = match check.operation() {
                    PatternOperation::SharedBorrow => BorrowKind::Shared,
                    PatternOperation::MutableBorrow => BorrowKind::Mutable,
                    PatternOperation::Observe
                    | PatternOperation::Consume
                    | PatternOperation::Copy
                    | PatternOperation::Recovered => {
                        panic!(
                            "lowering contract violation: UnsupportedPattern {value:?}",
                            value = pattern
                        );
                    }
                };

                let source = self.source(self.pattern(pattern).origin());

                let result = self.push_operation(
                    current,
                    source,
                    MirOperationKind::Borrow { kind, place },
                    Some(check.input_type()),
                )?;

                Ok(result.result().map(MirOperand::Value).unwrap_or_else(|| {
                    panic!(
                        "lowering contract violation: UnsupportedPattern {value:?}",
                        value = pattern
                    )
                }))
            }
            PatternOperation::Recovered => panic!(
                "lowering contract violation: UnsupportedPattern {value:?}",
                value = pattern
            ),
        }
    }

    fn pattern_operation(&self, pattern: BoundPatternId) -> PatternOperation {
        self.input
            .patterns()
            .pattern(pattern)
            .map(bray_bound_tree::PatternCheckEntry::operation)
            .filter(|operation| *operation != PatternOperation::Recovered)
            .unwrap_or_else(|| {
                panic!(
                    "lowering contract violation: UnsupportedPattern {value:?}",
                    value = pattern
                )
            })
    }

    fn pattern_introduces_direct_bindings(&self, pattern: BoundPatternId) -> bool {
        self.input
            .patterns()
            .pattern(pattern)
            .map(|pattern| pattern.target().is_none())
            .unwrap_or_else(|| {
                panic!(
                    "lowering contract violation: UnsupportedPattern {value:?}",
                    value = pattern
                )
            })
    }

    fn pattern(&self, pattern: BoundPatternId) -> bray_bound_tree::BoundPattern {
        // Recursive lowering must release the immutable unit view before mutating the MIR builder.
        self.input
            .unit()
            .view()
            .pattern(pattern)
            .cloned()
            .unwrap_or_else(|| {
                panic!(
                    "lowering contract violation: MissingBoundNode {value:?}",
                    value = pattern
                )
            })
    }
}
