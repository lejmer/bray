use super::build::{SourceTemplateBuilder, incomplete};
use crate::fact::FactQueryError;
use bray_bound_tree::CheckedTemplateOperation;
use bray_bound_tree::{
    BoundExpressionId, BoundStructuredExpressionKind, SelectedOperation, SemanticSelection,
};

impl SourceTemplateBuilder<'_, '_, '_> {
    pub(super) fn index(
        &mut self,
        id: BoundExpressionId,
        expression: &bray_bound_tree::BoundStructuredExpression,
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        let Some(SemanticSelection::Operation(SelectedOperation::Index { target, .. })) =
            self.selections.expression(id)
        else {
            return Err(incomplete("invalid_index_selection"));
        };

        let call = match target {
            bray_bound_tree::IndexTarget::Custom {
                borrow_kind,
                fulfillment,
                witness,
                ..
            } => {
                let (callable, substitution) = self.declaration_reference(
                    fulfillment.definition().symbol(),
                    fulfillment.substitution(),
                )?;

                Some(std::sync::Arc::new(
                    bray_bound_tree::CheckedTemplateIndexCall {
                        callable,
                        substitution,
                        borrow_kind: *borrow_kind,
                        dispatch: {
                            let (implementation, substitution) =
                                self.implementation_reference(*witness)?;

                            bray_bound_tree::CheckedTemplateDispatch::Implementation(
                                implementation,
                                substitution,
                            )
                        },
                    },
                ))
            }
            bray_bound_tree::IndexTarget::TraitConstraint {
                borrow_kind,
                member,
                dispatch,
                ..
            } => {
                let (callable, substitution) = self
                    .declaration_reference(member.definition().symbol(), member.substitution())?;

                Some(std::sync::Arc::new(
                    bray_bound_tree::CheckedTemplateIndexCall {
                        callable,
                        substitution,
                        borrow_kind: *borrow_kind,
                        dispatch: self.constraint_dispatch(*dispatch)?,
                    },
                ))
            }
            _ => None,
        };

        match expression.kind() {
            BoundStructuredExpressionKind::ElementIndex => {
                let [subject, index] = expression.operands() else {
                    return Err(incomplete("invalid_index_operand_count"));
                };

                Ok(CheckedTemplateOperation::Index {
                    call,
                    subject: self.expression(*subject)?,
                    index: self.expression(*index)?,
                })
            }
            BoundStructuredExpressionKind::SliceIndex => {
                let Some(subject) = expression.operands().first() else {
                    return Err(incomplete("invalid_slice_operand_count"));
                };

                let bounds = expression
                    .slice_bounds()
                    .ok_or_else(|| incomplete("missing_slice_bounds"))?;

                Ok(CheckedTemplateOperation::Slice {
                    call,
                    subject: self.expression(*subject)?,
                    lower: bounds
                        .lower()
                        .map(|bound| self.expression(bound))
                        .transpose()?,
                    upper: bounds
                        .upper()
                        .map(|bound| self.expression(bound))
                        .transpose()?,
                })
            }
            _ => Err(incomplete("invalid_index_expression_kind")),
        }
    }
}
