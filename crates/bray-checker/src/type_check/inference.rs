use std::collections::BTreeSet;

use bray_bound_tree::{BoundExpressionId, ExpressionTypeResult, ExpressionTypeStatus};
use bray_symbols::TypeId;

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
    pub(super) expected: TypeId,
    pub(super) actual: TypeId,
    pub(super) is_directional: bool,
}

pub(super) struct TypeInferenceContext {
    nodes: Vec<InferenceNode>,
    error_type: TypeId,
    never_type: TypeId,
    implicit_compatibilities: BTreeSet<(TypeId, TypeId)>,
    conflicts: Vec<TypeConflict>,
    revision: u64,
}

impl TypeInferenceContext {
    pub(super) fn new(error_type: TypeId, never_type: TypeId) -> Self {
        Self {
            nodes: Vec::new(),
            error_type,
            never_type,
            implicit_compatibilities: BTreeSet::new(),
            conflicts: Vec::new(),
            revision: 0,
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

        self.revision = self.revision.saturating_add(1);

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
            None => {
                self.nodes[index].evidence = Some(ty);
                self.revision = self.revision.saturating_add(1);
            }
            Some(current) => {
                let (merged, recovered) = self.merge_evidence(current, ty, expression);

                if merged != current || recovered && !self.nodes[index].is_recovered {
                    self.revision = self.revision.saturating_add(1);
                }

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

        let expectation = TypeExpectation { expression, ty };

        if !self.nodes[index]
            .expectations
            .iter()
            .any(|current| current.expression == expression && current.ty == ty)
        {
            self.nodes[index].expectations.push(expectation);
            self.revision = self.revision.saturating_add(1);
        }
    }

    pub(super) fn replace_evidence(&mut self, id: InferenceTypeId, ty: TypeId) {
        let root = self.find(id);

        let Some(index) = root.to_index() else {
            return;
        };

        if self.nodes[index].evidence != Some(ty) {
            self.nodes[index].evidence = Some(ty);
            self.revision = self.revision.saturating_add(1);
        }
    }

    pub(super) fn add_implicit_compatibility(&mut self, expected: TypeId, actual: TypeId) {
        self.implicit_compatibilities.insert((expected, actual));
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
        self.revision = self.revision.saturating_add(1);

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

    pub(super) fn try_unique_matching_expectation<E>(
        &mut self,
        id: InferenceTypeId,
        mut is_match: impl FnMut(TypeId) -> Result<bool, E>,
    ) -> Result<Option<TypeId>, E> {
        let root = self.find(id);

        let Some(index) = root.to_index() else {
            return Ok(None);
        };

        let Some(node) = self.nodes.get(index) else {
            return Ok(None);
        };

        let mut selected = None;

        for expectation in &node.expectations {
            if !is_match(expectation.ty)? {
                continue;
            }

            match selected {
                Some(current) if current != expectation.ty => return Ok(None),
                Some(_) => {}
                None => selected = Some(expectation.ty),
            }
        }

        Ok(selected)
    }

    pub(super) fn unique_expectation(&mut self, id: InferenceTypeId) -> Option<TypeId> {
        self.try_unique_matching_expectation(id, |_| Ok::<_, std::convert::Infallible>(true))
            .ok()
            .flatten()
    }

    pub(super) fn result(&mut self, id: InferenceTypeId) -> Option<ExpressionTypeResult> {
        let root = self.find(id);
        let index = root.to_index()?;
        let node = self.nodes.get(index)?;
        let ty = node.evidence?;

        let status = if node.is_recovered || ty == self.error_type {
            ExpressionTypeStatus::Recovered
        } else {
            ExpressionTypeStatus::Valid
        };

        Some(ExpressionTypeResult::new(ty, status))
    }

    pub(super) fn is_recovered(&mut self, id: InferenceTypeId) -> bool {
        let root = self.find(id);

        let Some(index) = root.to_index() else {
            return true;
        };

        self.nodes.get(index).is_none_or(|node| node.is_recovered)
    }

    pub(super) const fn revision(&self) -> u64 {
        self.revision
    }

    pub(super) fn mark_recovered(&mut self, id: InferenceTypeId) {
        let root = self.find(id);

        let Some(index) = root.to_index() else {
            return;
        };

        if !self.nodes[index].is_recovered {
            self.nodes[index].is_recovered = true;
            self.revision = self.revision.saturating_add(1);
        }
    }

    pub(super) fn add_directional_conflict(
        &mut self,
        expression: BoundExpressionId,
        expected: TypeId,
        actual: TypeId,
    ) {
        self.conflicts.push(TypeConflict {
            expression,
            expected,
            actual,
            is_directional: true,
        });
    }

    pub(super) fn check_expectations(
        &mut self,
        expressions: &[(BoundExpressionId, InferenceTypeId)],
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

            if self.nodes[index].is_recovered || actual == self.error_type {
                self.nodes[index].is_recovered = true;
                continue;
            }

            let expectations = std::mem::take(&mut self.nodes[index].expectations);

            for expectation in &expectations {
                if self.compatibility(expectation.ty, actual) == TypeCompatibility::Incompatible {
                    self.conflicts.push(TypeConflict {
                        expression: expectation.expression,
                        expected: expectation.ty,
                        actual,
                        is_directional: true,
                    });

                    self.mark_recovered(root);
                }
            }

            self.nodes[index].expectations = expectations;
        }
    }

    pub(super) fn finish(
        mut self,
        expressions: &[(BoundExpressionId, InferenceTypeId)],
    ) -> (
        Vec<(BoundExpressionId, ExpressionTypeResult)>,
        Vec<TypeConflict>,
        Vec<BoundExpressionId>,
    ) {
        self.check_expectations(expressions);

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

        if current == self.error_type || incoming == self.error_type {
            return (self.error_type, true);
        }

        self.conflicts.push(TypeConflict {
            expression,
            expected: current,
            actual: incoming,
            is_directional: false,
        });

        (self.error_type, true)
    }

    fn compatibility(&self, expected: TypeId, actual: TypeId) -> TypeCompatibility {
        if expected == self.error_type || actual == self.error_type {
            TypeCompatibility::Recovered
        } else if expected == actual
            || actual == self.never_type
            || self.implicit_compatibilities.contains(&(expected, actual))
        {
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
