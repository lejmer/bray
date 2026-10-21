use bray_bound_tree::{CheckedTemplateDispatch, CheckedTemplateNodeId, CheckedTemplateOperation, ConversionTarget, SelectedConversion};

use super::build::SourceTemplateBuilder;
use crate::fact::FactQueryError;

impl SourceTemplateBuilder<'_, '_, '_> {
    pub(super) fn conversion_operation(
        &self,
        value: CheckedTemplateNodeId,
        conversion: &SelectedConversion,
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        let (callable, dispatch) = match conversion.target() {
            ConversionTarget::Trait { fulfillment, witness, .. } => {
                let (implementation, substitution) = self.implementation_reference(*witness)?;
                (*fulfillment, CheckedTemplateDispatch::Implementation(implementation, substitution))
            }
            ConversionTarget::TraitConstraint { member, dispatch, .. } => {
                (*member, self.constraint_dispatch(*dispatch)?)
            }
            ConversionTarget::Identity | ConversionTarget::CallableContract
            | ConversionTarget::NullablePresent | ConversionTarget::BuiltInScalar
            | ConversionTarget::CVariadicPromotion | ConversionTarget::Composite(_) => {
                return Ok(CheckedTemplateOperation::Convert { value, target: conversion.target_type() });
            }
        };
        let (callable, substitution) = self.declaration_reference(callable.definition().symbol(), callable.substitution())?;
        Ok(CheckedTemplateOperation::call(callable, substitution, [value], Some(dispatch)))
    }
}
