use bray_binder::BindingQueryContext;
use bray_bound_tree::{
    BoundPatternId, BoundReferenceTarget, CheckedTemplateNodeId, CheckedTemplateOperation,
    CheckedTemplateShortCircuitKind, PatternPredicate, PatternProjection,
};
use bray_symbols::{
    ConstantBinaryOperation, ConstantTermData, ConstantValueData, ConstantValueKind,
};

use super::build::SourceTemplateBuilder;
use crate::fact::FactQueryError;

impl SourceTemplateBuilder<'_, '_, '_> {
    pub(super) fn bind_pattern_value(
        &mut self,
        pattern: BoundPatternId,
        subject: CheckedTemplateNodeId,
    ) -> Result<(), FactQueryError> {
        let bindings = self.pattern_binding_values(pattern, subject)?;

        for (reference, value) in bindings {
            let binding = bray_bound_tree::StorageBindingTarget::from_reference_target(reference)
                .and_then(|target| self.storage.binding(target));

            if let Some(bray_bound_tree::StorageBinding::Identity(root)) = binding {
                let value = self.materialize(value);

                self.introduce_state(root, value);
            } else {
                self.bindings.insert(reference, value);
            }
        }

        Ok(())
    }

    fn pattern_binding_values(
        &mut self,
        pattern: BoundPatternId,
        subject: CheckedTemplateNodeId,
    ) -> Result<
        std::collections::BTreeMap<BoundReferenceTarget, CheckedTemplateNodeId>,
        FactQueryError,
    > {
        let value = self.projected_pattern_value(pattern, subject)?;

        let bound = self
            .unit
            .view()
            .pattern(pattern)
            .expect("checked pattern exists");

        let checked = self
            .patterns
            .pattern(pattern)
            .expect("checked pattern retains its semantics");

        let mut bindings = checked
            .bindings(bound)
            .map(|binding| (BoundReferenceTarget::Local(binding.into()), value))
            .collect::<std::collections::BTreeMap<_, _>>();

        let alternative = bound.kind() == bray_bound_tree::BoundPatternKind::Alternative;

        for child in bound.children().iter().rev() {
            let values = self.pattern_binding_values(*child, value)?;

            let condition = alternative
                .then(|| self.pattern_condition(*child, value))
                .transpose()?;

            for (binding, selected) in values {
                let selected = if let Some(condition) = condition
                    && let Some(previous) = bindings.get(&binding)
                {
                    let ty = self.nodes
                        [usize::try_from(selected.raw()).expect("binding node fits")]
                    .ty();

                    self.push_node(
                        CheckedTemplateOperation::Conditional {
                            condition,
                            when_true: selected,
                            when_false: *previous,
                            state_outputs: Default::default(),
                        },
                        ty,
                    )
                } else {
                    selected
                };

                bindings.insert(binding, selected);
            }
        }

        Ok(bindings)
    }

    pub(super) fn pattern_condition(
        &mut self,
        pattern: BoundPatternId,
        subject: CheckedTemplateNodeId,
    ) -> Result<CheckedTemplateNodeId, FactQueryError> {
        let subject = self.projected_pattern_value(pattern, subject)?;

        let checked = self
            .patterns
            .pattern(pattern)
            .expect("checked pattern retains its semantics");

        let boolean = self
            .context
            .compilation()
            .target_property_type(bray_target::TargetPropertyKind::ScalarBool)?;

        let alternative = self
            .unit
            .view()
            .pattern(pattern)
            .expect("checked pattern exists")
            .kind()
            == bray_bound_tree::BoundPatternKind::Alternative;

        let mut condition = match checked.test() {
            Some(PatternPredicate::ActiveUnionVariant(variant)) => self.push_node(
                CheckedTemplateOperation::VariantTest {
                    subject,
                    variant: self.symbol_key(variant.into())?,
                },
                boolean,
            ),
            Some(PatternPredicate::Literal(literal)) => {
                let value = self
                    .context
                    .semantic_values()
                    .intern_constant_term(ConstantTermData::Value(literal.value()))?;

                let right = self.push_node(
                    CheckedTemplateOperation::Constant {
                        term: value,
                        usage: Default::default(),
                    },
                    checked.input_type(),
                );

                self.push_node(
                    CheckedTemplateOperation::Binary {
                        operation: ConstantBinaryOperation::Equal,
                        left: subject,
                        right,
                    },
                    boolean,
                )
            }
            Some(PatternPredicate::NullableAbsent | PatternPredicate::NullablePresent) => {
                let operation = self.nullable_test(
                    subject,
                    checked.input_type(),
                    checked.test() == Some(PatternPredicate::NullablePresent),
                )?;

                self.push_node(operation, boolean)
            }
            Some(PatternPredicate::Constant(term)) => {
                let right = self.push_node(
                    CheckedTemplateOperation::Constant {
                        term,
                        usage: Default::default(),
                    },
                    checked.input_type(),
                );

                self.push_node(
                    CheckedTemplateOperation::Binary {
                        operation: ConstantBinaryOperation::Equal,
                        left: subject,
                        right,
                    },
                    boolean,
                )
            }
            _ => {
                let value = self.context.semantic_values().intern_constant_value(
                    ConstantValueData::new(boolean, ConstantValueKind::Boolean(!alternative)),
                )?;

                let term = self
                    .context
                    .semantic_values()
                    .intern_constant_term(ConstantTermData::Value(value))?;

                self.push_node(
                    CheckedTemplateOperation::Constant {
                        term,
                        usage: Default::default(),
                    },
                    boolean,
                )
            }
        };

        for child in self
            .unit
            .view()
            .pattern(pattern)
            .expect("checked pattern exists")
            .children()
        {
            let right = self.pattern_condition(*child, subject)?;

            condition = self.push_node(
                CheckedTemplateOperation::ShortCircuit {
                    kind: if alternative {
                        CheckedTemplateShortCircuitKind::Or
                    } else {
                        CheckedTemplateShortCircuitKind::And
                    },
                    left: condition,
                    right,
                },
                boolean,
            );
        }

        Ok(condition)
    }

    fn projected_pattern_value(
        &mut self,
        pattern: BoundPatternId,
        subject: CheckedTemplateNodeId,
    ) -> Result<CheckedTemplateNodeId, FactQueryError> {
        if let Some(value) = self.pattern_values.get(&pattern) {
            return Ok(*value);
        }

        let checked = self
            .patterns
            .pattern(pattern)
            .expect("checked pattern retains its semantics");

        let Some(projection) = checked.projection() else {
            self.pattern_values.insert(pattern, subject);

            return Ok(subject);
        };

        if projection == PatternProjection::OwnedTarget {
            return Err(super::build::incomplete(
                "unsupported const pattern projection",
            ));
        }

        let operation = self.value_projection(subject, projection.into())?;

        let value = self.push_node(operation, checked.input_type());

        self.pattern_values.insert(pattern, value);

        Ok(value)
    }
}
