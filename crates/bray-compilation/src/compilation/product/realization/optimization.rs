use std::collections::{BTreeMap, BTreeSet, VecDeque};
use bray_codegen::demanded_constant_terms;
use bray_ir::{
    MirAggregateKind, MirBinaryOperator, MirBlockId, MirEdge, MirImmediateValue,
    MirNullableQueryKind, MirOperand, MirOperationId, MirOperationKind, MirPatternPredicate, MirStorageId,
    MirStorageKind, MirTerminatorKind, MirUnaryOperator, MirUnit, MirValueId, MirValueOrigin,
    reconstruct_with_edits,
};
use bray_symbols::{BorrowKind, ConstantTermData, ConstantTermId, ConstantValueId, ConstantValueKind, SemanticValueStore};

use super::super::super::{CodegenPreparationError, Compilation};
use super::super::specialization::ConcreteCodegenInstance;
use crate::fact::CancellationToken;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Scalar {
    Unknown,
    Boolean(bool),
    NullablePresent,
    NullableAbsent,
    Constant(bray_symbols::ConstantValueId),
    Overdefined,
}

impl Scalar {
    fn join(self, other: Self) -> Self {
        match (self, other) {
            (Self::Unknown, value) | (value, Self::Unknown) => value,
            (left, right) if left == right => left,
            _ => Self::Overdefined,
        }
    }
}

#[derive(Clone)]
struct BlockState {
    executable: bool,
    queued: bool,
}

impl Compilation {
    pub(in crate::compilation::product) fn simplify_concrete_mir(
        &self,
        realization: &ConcreteCodegenInstance,
        unit: &MirUnit,
        cancellation: &CancellationToken,
    ) -> Result<MirUnit, CodegenPreparationError> {
        let values = self.semantic_value_store()?;
        let mut terms = BTreeMap::new();

        for term in demanded_constant_terms(unit) {
            let resolved = self.substitute_codegen_constant_term(term, realization.substitution())?;
            let data = values.constant_term_data(resolved);

            let ConstantTermData::Value(value) = data.as_ref() else {
                return Err(CodegenPreparationError::OpenConstantTerm(resolved));
            };

            terms.insert(term, *value);
        }

        let mut analysis = ScalarAnalysis::new(unit, values, terms);

        analysis.run(self, realization, cancellation)?;

        let mut terminators = BTreeMap::new();

        for (block_id, block) in unit.blocks_with_ids() {
            if !analysis.blocks[slot(block_id.slot())].executable {
                continue;
            }

            let local = analysis.evaluate_operations(block_id);

            if let Decision::Known(edge) =
                analysis.decision(self, realization, block.terminator().kind(), &local)?
            {
                if !matches!(block.terminator().kind(), MirTerminatorKind::Goto(_)) {
                    terminators.insert(block_id, MirTerminatorKind::Goto(edge));
                }
            }
        }

        let omitted = analysis.dead_scalar_operations(&terminators);

        if terminators.is_empty() && omitted.is_empty() {
            return Ok(unit.clone());
        }

        let (rewritten, _) = reconstruct_with_edits(unit, &terminators, &omitted)
            .map_err(CodegenPreparationError::MirCapacity)?;

        Ok(rewritten)
    }
}

enum Decision {
    Known(MirEdge),
    Pending,
    All,
}

struct ScalarAnalysis<'a> {
    unit: &'a MirUnit,
    values: &'a SemanticValueStore,
    terms: BTreeMap<ConstantTermId, ConstantValueId>,
    states: Vec<Scalar>,
    blocks: Vec<BlockState>,
    users: Vec<Vec<MirBlockId>>,
    pending: VecDeque<MirBlockId>,
    allow_unknown_edges: bool,
}

