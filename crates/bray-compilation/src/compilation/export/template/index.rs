use super::super::PackageInterfaceExportError;
use super::build::{SourceTemplateBuilder, incomplete};
use bray_bound_tree::{
    BoundExpressionId, BoundStructuredExpressionKind, SelectedOperation, SemanticSelection,
};
use bray_package_interface::{InterfaceCheckedTemplateOperation, InterfaceTemplateReference};

impl SourceTemplateBuilder<'_, '_, '_> {
    pub(super) fn index(
        &mut self,
        id: BoundExpressionId,
        expression: &bray_bound_tree::BoundStructuredExpression,
    ) -> Result<InterfaceCheckedTemplateOperation, PackageInterfaceExportError> {
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
                let (callable, substitution) = self.export.declaration_template_reference(
                    fulfillment.definition().symbol(),
                    fulfillment.substitution(),
                )?;

                Some(std::sync::Arc::new(
                    bray_package_interface::InterfaceCheckedTemplateIndexCall {
                        callable,
                        substitution,
                        borrow_kind: *borrow_kind,
                        dispatch: {
                            let (implementation, substitution) =
                                self.export.implementation_template_reference(*witness)?;

                            bray_bound_tree::CheckedTemplateIndexDispatch::Implementation(
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
                let (callable, substitution) = self.export.declaration_template_reference(
                    member.definition().symbol(),
                    member.substitution(),
                )?;

                Some(std::sync::Arc::new(
                    bray_package_interface::InterfaceCheckedTemplateIndexCall {
                        callable,
                        substitution,
                        borrow_kind: *borrow_kind,
                        dispatch: match dispatch {
                            bray_symbols::TraitConstraintDispatch::Constraint {
                                owner,
                                ordinal,
                            } => bray_bound_tree::CheckedTemplateIndexDispatch::Constraint {
                                owner: InterfaceTemplateReference::Symbol(
                                    self.export.symbol_reference(owner.symbol())?,
                                ),
                                ordinal: *ordinal,
                            },
                            bray_symbols::TraitConstraintDispatch::TraitDefault(requirement) => {
                                let (subject, substitution) = self
                                    .export
                                    .trait_default_template_requirement(*requirement)?;

                                bray_bound_tree::CheckedTemplateIndexDispatch::TraitDefault {
                                    subject,
                                    substitution,
                                }
                            }
                        },
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

                Ok(InterfaceCheckedTemplateOperation::Index {
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

                Ok(InterfaceCheckedTemplateOperation::Slice {
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
