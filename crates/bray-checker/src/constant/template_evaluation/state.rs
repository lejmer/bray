use bray_bound_tree::{CheckedTemplateNodeId, CheckedTemplateOperation};
use bray_symbols::{AnySymbolId, ConstantField, ConstantValueId, ConstantValueKind};

use super::evaluator::TemplateEvaluator;
use super::support::{TemplateEvaluationFailure, template_index};
use crate::CheckerRequestContext;

impl<C: CheckerRequestContext + ?Sized> TemplateEvaluator<'_, C> {
    pub(super) fn replace_component(
        &mut self,
        subject: ConstantValueId,
        base: CheckedTemplateNodeId,
        component: CheckedTemplateNodeId,
        replacement: ConstantValueId,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        let mut path = Vec::new();
        let mut next = component;

        while next != base {
            path.push(next);

            next = self.template().nodes()[template_index(next.raw())]
                .operation()
                .inline_projection_subject()
                .expect("checked update retains its inline base");
        }

        path.reverse();

        self.replace_path(subject, &path, replacement)
    }

    fn replace_path(
        &mut self,
        subject: ConstantValueId,
        path: &[CheckedTemplateNodeId],
        replacement: ConstantValueId,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        let Some((component, remaining)) = path.split_first() else {
            return Ok(replacement);
        };

        let operation = self.template().nodes()[template_index(component.raw())]
            .operation()
            .clone();

        let parent = operation
            .inline_projection_subject()
            .expect("checked update contains only inline projections");

        // Walk each component once. Destination indices retain their earlier snapshots.
        let child = self.current_component(subject, parent, *component)?;
        let replacement = self.replace_path(child, remaining, replacement)?;
        let data = self.observed_value(subject)?;

        let elements = match data.kind() {
            ConstantValueKind::Product(fields) => fields.len(),
            ConstantValueKind::Union { fields, .. } => fields.len(),
            ConstantValueKind::Tuple(elements) | ConstantValueKind::Array(elements) => {
                elements.len()
            }
            ConstantValueKind::NullablePresent(_) => 1,
            _ => unreachable!("checked inline update has aggregate storage"),
        };

        self.budget
            .try_charge_elements(elements)
            .map_err(TemplateEvaluationFailure::Diagnostic)?;

        let kind = match (operation, data.kind()) {
            (
                CheckedTemplateOperation::Project { member, .. },
                ConstantValueKind::Product(fields),
            ) => {
                let Some(AnySymbolId::StructField(field)) = self.resolver.symbol(&member) else {
                    panic!("checked product update retains its field");
                };

                ConstantValueKind::Product(
                    fields
                        .iter()
                        .map(|entry| {
                            ConstantField::new(
                                *entry.field(),
                                if *entry.field() == field {
                                    replacement
                                } else {
                                    *entry.value()
                                },
                            )
                        })
                        .collect(),
                )
            }
            (
                CheckedTemplateOperation::Project { member, .. },
                ConstantValueKind::Union { variant, fields },
            ) => {
                let Some(AnySymbolId::UnionPayloadField(field)) = self.resolver.symbol(&member)
                else {
                    panic!("checked union update retains its payload field");
                };

                ConstantValueKind::Union {
                    variant: *variant,
                    fields: fields
                        .iter()
                        .map(|entry| {
                            ConstantField::new(
                                *entry.field(),
                                if *entry.field() == field {
                                    replacement
                                } else {
                                    *entry.value()
                                },
                            )
                        })
                        .collect(),
                }
            }
            (
                CheckedTemplateOperation::TupleElement { index, .. },
                ConstantValueKind::Tuple(elements),
            ) => {
                let mut elements = elements.to_vec();

                elements[index.to_index().expect("checked tuple index fits host")] = replacement;

                ConstantValueKind::Tuple(elements.into())
            }
            (
                CheckedTemplateOperation::NullableValue { .. },
                ConstantValueKind::NullablePresent(_),
            ) => ConstantValueKind::NullablePresent(replacement),
            (
                CheckedTemplateOperation::Index {
                    index, call: None, ..
                },
                ConstantValueKind::Array(elements),
            ) => {
                let index = self.evaluate_index_bound(index)?;
                let mut elements = elements.to_vec();

                elements[index] = replacement;

                ConstantValueKind::Array(elements.into())
            }
            _ => panic!("checked update matches its inline component type"),
        };

        Ok(self.intern_value(data.ty(), kind))
    }

    fn current_component(
        &mut self,
        subject: ConstantValueId,
        base: CheckedTemplateNodeId,
        component: CheckedTemplateNodeId,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        if component == base {
            return Ok(subject);
        }

        let node = &self.template().nodes()[template_index(component.raw())];
        let ty = node.ty();
        let operation = node.operation().clone();

        let parent = operation
            .inline_projection_subject()
            .expect("checked update retains an inline parent");

        let value = self.current_component(subject, base, parent)?;

        match operation {
            CheckedTemplateOperation::Project { member, .. } => {
                let ty = self.instantiate_type(ty)?;

                self.evaluate_projection(value, &member, ty)
            }
            CheckedTemplateOperation::TupleElement { index, .. } => {
                let data = self.observed_value(value)?;

                let ConstantValueKind::Tuple(elements) = data.kind() else {
                    panic!("checked inline tuple parent has tuple storage");
                };

                Ok(elements[index.to_index().expect("checked tuple index fits host")])
            }
            CheckedTemplateOperation::NullableValue { .. } => {
                let data = self.observed_value(value)?;

                let ConstantValueKind::NullablePresent(value) = data.kind() else {
                    panic!("checked inline nullable parent is present");
                };

                Ok(*value)
            }
            CheckedTemplateOperation::Index {
                index, call: None, ..
            } => {
                let index = self.evaluate_index_bound(index)?;
                let data = self.observed_value(value)?;

                let ConstantValueKind::Array(elements) = data.kind() else {
                    panic!("checked inline array parent has array storage");
                };

                Ok(elements[index])
            }
            _ => unreachable!("checked inline parent has a supported projection"),
        }
    }
}
