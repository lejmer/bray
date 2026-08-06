use std::sync::Arc;

use bray_bound_tree::{
    BoundExpressionId, BoundOperator, ConversionTarget, SelectedConversion, SelectedOperation,
    SemanticSelection, SemanticSelectionEntry,
};
use bray_checker::{
    CompilerKnownOperationEvidence, ImplementationSelectionEvidence, OperationCandidate,
    OperationCandidateState,
};
use bray_symbols::{SymbolKey, TypeId};

use crate::fact::FactQueryError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::compilation) struct OperationResolution {
    expression: BoundExpressionId,
    result_type: TypeId,
    expectations: Arc<[(BoundExpressionId, TypeId)]>,
    selection: Option<SelectedOperation>,
}

#[derive(Clone, Copy)]
pub(super) enum TraitOperation {
    Operator(BoundOperator),
    Index,
    Conversion(TypeId),
}

pub(super) struct TraitOperationCandidate {
    pub(super) key: SymbolKey,
    pub(super) operation: SelectedOperation,
    pub(super) operand_types: Vec<TypeId>,
    pub(super) implementation_selection: Option<ImplementationSelectionEvidence>,
    pub(super) compiler_known_operation: CompilerKnownOperationEvidence,
}

impl TraitOperationCandidate {
    pub(super) fn into_candidate(self) -> OperationCandidate {
        let candidate = OperationCandidate::symbol(
            self.key,
            self.operation,
            self.operand_types,
            OperationCandidateState::Available,
        )
        .with_compiler_known_operations([self.compiler_known_operation]);

        match self.implementation_selection {
            Some(selection) => candidate.with_implementation_selections([selection]),
            None => candidate,
        }
    }
}

pub(super) struct ConversionPlan {
    conversion: SelectedConversion,
    implementation_selections: Vec<ImplementationSelectionEvidence>,
    compiler_known_operations: Vec<CompilerKnownOperationEvidence>,
}

impl ConversionPlan {
    pub(super) fn built_in(conversion: SelectedConversion) -> Self {
        Self {
            conversion,
            implementation_selections: Vec::new(),
            compiler_known_operations: Vec::new(),
        }
    }

    pub(super) fn composite(
        source: TypeId,
        target: TypeId,
        children: impl IntoIterator<Item = Self>,
    ) -> Self {
        let mut conversions = Vec::new();
        let mut implementation_selections = Vec::new();
        let mut compiler_known_operations = Vec::new();

        for child in children {
            conversions.push(child.conversion);
            implementation_selections.extend(child.implementation_selections);
            compiler_known_operations.extend(child.compiler_known_operations);
        }

        Self {
            conversion: SelectedConversion::new(
                source,
                target,
                ConversionTarget::Composite(conversions.into()),
            ),
            implementation_selections,
            compiler_known_operations,
        }
    }

    pub(super) fn trait_backed(candidate: TraitOperationCandidate) -> Result<Self, FactQueryError> {
        let SelectedOperation::Conversion(conversion) = candidate.operation else {
            return Err(FactQueryError::InfrastructureFailure);
        };

        Ok(Self {
            conversion,
            implementation_selections: candidate.implementation_selection.into_iter().collect(),
            compiler_known_operations: vec![candidate.compiler_known_operation],
        })
    }

    pub(super) fn into_candidate(self, source: TypeId) -> OperationCandidate {
        OperationCandidate::built_in(
            SelectedOperation::Conversion(self.conversion),
            [source],
            OperationCandidateState::Available,
        )
        .with_implementation_selections(self.implementation_selections)
        .with_compiler_known_operations(self.compiler_known_operations)
    }
}

impl OperationResolution {
    pub(in crate::compilation) fn new(
        expression: BoundExpressionId,
        result_type: TypeId,
        expectations: impl IntoIterator<Item = (BoundExpressionId, TypeId)>,
        selection: Option<SelectedOperation>,
    ) -> Self {
        Self {
            expression,
            result_type,
            expectations: expectations.into_iter().collect(),
            selection,
        }
    }

    pub(in crate::compilation) const fn expression(&self) -> BoundExpressionId {
        self.expression
    }

    pub(in crate::compilation) const fn result_type(&self) -> TypeId {
        self.result_type
    }

    pub(in crate::compilation) fn expectations(&self) -> &[(BoundExpressionId, TypeId)] {
        &self.expectations
    }

    pub(in crate::compilation) fn selection_entry(&self) -> Option<SemanticSelectionEntry> {
        // The operation fact retains its selection while the type input owns the table entry.
        self.selection.clone().map(|selection| {
            SemanticSelectionEntry::new(self.expression, SemanticSelection::Operation(selection))
        })
    }
}