impl<'a> ScalarAnalysis<'a> {
    fn dead_scalar_operations(
        &self,
        terminators: &BTreeMap<MirBlockId, MirTerminatorKind>,
    ) -> BTreeSet<MirOperationId> {
        let mut uses = vec![0_usize; self.unit.values().len()];
        let mut pending = VecDeque::new();
        let mut omitted = BTreeSet::new();

        for (block_id, block) in self.unit.blocks_with_ids() {
            if !self.blocks[slot(block_id.slot())].executable {
                continue;
            }

            for operation_id in block.operations() {
                self.unit
                    .operation(*operation_id)
                    .expect("valid MIR operation")
                    .kind()
                    .for_each_operand(|operand| count_use(operand, &mut uses));
            }

            let mut terminator = terminators
                .get(&block_id)
                .unwrap_or_else(|| block.terminator().kind())
                .clone();

            terminator.for_each_input(|operand| count_use(operand, &mut uses));

            let _ = terminator.try_for_each_edge_mut::<()>(|edge| {
                for argument in edge.arguments() {
                    argument.for_each_operand(|operand| count_use(operand, &mut uses));
                }

                Ok(())
            });
        }

        for (block_id, block) in self.unit.blocks_with_ids() {
            if !self.blocks[slot(block_id.slot())].executable {
                continue;
            }

            for operation_id in block.operations() {
                if let Some(result) = self
                    .unit
                    .operation(*operation_id)
                    .expect("valid MIR operation")
                    .result()
                    && uses[slot(result.slot())] == 0
                {
                    pending.push_back(*operation_id);
                }
            }
        }

        while let Some(operation_id) = pending.pop_front() {
            let operation = self.unit.operation(operation_id).expect("valid MIR operation");

            let Some(result) = operation.result() else {
                continue;
            };

            if uses[slot(result.slot())] != 0
                || !dead_scalar_is_pure(operation.kind())
                || !omitted.insert(operation_id)
            {
                continue;
            }

            operation.kind().for_each_operand(|operand| {
                if let MirOperand::Value(value) = operand {
                    let count = &mut uses[slot(value.slot())];
                    *count -= 1;

                    if *count == 0
                        && let MirValueOrigin::Operation(producer) = self
                            .unit
                            .value(*value)
                            .expect("valid MIR value")
                            .origin()
                    {
                        pending.push_back(producer);
                    }
                }
            });
        }

        omitted
    }

    fn new(unit: &'a MirUnit, values: &'a SemanticValueStore, terms: BTreeMap<ConstantTermId, ConstantValueId>) -> Self {
        let mut users = vec![Vec::new(); unit.values().len()];

        for (block_id, block) in unit.blocks_with_ids() {
            let mut visit = |operand: &MirOperand| {
                if let MirOperand::Value(value) = operand {
                    users[slot(value.slot())].push(block_id);
                }
            };

            for operation in block.operations() {
                unit.operation(*operation)
                    .expect("valid MIR operation")
                    .kind()
                    .for_each_operand(&mut visit);
            }

            block.terminator().kind().for_each_input(&mut visit);

            // A clone is needed only to use the IR's exhaustive mutable edge walker.
            let mut terminator = block.terminator().kind().clone();

            let _ = terminator.try_for_each_edge_mut::<()>(|edge| {
                for argument in edge.arguments() {
                    argument.for_each_operand(&mut visit);
                }

                Ok(())
            });
        }

        for blocks in &mut users {
            blocks.sort_unstable();
            blocks.dedup();
        }

        Self {
            unit,
            values,
            terms,
            states: vec![Scalar::Unknown; unit.values().len()],
            blocks: vec![BlockState {
                executable: false,
                queued: false,
            }; unit.blocks().len()],
            users,
            pending: VecDeque::new(),
            allow_unknown_edges: false,
        }
    }

    fn enqueue(&mut self, block: MirBlockId) {
        let state = &mut self.blocks[slot(block.slot())];

        if !state.queued {
            state.queued = true;
            self.pending.push_back(block);
        }
    }

    fn update_value(&mut self, value: MirValueId, result: Scalar) {
        let index = slot(value.slot());
        let joined = self.states[index].join(result);

        if joined == self.states[index] {
            return;
        }

        self.states[index] = joined;

        // User lists are computed once, so changes reschedule only dependent blocks.
        let users = self.users[index].clone();

        for block in users {
            if self.blocks[slot(block.slot())].executable {
                self.enqueue(block);
            }
        }
    }

    fn enter_root(&mut self, block: MirBlockId) {
        let slot = slot(block.slot());

        self.blocks[slot].executable = true;

        let parameters = self.unit.block(block).expect("valid MIR root").parameters();

        for value in parameters {
            self.update_value(*value, Scalar::Overdefined);
        }

        self.enqueue(block);
    }

    fn run(
        &mut self,
        compilation: &Compilation,
        realization: &ConcreteCodegenInstance,
        cancellation: &CancellationToken,
    ) -> Result<(), CodegenPreparationError> {
        self.enter_root(self.unit.entry());

        if let Some(frame) = self.unit.frame_descriptor() {
            for state in frame.states() {
                self.enter_root(state.entry());
            }
        }

        self.drain(compilation, realization, cancellation)?;
        self.allow_unknown_edges = true;

        // One conservative completion wave handles cyclic values still unknown at the fixed point.
        for (block, _) in self.unit.blocks_with_ids() {
            if self.blocks[slot(block.slot())].executable {
                self.enqueue(block);
            }
        }

        self.drain(compilation, realization, cancellation)
    }

