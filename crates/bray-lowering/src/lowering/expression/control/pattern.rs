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

impl Lowerer<'_> {
    pub(super) fn lower_pattern_branch(
        &mut self,
        pattern: BoundPatternId,
        subject: MirOperand,
        current: MirBlockId,
        matched: MirBlockId,
        unmatched: MirBlockId,
    ) -> Result<(), LoweringError> {
        let pattern_node = self.pattern(pattern)?;
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

        self.builder.set_terminator(
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
        let fact = self
            .input
            .pattern_facts()
            .pattern(pattern)
            .ok_or(LoweringError::UnsupportedPattern(pattern))?;

        if !matches!(
            fact.test(),
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
            || projection.source_type() != fact.input_type()
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
        let pattern_node = self.pattern(pattern)?;
        let operation = self.pattern_operation(pattern)?;
        let subject = self.project_pattern_subject(pattern, subject, current, operation)?;

        if self.pattern_introduces_direct_bindings(pattern)? {
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

    fn lower_alternative_pattern(
        &mut self,
        pattern: BoundPatternId,
        subject: MirOperand,
        current: MirBlockId,
        matched: MirBlockId,
        unmatched: MirBlockId,
    ) -> Result<(), LoweringError> {
        let pattern_node = self.pattern(pattern)?;
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

            self.builder.set_terminator(
                completion,
                Self::retained_source(&source),
                MirTerminatorKind::Goto(MirEdge::new(matched, [])),
            )?;

            candidate = next;
        }

        if pattern_node.children().is_empty() {
            self.builder.set_terminator(
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
        let pattern_node = self.pattern(pattern)?;
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
                self.builder.set_terminator(
                    current,
                    source,
                    MirTerminatorKind::Goto(MirEdge::new(unmatched, [])),
                )?;
            }

            return Ok(());
        }

        let fact = self
            .input
            .pattern_facts()
            .pattern(pattern)
            .ok_or(LoweringError::UnsupportedPattern(pattern))?;

        let projects_after_test = matches!(
            (fact.test(), fact.projection()),
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

        let children_entry = match (fact.test(), pattern_node.children().is_empty()) {
            (Some(_), true) => matched,
            (Some(_), false) => self
                .builder
                .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?,
            (None, _) => current,
        };

        if let Some(predicate) = fact.test() {
            self.builder.set_terminator(
                current,
                Self::retained_source(&source),
                MirTerminatorKind::PatternBranch {
                    subject: Self::retained_operand(&subject),
                    predicate,
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

        if fact.test().is_none() && pattern_node.children().is_empty() {
            self.builder.set_terminator(
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
        let fact = self
            .input
            .pattern_facts()
            .pattern(pattern)
            .ok_or(LoweringError::UnsupportedPattern(pattern))?;

        let Some(projection) = fact.projection() else {
            return Ok(subject);
        };

        let source = self.source(self.pattern(pattern)?.origin());

        let result = self.builder.push_operation(
            current,
            source,
            MirOperationKind::PatternProjection {
                subject,
                projection,
                operation,
            },
            Some(fact.input_type()),
        )?;

        result
            .result()
            .map(MirOperand::Value)
            .ok_or(LoweringError::UnsupportedPattern(pattern))
    }

    fn store_direct_pattern_bindings(
        &mut self,
        pattern_id: BoundPatternId,
        pattern: &bray_bound_tree::BoundPattern,
        subject: MirOperand,
        current: MirBlockId,
    ) -> Result<(), LoweringError> {
        if self.pattern_introduces_direct_bindings(pattern_id)? {
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
        let fact = self
            .input
            .pattern_facts()
            .binding_type(binding)
            .ok_or(LoweringError::UnsupportedPattern(pattern))?;

        let value = match fact.projection().filter(|_| apply_projection) {
            Some(projection) => {
                let commit = self.builder.push_operation(
                    current,
                    self.source(origin),
                    MirOperationKind::PatternProjection {
                        subject,
                        projection,
                        operation: fact.operation(),
                    },
                    Some(fact.ty()),
                )?;

                commit
                    .result()
                    .map(MirOperand::Value)
                    .ok_or(LoweringError::UnsupportedPattern(pattern))?
            }
            None => subject,
        };

        let storage = self
            .input
            .storage_plan()
            .binding(StorageBindingTarget::Local(binding))
            .ok_or(LoweringError::UnsupportedPattern(pattern))?;

        let destination = match storage {
            StorageBinding::Identity(identity) => {
                self.place_for_identity(identity, fact.ty(), origin)?
            }
            StorageBinding::Access(access) => self.place_for_access(access, true)?,
        };

        self.builder.push_operation(
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
        let fact = self
            .input
            .pattern_facts()
            .pattern(pattern)
            .ok_or(LoweringError::UnsupportedPattern(pattern))?;

        match fact.operation() {
            PatternOperation::Consume => Ok(MirOperand::Move(place)),
            PatternOperation::Observe | PatternOperation::Copy => Ok(MirOperand::Copy(place)),
            PatternOperation::SharedBorrow | PatternOperation::MutableBorrow => {
                let kind = match fact.operation() {
                    PatternOperation::SharedBorrow => BorrowKind::Shared,
                    PatternOperation::MutableBorrow => BorrowKind::Mutable,
                    PatternOperation::Observe
                    | PatternOperation::Consume
                    | PatternOperation::Copy
                    | PatternOperation::Recovered => {
                        return Err(LoweringError::UnsupportedPattern(pattern));
                    }
                };

                let source = self.source(self.pattern(pattern)?.origin());

                let result = self.builder.push_operation(
                    current,
                    source,
                    MirOperationKind::Borrow { kind, place },
                    Some(fact.input_type()),
                )?;

                result
                    .result()
                    .map(MirOperand::Value)
                    .ok_or(LoweringError::UnsupportedPattern(pattern))
            }
            PatternOperation::Recovered => Err(LoweringError::UnsupportedPattern(pattern)),
        }
    }

    fn pattern_operation(
        &self,
        pattern: BoundPatternId,
    ) -> Result<PatternOperation, LoweringError> {
        self.input
            .pattern_facts()
            .pattern(pattern)
            .map(bray_bound_tree::PatternCheckEntry::operation)
            .filter(|operation| *operation != PatternOperation::Recovered)
            .ok_or(LoweringError::UnsupportedPattern(pattern))
    }

    fn pattern_introduces_direct_bindings(
        &self,
        pattern: BoundPatternId,
    ) -> Result<bool, LoweringError> {
        self.input
            .pattern_facts()
            .pattern(pattern)
            .map(|fact| fact.target().is_none())
            .ok_or(LoweringError::UnsupportedPattern(pattern))
    }

    fn pattern(
        &self,
        pattern: BoundPatternId,
    ) -> Result<bray_bound_tree::BoundPattern, LoweringError> {
        // Recursive lowering must release the immutable unit view before mutating the MIR builder.
        self.input
            .unit()
            .view()
            .pattern(pattern)
            .cloned()
            .ok_or_else(|| LoweringError::MissingBoundNode(pattern.into()))
    }
}
