use bray_bound_tree::{
    BoundPatternEntryKind, BoundPatternId, BoundPatternKind, PatternProjection, StorageBinding,
    StorageBindingTarget,
};
use bray_ir::{MirBlockId, MirBlockKind, MirEdge, MirOperand, MirOperationKind, MirTerminatorKind};
use bray_symbols::LocalBindingSymbolId;

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

    pub(in crate::lowering) fn lower_pattern_bindings(
        &mut self,
        pattern: BoundPatternId,
        subject: MirOperand,
        current: MirBlockId,
    ) -> Result<MirBlockId, LoweringError> {
        let pattern_node = self.pattern(pattern)?;
        let subject = self.project_pattern_subject(pattern, subject, current)?;

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
        let subject = self.project_pattern_subject(pattern, subject, current)?;
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
        let subject = self.project_pattern_subject(pattern, subject, current)?;

        if pattern_node.kind() == BoundPatternKind::Alternative {
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
                self.project_binding(pattern, subject, projection, fact.ty(), origin, current)?
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
            StorageBinding::Access(access) => self.place_for_access(access)?,
        };

        self.builder.push_operation(
            current,
            self.source(origin),
            MirOperationKind::Store { destination, value },
            None,
        )?;

        Ok(())
    }

    fn project_binding(
        &mut self,
        pattern: BoundPatternId,
        subject: MirOperand,
        projection: PatternProjection,
        ty: bray_symbols::TypeId,
        origin: bray_bound_tree::BoundNodeOrigin,
        current: MirBlockId,
    ) -> Result<MirOperand, LoweringError> {
        let commit = self.builder.push_operation(
            current,
            self.source(origin),
            MirOperationKind::PatternProjection {
                subject,
                projection,
            },
            Some(ty),
        )?;

        commit
            .result()
            .map(MirOperand::Value)
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