    fn drain(
        &mut self,
        compilation: &Compilation,
        realization: &ConcreteCodegenInstance,
        cancellation: &CancellationToken,
    ) -> Result<(), CodegenPreparationError> {
        while let Some(block_id) = self.pending.pop_front() {
            cancellation.check()?;

            self.blocks[slot(block_id.slot())].queued = false;

            let block = self.unit.block(block_id).expect("scheduled MIR block must exist");
            let local = self.evaluate_operations(block_id);

            let decision =
                self.decision(compilation, realization, block.terminator().kind(), &local)?;

            match decision {
                Decision::Known(edge) => self.enter_edge(&edge, &local),
                Decision::Pending => {}
                Decision::All => self.enter_all_edges(block.terminator().kind(), &local),
            }
        }

        Ok(())
    }

    fn evaluate_operations(&mut self, block_id: MirBlockId) -> BTreeMap<MirStorageId, Scalar> {
        let mut local = BTreeMap::new();
        let block = self.unit.block(block_id).expect("valid MIR block");

        for operation_id in block.operations() {
            let operation = self.unit.operation(*operation_id).expect("valid MIR operation");
            let kind = operation.kind();

            if let MirOperationKind::Store {
                destination, value, ..
            } = kind
            {
                if destination.projections().is_empty()
                    && self.unit.storage(destination.storage()).is_some_and(|storage| {
                        matches!(storage.kind(), MirStorageKind::Local | MirStorageKind::Temporary)
                    })
                {
                    let mut invalidates = false;

                    value.for_each_operand(|operand| {
                        invalidates |= match operand {
                            MirOperand::Move(_) => true,
                            MirOperand::Copy(_) => {
                                matches!(self.operand(operand, &local), Scalar::Overdefined)
                            }
                            _ => false,
                        };
                    });

                    if invalidates {
                        local.clear();
                    }

                    let value = self.operand(value, &local);
                    local.insert(destination.storage(), value);
                } else {
                    local.clear();
                }
            } else {
                let mut scalar_operands = true;

                kind.for_each_operand(|operand| {
                    scalar_operands &= match operand {
                        MirOperand::Move(_) => false,
                        MirOperand::Copy(_) => {
                            !matches!(self.operand(operand, &local), Scalar::Overdefined)
                        }
                        _ => true,
                    };
                });

                if !matches!(kind, MirOperationKind::Borrow { kind: BorrowKind::Shared, place } if place.projections().is_empty())
                    && (!scalar_operands
                    || !matches!(
                        kind,
                        MirOperationKind::Unary { .. }
                            | MirOperationKind::Binary { .. }
                            | MirOperationKind::NullableQuery(_)
                    ))
                {
                    local.clear();
                }
            }

            if let Some(result) = operation.result() {
                let value = self.operation(kind, &local);
                self.update_value(result, value);
            }
        }

        local
    }

    fn operation(&self, kind: &MirOperationKind, local: &BTreeMap<MirStorageId, Scalar>) -> Scalar {
        match kind {
            MirOperationKind::Unary {
                operator: MirUnaryOperator::Not,
                operand,
            } => match self.boolean(self.operand(operand, local)) {
                Some(value) => Scalar::Boolean(!value),
                None => unresolved(self.operand(operand, local)),
            },
            MirOperationKind::Binary {
                operator: MirBinaryOperator::Equal | MirBinaryOperator::NotEqual,
                left,
                right,
            } => {
                let left = self.operand(left, local);
                let right = self.operand(right, local);

                match (self.boolean(left), self.boolean(right)) {
                    (Some(left), Some(right)) => Scalar::Boolean(
                        (left == right)
                            == matches!(
                                kind,
                                MirOperationKind::Binary {
                                    operator: MirBinaryOperator::Equal,
                                    ..
                                }
                            ),
                    ),
                    _ if matches!(left, Scalar::Unknown) || matches!(right, Scalar::Unknown) => {
                        Scalar::Unknown
                    }
                    _ => Scalar::Overdefined,
                }
            }
            MirOperationKind::NullableQuery(query) => {
                let state = self.nullable_operand(query.operand(), local);

                match self.nullable_present(state) {
                    Some(present) => Scalar::Boolean(
                        present == matches!(query.kind(), MirNullableQueryKind::IsPresent),
                    ),
                    None => unresolved(state),
                }
            }
            MirOperationKind::Aggregate(aggregate)
                if aggregate.kind() == MirAggregateKind::NullablePresent =>
            {
                Scalar::NullablePresent
            }
            _ => Scalar::Overdefined,
        }
    }

