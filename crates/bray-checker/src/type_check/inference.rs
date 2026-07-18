use bray_bound_tree::BoundExpressionId;
use bray_symbols::TypeId;

use super::{ExpressionTypeResult, ExpressionTypeStatus};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct InferenceTypeId(u32);

impl InferenceTypeId {
    fn to_index(self) -> Option<usize> {
        usize::try_from(self.0).ok()
    }
}

#[derive(Clone, Copy)]
struct TypeExpectation {
    expression: BoundExpressionId,
    ty: TypeId,
}

struct InferenceNode {
    parent: InferenceTypeId,
    rank: u8,
    evidence: Option<TypeId>,
    expectations: Vec<TypeExpectation>,
    is_recovered: bool,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct TypeConflict {
    pub(super) expression: BoundExpressionId,
}

pub(super) struct TypeInferenceContext {
    nodes: Vec<InferenceNode>,
    error_type: TypeId,
    never_type: TypeId,
    conflicts: Vec<TypeConflict>,
}

impl TypeInferenceContext {
    pub(super) fn new(error_type: TypeId, never_type: TypeId) -> Self {
        Self {
            nodes: Vec::new(),
            error_type,
            never_type,
            conflicts: Vec::new(),
        }
    }

    pub(super) fn fresh(&mut self, is_recovered: bool) -> Option<InferenceTypeId> {
        let raw = u32::try_from(self.nodes.len()).ok()?;
        let id = InferenceTypeId(raw);

        self.nodes.push(InferenceNode {
            parent: id,
            rank: 0,
            evidence: None,
            expectations: Vec::new(),
            is_recovered,
        });

        Some(id)
    }

    pub(super) fn add_evidence(
        &mut self,
        id: InferenceTypeId,
        ty: TypeId,
        expression: BoundExpressionId,
    ) {
        let root = self.find(id);
        let Some(index) = root.to_index() else {
            return;
        };

        let current = self.nodes[index].evidence;

        match current {
            None => self.nodes[index].evidence = Some(ty),
            Some(current) => {
                let (merged, recovered) = self.merge_evidence(current, ty, expression);

                self.nodes[index].evidence = Some(merged);
                self.nodes[index].is_recovered |= recovered;
            }
        }
    }

    pub(super) fn add_expectation(
        &mut self,
        id: InferenceTypeId,
        ty: TypeId,
        expression: BoundExpressionId,
    ) {
        let root = self.find(id);
        let Some(index) = root.to_index() else {
            return;
        };

        self.nodes[index]
            .expectations
            .push(TypeExpectation { expression, ty });
    }

    pub(super) fn unify(
        &mut self,
        left: InferenceTypeId,
        right: InferenceTypeId,
        expression: BoundExpressionId,
    ) {
        let mut left = self.find(left);
        let mut right = self.find(right);

        if left == right {
            return;
        }

        let Some(left_index) = left.to_index() else {
            return;
        };

        let Some(right_index) = right.to_index() else {
            return;
        };

        if self.nodes[left_index].rank < self.nodes[right_index].rank {
            std::mem::swap(&mut left, &mut right);
        }

        let Some(left_index) = left.to_index() else {
            return;
        };

        let Some(right_index) = right.to_index() else {
            return;
        };

        self.nodes[right_index].parent = left;

        if self.nodes[left_index].rank == self.nodes[right_index].rank {
            self.nodes[left_index].rank = self.nodes[left_index].rank.saturating_add(1);
        }

        let right_evidence = self.nodes[right_index].evidence.take();
        let right_expectations = std::mem::take(&mut self.nodes[right_index].expectations);
        let right_recovered = self.nodes[right_index].is_recovered;

        self.nodes[left_index]
            .expectations
            .extend(right_expectations);
        self.nodes[left_index].is_recovered |= right_recovered;

        if let Some(evidence) = right_evidence {
            self.add_evidence(left, evidence, expression);
        }
    }

    pub(super) fn evidence(&mut self, id: InferenceTypeId) -> Option<TypeId> {
        let root = self.find(id);
        let index = root.to_index()?;

        self.nodes.get(index)?.evidence
    }

    pub(super) fn finish(
        mut self,
        expressions: &[(BoundExpressionId, InferenceTypeId)],
    ) -> (
        Vec<(BoundExpressionId, ExpressionTypeResult)>,
        Vec<TypeConflict>,
        Vec<BoundExpressionId>,
    ) {
        let mut checked = vec![false; self.nodes.len()];

        for &(_, id) in expressions {
            let root = self.find(id);
            let Some(index) = root.to_index() else {
                continue;
            };

            if checked[index] {
                continue;
            }

            checked[index] = true;

            let Some(actual) = self.nodes[index].evidence else {
                continue;
            };

            let expectations = std::mem::take(&mut self.nodes[index].expectations);

            for expectation in expectations {
                if self.compatibility(expectation.ty, actual) == TypeCompatibility::Incompatible {
                    self.nodes[index].is_recovered = true;
                    self.conflicts.push(TypeConflict {
                        expression: expectation.expression,
                    });
                }
            }

            if actual == self.error_type {
                self.nodes[index].is_recovered = true;
            }
        }

        self.conflicts.sort_unstable();
        self.conflicts.dedup();

        let mut unresolved = Vec::new();
        let mut unresolved_roots = vec![false; self.nodes.len()];

        for &(expression, id) in expressions {
            let root = self.find(id);
            let Some(index) = root.to_index() else {
                continue;
            };

            let Some(node) = self.nodes.get(index) else {
                continue;
            };

            if node.evidence.is_none() && !node.is_recovered && !unresolved_roots[index] {
                unresolved_roots[index] = true;
                unresolved.push(expression);
            }
        }

        let results = expressions
            .iter()
            .map(|&(expression, id)| {
                let root = self.find(id);
                let index = root.to_index();
                let node = index.and_then(|index| self.nodes.get(index));
                let ty = node
                    .and_then(|node| node.evidence)
                    .unwrap_or(self.error_type);
                let recovered =
                    node.is_none_or(|node| node.is_recovered || node.evidence.is_none());
                let status = if recovered {
                    ExpressionTypeStatus::Recovered
                } else {
                    ExpressionTypeStatus::Valid
                };

                (expression, ExpressionTypeResult::new(ty, status))
            })
            .collect();

        (results, self.conflicts, unresolved)
    }

    fn find(&mut self, id: InferenceTypeId) -> InferenceTypeId {
        let Some(index) = id.to_index() else {
            return id;
        };

        let Some(node) = self.nodes.get(index) else {
            return id;
        };

        if node.parent == id {
            return id;
        }

        let root = self.find(node.parent);

        if let Some(node) = self.nodes.get_mut(index) {
            node.parent = root;
        }

        root
    }

    fn merge_evidence(
        &mut self,
        current: TypeId,
        incoming: TypeId,
        expression: BoundExpressionId,
    ) -> (TypeId, bool) {
        if current == incoming || incoming == self.never_type {
            return (current, false);
        }

        if current == self.never_type {
            return (incoming, false);
        }

        if current == self.error_type {
            return (incoming, true);
        }

        if incoming == self.error_type {
            return (current, true);
        }

        self.conflicts.push(TypeConflict { expression });

        (current, true)
    }

    fn compatibility(&self, expected: TypeId, actual: TypeId) -> TypeCompatibility {
        if expected == self.error_type || actual == self.error_type {
            TypeCompatibility::Recovered
        } else if expected == actual || actual == self.never_type {
            TypeCompatibility::Compatible
        } else {
            TypeCompatibility::Incompatible
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum TypeCompatibility {
    Compatible,
    Recovered,
    Incompatible,
}
