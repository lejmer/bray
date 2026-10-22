use bray_binder::BindingQueryContext;
use bray_bound_tree::{
    CheckedTemplateDispatch, CheckedTemplateNodeId, CheckedTemplateOperation, ConversionTarget,
    SelectedConversion,
};

use super::build::SourceTemplateBuilder;
use crate::fact::FactQueryError;

impl SourceTemplateBuilder<'_, '_, '_> {
    pub(super) fn conversion_operation(
        &mut self,
        value: CheckedTemplateNodeId,
        conversion: &SelectedConversion,
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        let (callable, dispatch) = match conversion.target() {
            ConversionTarget::Composite(children) => {
                return self.composite_conversion(value, conversion, children);
            }
            ConversionTarget::Trait {
                fulfillment,
                witness,
                ..
            } => {
                let (implementation, substitution) = self.implementation_reference(*witness)?;

                (
                    *fulfillment,
                    CheckedTemplateDispatch::Implementation(implementation, substitution),
                )
            }
            ConversionTarget::TraitConstraint {
                member, dispatch, ..
            } => (*member, self.constraint_dispatch(*dispatch)?),
            ConversionTarget::Identity
            | ConversionTarget::CallableContract
            | ConversionTarget::NullablePresent
            | ConversionTarget::BuiltInScalar
            | ConversionTarget::CVariadicPromotion => {
                return Ok(CheckedTemplateOperation::Convert {
                    value,
                    target: conversion.target_type(),
                });
            }
        };

        let (callable, substitution) =
            self.declaration_reference(callable.definition().symbol(), callable.substitution())?;

        Ok(CheckedTemplateOperation::call(
            callable,
            substitution,
            [value],
            Some(dispatch),
        ))
    }

    fn composite_conversion(
        &mut self,
        value: CheckedTemplateNodeId,
        conversion: &SelectedConversion,
        children: &[SelectedConversion],
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        match self
            .context
            .semantic_values()
            .type_data(conversion.source_type())
            .as_ref()
        {
            bray_symbols::TypeData::Tuple(elements) => {
                assert_eq!(
                    elements.len(),
                    children.len(),
                    "selected tuple conversion has one plan per component"
                );

                let mut converted = Vec::with_capacity(children.len());

                for (ordinal, child) in children.iter().enumerate() {
                    let element = self.push_node(
                        CheckedTemplateOperation::TupleElement {
                            subject: value,
                            index: bray_symbols::SymbolOrdinal::new(super::build::index(ordinal)),
                        },
                        child.source_type(),
                    );

                    let operation = self.conversion_operation(element, child)?;

                    converted.push(self.push_node(operation, child.target_type()));
                }

                let tuple =
                    self.context
                        .semantic_values()
                        .intern_type(bray_symbols::TypeData::tuple(
                            children.iter().map(SelectedConversion::target_type),
                        ))?;

                let value =
                    self.push_node(CheckedTemplateOperation::Tuple(converted.into()), tuple);

                Ok(CheckedTemplateOperation::Convert {
                    value,
                    target: conversion.target_type(),
                })
            }
            bray_symbols::TypeData::Nullable(_) => {
                let [child] = children else {
                    panic!("selected nullable conversion has one payload plan");
                };

                let boolean = self
                    .context
                    .compilation()
                    .target_property_type(bray_target::TargetPropertyKind::ScalarBool)?;

                let test = self.nullable_test(value, conversion.source_type(), true)?;
                let condition = self.push_node(test, boolean);

                let present = self.push_node(
                    CheckedTemplateOperation::NullableValue { subject: value },
                    child.source_type(),
                );

                let operation = self.conversion_operation(present, child)?;
                let present = self.push_node(operation, child.target_type());

                let present = self.push_node(
                    CheckedTemplateOperation::Convert {
                        value: present,
                        target: conversion.target_type(),
                    },
                    conversion.target_type(),
                );

                let absent = self.nullable_absence_term(conversion.target_type())?;

                let absent = self.push_node(
                    CheckedTemplateOperation::Constant {
                        term: absent,
                        usage: Default::default(),
                    },
                    conversion.target_type(),
                );

                Ok(CheckedTemplateOperation::Conditional {
                    condition,
                    when_true: present,
                    when_false: absent,
                    state_outputs: Default::default(),
                })
            }
            bray_symbols::TypeData::Array { .. } => {
                let [child] = children else {
                    panic!("selected array conversion has one element plan");
                };

                let element = self.push_node(
                    CheckedTemplateOperation::ArrayElement { subject: value },
                    child.source_type(),
                );

                let operation = self.conversion_operation(element, child)?;
                let converted = self.push_node(operation, child.target_type());

                Ok(CheckedTemplateOperation::ArrayTransform {
                    subject: value,
                    element,
                    value: converted,
                })
            }
            _ => panic!("checked composite conversion retains an aggregate source shape"),
        }
    }
}