    fn operand(&self, operand: &MirOperand, local: &BTreeMap<MirStorageId, Scalar>) -> Scalar {
        match operand {
            MirOperand::Value(value) => self.states[slot(value.slot())],
            MirOperand::Constant { value, .. } => Scalar::Constant(*value),
            MirOperand::ConstantTerm { term, .. } => Scalar::Constant(*self.terms.get(term).expect("concrete MIR term must resolve before scalar analysis")),
            MirOperand::Immediate {
                value: MirImmediateValue::Boolean(value),
                ..
            } => Scalar::Boolean(*value),
            MirOperand::Immediate {
                value: MirImmediateValue::NullableAbsent,
                ..
            } => Scalar::NullableAbsent,
            MirOperand::Copy(place) if place.projections().is_empty() => {
                match local.get(&place.storage()).copied() {
                    Some(value) if self.nullable_present(value) == Some(false) => {
                        Scalar::NullableAbsent
                    }
                    Some(value) => self.boolean(value).map_or(Scalar::Overdefined, Scalar::Boolean),
                    None => Scalar::Overdefined,
                }
            }
            MirOperand::Immediate { .. } | MirOperand::Copy(_) | MirOperand::Move(_) => {
                Scalar::Overdefined
            }
        }
    }

    fn boolean(&self, state: Scalar) -> Option<bool> {
        match state {
            Scalar::Boolean(value) => Some(value),
            Scalar::Constant(value) => {
                match self.values.constant_value_data(value).kind() {
                    ConstantValueKind::Boolean(value) => Some(*value),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn nullable_present(&self, state: Scalar) -> Option<bool> {
        match state {
            Scalar::NullablePresent => Some(true),
            Scalar::NullableAbsent => Some(false),
            Scalar::Constant(value) => match self.values.constant_value_data(value).kind() {
                ConstantValueKind::NullablePresent(_) => Some(true),
                ConstantValueKind::NullableAbsent => Some(false),
                _ => None,
            },
            _ => None,
        }
    }

    fn nullable_operand(
        &self,
        operand: &MirOperand,
        local: &BTreeMap<MirStorageId, Scalar>,
    ) -> Scalar {
        if let MirOperand::Value(value) = operand
            && let MirValueOrigin::Operation(operation) = self
                .unit
                .value(*value)
                .expect("valid MIR value")
                .origin()
            && let MirOperationKind::Borrow {
                kind: BorrowKind::Shared,
                place,
            } = self.unit.operation(operation).expect("valid MIR operation").kind()
            && place.projections().is_empty()
        {
            let state = local
                .get(&place.storage())
                .copied()
                .unwrap_or(Scalar::Overdefined);

            if self.nullable_present(state).is_some() {
                return state;
            }
        }

        self.operand(operand, local)
    }

    fn decision(
        &self,
        compilation: &Compilation,
        realization: &ConcreteCodegenInstance,
        terminator: &MirTerminatorKind,
        local: &BTreeMap<MirStorageId, Scalar>,
    ) -> Result<Decision, CodegenPreparationError> {
        let result = match terminator {
            MirTerminatorKind::Branch {
                condition,
                then_edge,
                else_edge,
            } => {
                let state = self.operand(condition, local);

                match self.boolean(state) {
                    Some(true) => Decision::Known(then_edge.clone()),
                    Some(false) => Decision::Known(else_edge.clone()),
                    None => self.unresolved_decision(state),
                }
            }
            MirTerminatorKind::PatternBranch {
                subject,
                predicate,
                matched,
                unmatched,
            } => {
                let state = self.operand(subject, local);

                let known = match predicate {
                    MirPatternPredicate::NullablePresent => {
                        self.nullable_present(self.nullable_operand(subject, local))
                    }
                    MirPatternPredicate::NullableAbsent => self
                        .nullable_present(self.nullable_operand(subject, local))
                        .map(|present| !present),
                    MirPatternPredicate::Literal(value) => self.equals_constant(state, *value),
                    MirPatternPredicate::Constant(term) => {
                        let term = compilation
                            .substitute_codegen_constant_term(*term, realization.substitution())?;

                        let data = self.values.constant_term_data(term);

                        let bray_symbols::ConstantTermData::Value(value) = data.as_ref() else {
                            return Err(CodegenPreparationError::OpenConstantTerm(term));
                        };

                        self.equals_constant(state, *value)
                    }
                    _ => None,
                };

                match known {
                    Some(true) => Decision::Known(matched.clone()),
                    Some(false) => Decision::Known(unmatched.clone()),
                    None => self.unresolved_decision(state),
                }
            }
            MirTerminatorKind::Switch {
                discriminant,
                cases,
                otherwise,
            } => {
                let state = self.operand(discriminant, local);

                if let Some(case) = cases
                    .iter()
                    .find(|case| self.equals_constant(state, case.value()) == Some(true))
                {
                    Decision::Known(case.edge().clone())
                } else if matches!(state, Scalar::Constant(_) | Scalar::Boolean(_) | Scalar::NullableAbsent)
                    && cases
                        .iter()
                        .all(|case| self.equals_constant(state, case.value()) == Some(false))
                {
                    Decision::Known(otherwise.clone())
                } else {
                    self.unresolved_decision(state)
                }
            }
            _ => Decision::All,
        };

        Ok(result)
    }

    fn equals_constant(
        &self,
        state: Scalar,
        value: bray_symbols::ConstantValueId,
    ) -> Option<bool> {
        match state {
            Scalar::Constant(actual) => Some(actual == value),
            Scalar::Boolean(actual) => match self.values.constant_value_data(value).kind() {
                ConstantValueKind::Boolean(expected) => Some(actual == *expected),
                _ => None,
            },
            Scalar::NullableAbsent => Some(matches!(
                self.values.constant_value_data(value).kind(),
                ConstantValueKind::NullableAbsent
            )),
            _ => None,
        }
    }

    fn unresolved_decision(&self, state: Scalar) -> Decision {
        if matches!(state, Scalar::Unknown) && !self.allow_unknown_edges {
            Decision::Pending
        } else {
            Decision::All
        }
    }

    fn enter_edge(
        &mut self,
        edge: &MirEdge,
        local: &BTreeMap<MirStorageId, Scalar>,
    ) {
        let arguments = edge
            .arguments()
            .iter()
            .map(|argument| self.operand(argument, local))
            .collect::<Vec<_>>();

        self.enter_successor(edge.target(), Some(&arguments));
    }

    fn enter_all_edges(
        &mut self,
        terminator: &MirTerminatorKind,
        local: &BTreeMap<MirStorageId, Scalar>,
    ) {
        let mut explicit = BTreeMap::<MirBlockId, usize>::new();

        // A clone lets the existing exhaustive edge walker expose argument-bearing edges.
        let mut terminator = terminator.clone();

        let _ = terminator.try_for_each_edge_mut::<()>(|edge| {
            *explicit.entry(edge.target()).or_default() += 1;
            self.enter_edge(edge, local);

            Ok(())
        });

        terminator.for_each_successor(|target| {
            let count = explicit.entry(target).or_default();

            if *count > 0 {
                *count -= 1;
            } else {
                self.enter_successor(target, None);
            }
        });
    }

    fn enter_successor(&mut self, target: MirBlockId, arguments: Option<&[Scalar]>) {
        let slot = slot(target.slot());
        let newly_executable = !self.blocks[slot].executable;

        self.blocks[slot].executable = true;

        let parameters = self.unit.block(target).expect("valid MIR successor").parameters();

        for (index, parameter) in parameters.iter().enumerate() {
            let incoming = arguments
                .and_then(|arguments| arguments.get(index))
                .copied()
                .unwrap_or(Scalar::Overdefined);

            self.update_value(*parameter, incoming);
        }

        if newly_executable {
            self.enqueue(target);
        }
    }
}

fn count_use(operand: &MirOperand, uses: &mut [usize]) {
    if let MirOperand::Value(value) = operand {
        uses[slot(value.slot())] += 1;
    }
}

fn dead_scalar_is_pure(kind: &MirOperationKind) -> bool {
    let scalar_operand = |operand: &MirOperand| {
        matches!(operand, MirOperand::Value(_) | MirOperand::Constant { .. } | MirOperand::ConstantTerm { .. } | MirOperand::Immediate { .. })
    };

    match kind {
        MirOperationKind::Unary { operator: MirUnaryOperator::Not, operand } => {
            scalar_operand(operand)
        }
        MirOperationKind::Binary {
            operator: MirBinaryOperator::Equal | MirBinaryOperator::NotEqual,
            left,
            right,
        } => scalar_operand(left) && scalar_operand(right),
        MirOperationKind::NullableQuery(query) => scalar_operand(query.operand()),
        _ => false,
    }
}

fn unresolved(state: Scalar) -> Scalar {
    if matches!(state, Scalar::Unknown) {
        Scalar::Unknown
    } else {
        Scalar::Overdefined
    }
}

fn slot(raw: u32) -> usize {
    usize::try_from(raw).expect("validated MIR identity must fit the host index")
}
