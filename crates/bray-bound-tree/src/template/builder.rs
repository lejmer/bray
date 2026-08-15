use bray_symbols::{DependencyContractTemplateId, TypeId};

use super::{
    CheckedTemplate, CheckedTemplateBehavior, CheckedTemplateBuildError, CheckedTemplateCompletion,
    CheckedTemplateInput, CheckedTemplateInputId, CheckedTemplateKind, CheckedTemplateNode,
    CheckedTemplateNodeId, CheckedTemplateOperation, CheckedTemplateTemporary,
    CheckedTemplateTemporaryId,
};

/// Builder for one portable checked template.
///
/// Construction validates template-local references and type equalities. The checker or interface
/// decoder remains responsible for conditions that require semantic lookup, including canonical
/// boolean identity, callable signatures, member types, and structural result-type expansion.
#[derive(Debug)]
pub struct CheckedTemplateBuilder {
    kind: CheckedTemplateKind,
    inputs: Vec<CheckedTemplateInput>,
    nodes: Vec<CheckedTemplateNode>,
    temporaries: Vec<CheckedTemplateTemporary>,
    behavior: CheckedTemplateBehavior,
}

impl CheckedTemplateBuilder {
    /// Creates an empty builder for one declaration-owned semantic category.
    pub const fn new(kind: CheckedTemplateKind, behavior: CheckedTemplateBehavior) -> Self {
        Self {
            kind,
            inputs: Vec::new(),
            nodes: Vec::new(),
            temporaries: Vec::new(),
            behavior,
        }
    }

    /// Adds one explicit input in deterministic binding order.
    pub fn push_input(
        &mut self,
        input: CheckedTemplateInput,
    ) -> Result<CheckedTemplateInputId, CheckedTemplateBuildError> {
        let Some(id) = CheckedTemplateInputId::try_from_index(self.inputs.len()) else {
            return Err(CheckedTemplateBuildError::CapacityExceeded);
        };

        if let Some(index) = self
            .inputs
            .iter()
            .position(|existing| existing.kind() == input.kind())
        {
            let Some(first) = CheckedTemplateInputId::try_from_index(index) else {
                return Err(CheckedTemplateBuildError::CapacityExceeded);
            };

            return Err(CheckedTemplateBuildError::DuplicateInput {
                first,
                duplicate: id,
            });
        }

        self.inputs.push(input);

        Ok(id)
    }

    /// Adds one operation after validating all template-local references.
    pub fn push_node(
        &mut self,
        node: CheckedTemplateNode,
    ) -> Result<CheckedTemplateNodeId, CheckedTemplateBuildError> {
        let Some(id) = CheckedTemplateNodeId::try_from_index(self.nodes.len()) else {
            return Err(CheckedTemplateBuildError::CapacityExceeded);
        };

        validate_operation(
            id,
            node.operation(),
            node.ty(),
            &self.inputs,
            &self.nodes,
            &self.temporaries,
        )?;

        self.nodes.push(node);

        Ok(id)
    }

    /// Materializes one temporary from an existing operation result.
    pub fn push_temporary(
        &mut self,
        initializer: CheckedTemplateNodeId,
        ty: TypeId,
        dependency_contract: DependencyContractTemplateId,
    ) -> Result<CheckedTemplateTemporaryId, CheckedTemplateBuildError> {
        validate_present_node(initializer, self.nodes.len())?;

        let expected = referenced_node_type(initializer, &self.nodes)?;

        if expected != ty {
            return Err(
                CheckedTemplateBuildError::TemporaryInitializerTypeMismatch {
                    initializer,
                    expected,
                    actual: ty,
                },
            );
        }

        let Some(id) = CheckedTemplateTemporaryId::try_from_index(self.temporaries.len()) else {
            return Err(CheckedTemplateBuildError::CapacityExceeded);
        };

        self.temporaries.push(CheckedTemplateTemporary::new(
            initializer,
            ty,
            dependency_contract,
        ));

        Ok(id)
    }

    /// Completes the checked template after validating its result.
    pub fn finish(
        self,
        result: CheckedTemplateNodeId,
        completion: CheckedTemplateCompletion,
    ) -> Result<CheckedTemplate, CheckedTemplateBuildError> {
        if completion == CheckedTemplateCompletion::Recovered {
            return Err(CheckedTemplateBuildError::RecoveredTemplate);
        }

        validate_present_node(result, self.nodes.len())?;

        Ok(CheckedTemplate::new(
            self.kind,
            self.inputs,
            self.nodes,
            self.temporaries,
            result,
            self.behavior,
        ))
    }
}

fn validate_operation(
    node: CheckedTemplateNodeId,
    operation: &CheckedTemplateOperation,
    actual: TypeId,
    inputs: &[CheckedTemplateInput],
    nodes: &[CheckedTemplateNode],
    temporaries: &[CheckedTemplateTemporary],
) -> Result<(), CheckedTemplateBuildError> {
    if let CheckedTemplateOperation::Input(input) = operation {
        validate_input(*input, inputs.len())?;
    }

    operation.try_for_each_node_reference(|referenced| {
        validate_prior_node(node, referenced, nodes.len())
    })?;

    if let CheckedTemplateOperation::Temporary(temporary) = operation {
        validate_temporary(node, *temporary, temporaries)?;
    }

    validate_operation_types(node, operation, actual, inputs, nodes, temporaries)?;

    Ok(())
}

fn validate_operation_types(
    node: CheckedTemplateNodeId,
    operation: &CheckedTemplateOperation,
    actual: TypeId,
    inputs: &[CheckedTemplateInput],
    nodes: &[CheckedTemplateNode],
    temporaries: &[CheckedTemplateTemporary],
) -> Result<(), CheckedTemplateBuildError> {
    match operation {
        CheckedTemplateOperation::Input(input) => {
            let expected = input_type(*input, inputs)?;

            if expected != actual {
                return Err(CheckedTemplateBuildError::InputTypeMismatch {
                    node,
                    input: *input,
                    expected,
                    actual,
                });
            }
        }
        CheckedTemplateOperation::Temporary(temporary) => {
            let expected = temporary_type(*temporary, temporaries)?;

            if expected != actual {
                return Err(CheckedTemplateBuildError::TemporaryTypeMismatch {
                    node,
                    temporary: *temporary,
                    expected,
                    actual,
                });
            }
        }
        CheckedTemplateOperation::Convert { target, .. } if *target != actual => {
            return Err(CheckedTemplateBuildError::ConversionTypeMismatch {
                node,
                expected: *target,
                actual,
            });
        }
        CheckedTemplateOperation::Conditional {
            when_true,
            when_false,
            ..
        } => validate_conditional_types(node, *when_true, *when_false, actual, nodes)?,
        CheckedTemplateOperation::ShortCircuit { left, right, .. } => {
            validate_short_circuit_types(node, *left, *right, actual, nodes)?;
        }
        CheckedTemplateOperation::Array(elements) => validate_array_types(node, elements, nodes)?,
        CheckedTemplateOperation::Constant { .. }
        | CheckedTemplateOperation::Unary { .. }
        | CheckedTemplateOperation::Binary { .. }
        | CheckedTemplateOperation::Borrow { .. }
        | CheckedTemplateOperation::Declaration(_)
        | CheckedTemplateOperation::Call { .. }
        | CheckedTemplateOperation::Convert { .. }
        | CheckedTemplateOperation::Tuple(_)
        | CheckedTemplateOperation::Project { .. } => {}
    }

    Ok(())
}

fn validate_array_types(
    node: CheckedTemplateNodeId,
    elements: &[CheckedTemplateNodeId],
    nodes: &[CheckedTemplateNode],
) -> Result<(), CheckedTemplateBuildError> {
    let Some((first, remaining)) = elements.split_first() else {
        return Ok(());
    };

    let expected = referenced_node_type(*first, nodes)?;

    for element in remaining {
        let actual = referenced_node_type(*element, nodes)?;

        if actual != expected {
            return Err(CheckedTemplateBuildError::ArrayElementTypeMismatch {
                node,
                element: *element,
                expected,
                actual,
            });
        }
    }

    Ok(())
}

fn validate_conditional_types(
    node: CheckedTemplateNodeId,
    when_true: CheckedTemplateNodeId,
    when_false: CheckedTemplateNodeId,
    actual: TypeId,
    nodes: &[CheckedTemplateNode],
) -> Result<(), CheckedTemplateBuildError> {
    let true_type = referenced_node_type(when_true, nodes)?;
    let false_type = referenced_node_type(when_false, nodes)?;

    if true_type != false_type {
        return Err(CheckedTemplateBuildError::ConditionalBranchTypeMismatch {
            node,
            when_true: true_type,
            when_false: false_type,
        });
    }

    if true_type != actual {
        return Err(CheckedTemplateBuildError::ConditionalResultTypeMismatch {
            node,
            expected: true_type,
            actual,
        });
    }

    Ok(())
}

fn validate_short_circuit_types(
    node: CheckedTemplateNodeId,
    left: CheckedTemplateNodeId,
    right: CheckedTemplateNodeId,
    actual: TypeId,
    nodes: &[CheckedTemplateNode],
) -> Result<(), CheckedTemplateBuildError> {
    let left_type = referenced_node_type(left, nodes)?;
    let right_type = referenced_node_type(right, nodes)?;

    if left_type != right_type {
        return Err(CheckedTemplateBuildError::ShortCircuitOperandTypeMismatch {
            node,
            left: left_type,
            right: right_type,
        });
    }

    if left_type != actual {
        return Err(CheckedTemplateBuildError::ShortCircuitResultTypeMismatch {
            node,
            expected: left_type,
            actual,
        });
    }

    Ok(())
}

fn input_type(
    input: CheckedTemplateInputId,
    inputs: &[CheckedTemplateInput],
) -> Result<TypeId, CheckedTemplateBuildError> {
    let Some(index) = input.to_index() else {
        return Err(CheckedTemplateBuildError::MissingInput(input));
    };

    inputs
        .get(index)
        .map(CheckedTemplateInput::ty)
        .ok_or(CheckedTemplateBuildError::MissingInput(input))
}

fn temporary_type(
    temporary: CheckedTemplateTemporaryId,
    temporaries: &[CheckedTemplateTemporary],
) -> Result<TypeId, CheckedTemplateBuildError> {
    let Some(index) = temporary.to_index() else {
        return Err(CheckedTemplateBuildError::MissingTemporary(temporary));
    };

    temporaries
        .get(index)
        .map(|temporary| temporary.ty())
        .ok_or(CheckedTemplateBuildError::MissingTemporary(temporary))
}

fn referenced_node_type(
    node: CheckedTemplateNodeId,
    nodes: &[CheckedTemplateNode],
) -> Result<TypeId, CheckedTemplateBuildError> {
    let Some(index) = node.to_index() else {
        return Err(CheckedTemplateBuildError::MissingNode(node));
    };

    nodes
        .get(index)
        .map(CheckedTemplateNode::ty)
        .ok_or(CheckedTemplateBuildError::MissingNode(node))
}

fn validate_input(
    input: CheckedTemplateInputId,
    input_count: usize,
) -> Result<(), CheckedTemplateBuildError> {
    let Some(index) = input.to_index() else {
        return Err(CheckedTemplateBuildError::MissingInput(input));
    };

    if index >= input_count {
        return Err(CheckedTemplateBuildError::MissingInput(input));
    }

    Ok(())
}

fn validate_present_node(
    node: CheckedTemplateNodeId,
    node_count: usize,
) -> Result<(), CheckedTemplateBuildError> {
    let Some(index) = node.to_index() else {
        return Err(CheckedTemplateBuildError::MissingNode(node));
    };

    if index >= node_count {
        return Err(CheckedTemplateBuildError::MissingNode(node));
    }

    Ok(())
}

fn validate_prior_node(
    node: CheckedTemplateNodeId,
    referenced: CheckedTemplateNodeId,
    node_count: usize,
) -> Result<(), CheckedTemplateBuildError> {
    let Some(index) = referenced.to_index() else {
        return Err(CheckedTemplateBuildError::MissingNode(referenced));
    };

    if index >= node_count {
        return Err(CheckedTemplateBuildError::ForwardNodeReference { node, referenced });
    }

    Ok(())
}

fn validate_temporary(
    node: CheckedTemplateNodeId,
    temporary: CheckedTemplateTemporaryId,
    temporaries: &[CheckedTemplateTemporary],
) -> Result<(), CheckedTemplateBuildError> {
    let Some(index) = temporary.to_index() else {
        return Err(CheckedTemplateBuildError::MissingTemporary(temporary));
    };

    let Some(temporary_data) = temporaries.get(index) else {
        return Err(CheckedTemplateBuildError::MissingTemporary(temporary));
    };

    if temporary_data.initializer().raw() >= node.raw() {
        return Err(CheckedTemplateBuildError::UninitializedTemporary { node, temporary });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::thread;

    use bray_symbols::{
        DependencyContractTemplateData, PackageIdentity, SemanticValueStore, SymbolKey, TypeData,
    };

    use super::{CheckedTemplateBuildError, CheckedTemplateBuilder};
    use crate::{
        CheckedTemplateBehavior, CheckedTemplateCompletion, CheckedTemplateInput,
        CheckedTemplateInputId, CheckedTemplateInputKind, CheckedTemplateKind, CheckedTemplateNode,
        CheckedTemplateNodeId, CheckedTemplateOperation, CheckedTemplateShortCircuitKind,
        CheckedTemplateTemporaryId,
    };

    #[test]
    fn all_declaration_owned_categories_publish_portable_templates() {
        for kind in [
            CheckedTemplateKind::RuntimeDefault,
            CheckedTemplateKind::ConstantDefinition,
            CheckedTemplateKind::PredicateDefinition,
            CheckedTemplateKind::GenericConstraint,
            CheckedTemplateKind::CallableContract,
        ] {
            let template = input_template(kind);

            assert_eq!(template.kind(), kind);
            assert_eq!(template.inputs().len(), 1);
            assert_eq!(template.nodes().len(), 1);
            assert_eq!(template.result(), CheckedTemplateNodeId::new(0));
        }
    }

    #[test]
    fn templates_reject_missing_and_forward_references() {
        let (ty, behavior) = semantic_values();

        let mut builder =
            CheckedTemplateBuilder::new(CheckedTemplateKind::PredicateDefinition, behavior);

        assert_eq!(
            builder.push_node(CheckedTemplateNode::new(
                CheckedTemplateOperation::Input(CheckedTemplateInputId::new(0)),
                ty,
            )),
            Err(CheckedTemplateBuildError::MissingInput(
                CheckedTemplateInputId::new(0)
            ))
        );

        assert_eq!(
            builder.push_node(CheckedTemplateNode::new(
                CheckedTemplateOperation::tuple([CheckedTemplateNodeId::new(0)]),
                ty,
            )),
            Err(CheckedTemplateBuildError::ForwardNodeReference {
                node: CheckedTemplateNodeId::new(0),
                referenced: CheckedTemplateNodeId::new(0),
            })
        );

        assert_eq!(
            builder.push_node(CheckedTemplateNode::new(
                CheckedTemplateOperation::Temporary(CheckedTemplateTemporaryId::new(0)),
                ty,
            )),
            Err(CheckedTemplateBuildError::MissingTemporary(
                CheckedTemplateTemporaryId::new(0)
            ))
        );

        assert_eq!(
            builder.push_node(CheckedTemplateNode::new(
                CheckedTemplateOperation::Conditional {
                    condition: CheckedTemplateNodeId::new(0),
                    when_true: CheckedTemplateNodeId::new(1),
                    when_false: CheckedTemplateNodeId::new(2),
                },
                ty,
            )),
            Err(CheckedTemplateBuildError::ForwardNodeReference {
                node: CheckedTemplateNodeId::new(0),
                referenced: CheckedTemplateNodeId::new(0),
            })
        );

        assert_eq!(
            builder.push_node(CheckedTemplateNode::new(
                CheckedTemplateOperation::ShortCircuit {
                    kind: CheckedTemplateShortCircuitKind::And,
                    left: CheckedTemplateNodeId::new(0),
                    right: CheckedTemplateNodeId::new(1),
                },
                ty,
            )),
            Err(CheckedTemplateBuildError::ForwardNodeReference {
                node: CheckedTemplateNodeId::new(0),
                referenced: CheckedTemplateNodeId::new(0),
            })
        );

        assert_eq!(
            builder.finish(
                CheckedTemplateNodeId::new(0),
                CheckedTemplateCompletion::Complete,
            ),
            Err(CheckedTemplateBuildError::MissingNode(
                CheckedTemplateNodeId::new(0)
            ))
        );
    }

    #[test]
    fn templates_reject_ambiguous_contextual_inputs() {
        let (ty, behavior) = semantic_values();

        let mut builder =
            CheckedTemplateBuilder::new(CheckedTemplateKind::RuntimeDefault, behavior);

        let Ok(first) = builder.push_input(CheckedTemplateInput::new(
            CheckedTemplateInputKind::Receiver,
            ty,
        )) else {
            panic!("first receiver input must be valid");
        };

        assert_eq!(
            builder.push_input(CheckedTemplateInput::new(
                CheckedTemplateInputKind::Receiver,
                ty,
            )),
            Err(CheckedTemplateBuildError::DuplicateInput {
                first,
                duplicate: CheckedTemplateInputId::new(1),
            })
        );
    }

    #[test]
    fn control_flow_reference_validation_uses_semantic_child_order() {
        let (ty, behavior) = semantic_values();

        let mut builder =
            CheckedTemplateBuilder::new(CheckedTemplateKind::PredicateDefinition, behavior);

        let condition = push_declaration(&mut builder, ty);

        assert_eq!(
            builder.push_node(CheckedTemplateNode::new(
                CheckedTemplateOperation::Conditional {
                    condition,
                    when_true: CheckedTemplateNodeId::new(1),
                    when_false: CheckedTemplateNodeId::new(2),
                },
                ty,
            )),
            Err(CheckedTemplateBuildError::ForwardNodeReference {
                node: CheckedTemplateNodeId::new(1),
                referenced: CheckedTemplateNodeId::new(1),
            })
        );

        assert_eq!(
            builder.push_node(CheckedTemplateNode::new(
                CheckedTemplateOperation::ShortCircuit {
                    kind: CheckedTemplateShortCircuitKind::And,
                    left: condition,
                    right: CheckedTemplateNodeId::new(1),
                },
                ty,
            )),
            Err(CheckedTemplateBuildError::ForwardNodeReference {
                node: CheckedTemplateNodeId::new(1),
                referenced: CheckedTemplateNodeId::new(1),
            })
        );
    }

    #[test]
    fn recovered_templates_cannot_cross_the_portable_boundary() {
        let (ty, behavior) = semantic_values();

        let mut builder =
            CheckedTemplateBuilder::new(CheckedTemplateKind::ConstantDefinition, behavior);

        let Ok(result) = builder.push_node(CheckedTemplateNode::new(
            CheckedTemplateOperation::Declaration(external_symbol()),
            ty,
        )) else {
            panic!("one declaration node must be valid");
        };

        assert_eq!(
            builder.finish(result, CheckedTemplateCompletion::Recovered),
            Err(CheckedTemplateBuildError::RecoveredTemplate)
        );
    }

    #[test]
    fn template_construction_is_deterministic_across_workers() {
        let (ty, behavior) = semantic_values();

        let behaviors = [behavior.clone(), behavior];

        let [first, second] = behaviors.map(|behavior| {
            thread::spawn(move || {
                input_template_with_behavior(CheckedTemplateKind::RuntimeDefault, ty, behavior)
            })
        });

        let Ok(first) = first.join() else {
            panic!("first template worker must complete");
        };

        let Ok(second) = second.join() else {
            panic!("second template worker must complete");
        };

        assert_eq!(first, second);
    }

    #[test]
    fn initialized_temporaries_retain_type_and_dependencies() {
        let (ty, behavior) = semantic_values();

        let dependencies = behavior.dependency_contract();

        let mut builder =
            CheckedTemplateBuilder::new(CheckedTemplateKind::RuntimeDefault, behavior);

        let Ok(initializer) = builder.push_node(CheckedTemplateNode::new(
            CheckedTemplateOperation::Declaration(external_symbol()),
            ty,
        )) else {
            panic!("temporary initializer must be valid");
        };

        let Ok(temporary) = builder.push_temporary(initializer, ty, dependencies) else {
            panic!("temporary must accept a committed initializer");
        };

        let Ok(result) = builder.push_node(CheckedTemplateNode::new(
            CheckedTemplateOperation::Temporary(temporary),
            ty,
        )) else {
            panic!("initialized temporary must be readable");
        };

        let Ok(template) = builder.finish(result, CheckedTemplateCompletion::Complete) else {
            panic!("complete template must be valid");
        };

        assert_eq!(template.temporaries().len(), 1);
        assert_eq!(template.temporaries()[0].initializer(), initializer);
        assert_eq!(template.temporaries()[0].ty(), ty);

        assert_eq!(
            template.temporaries()[0].dependency_contract(),
            dependencies
        );
    }

    #[test]
    fn conditional_and_short_circuit_operations_retain_lazy_children() {
        let (ty, behavior) = semantic_values();

        let mut builder =
            CheckedTemplateBuilder::new(CheckedTemplateKind::PredicateDefinition, behavior);

        let condition = push_declaration(&mut builder, ty);
        let when_true = push_declaration(&mut builder, ty);
        let when_false = push_declaration(&mut builder, ty);

        let Ok(conditional) = builder.push_node(CheckedTemplateNode::new(
            CheckedTemplateOperation::Conditional {
                condition,
                when_true,
                when_false,
            },
            ty,
        )) else {
            panic!("coherent conditional must be valid");
        };

        let right = push_declaration(&mut builder, ty);

        let Ok(short_circuit) = builder.push_node(CheckedTemplateNode::new(
            CheckedTemplateOperation::ShortCircuit {
                kind: CheckedTemplateShortCircuitKind::Or,
                left: conditional,
                right,
            },
            ty,
        )) else {
            panic!("coherent short-circuit operation must be valid");
        };

        let Ok(template) = builder.finish(short_circuit, CheckedTemplateCompletion::Complete)
        else {
            panic!("complete control-flow template must be valid");
        };

        let Some(conditional_index) = conditional.to_index() else {
            panic!("small conditional ID must be indexable");
        };

        let Some(conditional_node) = template.nodes().get(conditional_index) else {
            panic!("conditional node must be present");
        };

        assert!(matches!(
            conditional_node.operation(),
            CheckedTemplateOperation::Conditional {
                condition: stored_condition,
                when_true: stored_true,
                when_false: stored_false,
            } if *stored_condition == condition
                && *stored_true == when_true
                && *stored_false == when_false
        ));

        let Some(short_circuit_index) = short_circuit.to_index() else {
            panic!("small short-circuit ID must be indexable");
        };

        let Some(short_circuit_node) = template.nodes().get(short_circuit_index) else {
            panic!("short-circuit node must be present");
        };

        assert!(matches!(
            short_circuit_node.operation(),
            CheckedTemplateOperation::ShortCircuit {
                kind: CheckedTemplateShortCircuitKind::Or,
                left,
                right: stored_right,
            } if *left == conditional && *stored_right == right
        ));
    }

    #[test]
    fn input_and_temporary_types_must_match_their_declarations() {
        let (expected, actual, behavior) = semantic_values_with_alternative();

        let dependencies = behavior.dependency_contract();

        let mut builder =
            CheckedTemplateBuilder::new(CheckedTemplateKind::RuntimeDefault, behavior);

        let Ok(input) = builder.push_input(CheckedTemplateInput::new(
            CheckedTemplateInputKind::Receiver,
            expected,
        )) else {
            panic!("receiver input must be valid");
        };

        assert_eq!(
            builder.push_node(CheckedTemplateNode::new(
                CheckedTemplateOperation::Input(input),
                actual,
            )),
            Err(CheckedTemplateBuildError::InputTypeMismatch {
                node: CheckedTemplateNodeId::new(0),
                input,
                expected,
                actual,
            })
        );

        let initializer = push_declaration(&mut builder, expected);

        assert_eq!(
            builder.push_temporary(initializer, actual, dependencies),
            Err(
                CheckedTemplateBuildError::TemporaryInitializerTypeMismatch {
                    initializer,
                    expected,
                    actual,
                }
            )
        );

        let Ok(temporary) = builder.push_temporary(initializer, expected, dependencies) else {
            panic!("matching temporary must be valid");
        };

        assert_eq!(
            builder.push_node(CheckedTemplateNode::new(
                CheckedTemplateOperation::Temporary(temporary),
                actual,
            )),
            Err(CheckedTemplateBuildError::TemporaryTypeMismatch {
                node: CheckedTemplateNodeId::new(1),
                temporary,
                expected,
                actual,
            })
        );
    }

    #[test]
    fn control_flow_and_conversion_types_must_be_locally_coherent() {
        let (expected, actual, behavior) = semantic_values_with_alternative();

        assert_conversion_type_mismatch(expected, actual, behavior.clone());
        assert_conditional_type_mismatches(expected, actual, behavior.clone());
        assert_short_circuit_type_mismatches(expected, actual, behavior.clone());
        assert_array_type_mismatch(expected, actual, behavior);
    }

    #[test]
    fn portable_templates_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<crate::CheckedTemplate>();
        assert_send_sync::<CheckedTemplateBuilder>();
    }

    fn input_template(kind: CheckedTemplateKind) -> crate::CheckedTemplate {
        let (ty, behavior) = semantic_values();

        input_template_with_behavior(kind, ty, behavior)
    }

    fn input_template_with_behavior(
        kind: CheckedTemplateKind,
        ty: bray_symbols::TypeId,
        behavior: CheckedTemplateBehavior,
    ) -> crate::CheckedTemplate {
        let mut builder = CheckedTemplateBuilder::new(kind, behavior);

        let Ok(input) = builder.push_input(CheckedTemplateInput::new(
            CheckedTemplateInputKind::Receiver,
            ty,
        )) else {
            panic!("one input must fit");
        };

        let Ok(result) = builder.push_node(CheckedTemplateNode::new(
            CheckedTemplateOperation::Input(input),
            ty,
        )) else {
            panic!("declared input must be readable");
        };

        let Ok(template) = builder.finish(result, CheckedTemplateCompletion::Complete) else {
            panic!("complete input template must be valid");
        };

        template
    }

    fn semantic_values() -> (bray_symbols::TypeId, CheckedTemplateBehavior) {
        let (ty, _, behavior) = semantic_values_with_alternative();

        (ty, behavior)
    }

    fn semantic_values_with_alternative() -> (
        bray_symbols::TypeId,
        bray_symbols::TypeId,
        CheckedTemplateBehavior,
    ) {
        let Ok(store) = SemanticValueStore::try_new() else {
            panic!("semantic value store identity must be available");
        };

        let Ok(ty) = store.intern_type(TypeData::tuple([])) else {
            panic!("unit tuple type must be valid test data");
        };

        let Ok(alternative) = store.intern_type(TypeData::Nullable(ty)) else {
            panic!("nullable unit type must be valid test data");
        };

        let Ok(dependencies) =
            store.intern_dependency_contract_template(DependencyContractTemplateData::new([]))
        else {
            panic!("empty dependency contract must be valid");
        };

        let behavior = CheckedTemplateBehavior::new(
            [],
            [],
            [],
            crate::CheckedTemplateExecution::new(
                [],
                bray_symbols::CurrentRunCancellation::NotEntered,
            ),
            [],
            dependencies,
            [],
        );

        (ty, alternative, behavior)
    }

    fn push_declaration(
        builder: &mut CheckedTemplateBuilder,
        ty: bray_symbols::TypeId,
    ) -> CheckedTemplateNodeId {
        let Ok(node) = builder.push_node(CheckedTemplateNode::new(
            CheckedTemplateOperation::Declaration(external_symbol()),
            ty,
        )) else {
            panic!("declaration node must be valid");
        };

        node
    }

    fn assert_conversion_type_mismatch(
        expected: bray_symbols::TypeId,
        actual: bray_symbols::TypeId,
        behavior: CheckedTemplateBehavior,
    ) {
        let mut builder =
            CheckedTemplateBuilder::new(CheckedTemplateKind::RuntimeDefault, behavior);

        let value = push_declaration(&mut builder, expected);

        assert_eq!(
            builder.push_node(CheckedTemplateNode::new(
                CheckedTemplateOperation::Convert {
                    value,
                    target: expected,
                },
                actual,
            )),
            Err(CheckedTemplateBuildError::ConversionTypeMismatch {
                node: CheckedTemplateNodeId::new(1),
                expected,
                actual,
            })
        );
    }

    fn assert_conditional_type_mismatches(
        expected: bray_symbols::TypeId,
        actual: bray_symbols::TypeId,
        behavior: CheckedTemplateBehavior,
    ) {
        let mut branch_builder =
            CheckedTemplateBuilder::new(CheckedTemplateKind::PredicateDefinition, behavior.clone());

        let condition = push_declaration(&mut branch_builder, expected);
        let when_true = push_declaration(&mut branch_builder, expected);
        let when_false = push_declaration(&mut branch_builder, actual);

        assert_eq!(
            branch_builder.push_node(CheckedTemplateNode::new(
                CheckedTemplateOperation::Conditional {
                    condition,
                    when_true,
                    when_false,
                },
                expected,
            )),
            Err(CheckedTemplateBuildError::ConditionalBranchTypeMismatch {
                node: CheckedTemplateNodeId::new(3),
                when_true: expected,
                when_false: actual,
            })
        );

        let mut result_builder =
            CheckedTemplateBuilder::new(CheckedTemplateKind::PredicateDefinition, behavior);

        let condition = push_declaration(&mut result_builder, expected);
        let branch = push_declaration(&mut result_builder, expected);

        assert_eq!(
            result_builder.push_node(CheckedTemplateNode::new(
                CheckedTemplateOperation::Conditional {
                    condition,
                    when_true: branch,
                    when_false: branch,
                },
                actual,
            )),
            Err(CheckedTemplateBuildError::ConditionalResultTypeMismatch {
                node: CheckedTemplateNodeId::new(2),
                expected,
                actual,
            })
        );
    }

    fn assert_short_circuit_type_mismatches(
        expected: bray_symbols::TypeId,
        actual: bray_symbols::TypeId,
        behavior: CheckedTemplateBehavior,
    ) {
        let mut operand_builder =
            CheckedTemplateBuilder::new(CheckedTemplateKind::PredicateDefinition, behavior.clone());

        let left = push_declaration(&mut operand_builder, expected);
        let right = push_declaration(&mut operand_builder, actual);

        assert_eq!(
            operand_builder.push_node(CheckedTemplateNode::new(
                CheckedTemplateOperation::ShortCircuit {
                    kind: CheckedTemplateShortCircuitKind::And,
                    left,
                    right,
                },
                expected,
            )),
            Err(CheckedTemplateBuildError::ShortCircuitOperandTypeMismatch {
                node: CheckedTemplateNodeId::new(2),
                left: expected,
                right: actual,
            })
        );

        let mut result_builder =
            CheckedTemplateBuilder::new(CheckedTemplateKind::PredicateDefinition, behavior);

        let operand = push_declaration(&mut result_builder, expected);

        assert_eq!(
            result_builder.push_node(CheckedTemplateNode::new(
                CheckedTemplateOperation::ShortCircuit {
                    kind: CheckedTemplateShortCircuitKind::Or,
                    left: operand,
                    right: operand,
                },
                actual,
            )),
            Err(CheckedTemplateBuildError::ShortCircuitResultTypeMismatch {
                node: CheckedTemplateNodeId::new(1),
                expected,
                actual,
            })
        );
    }

    fn assert_array_type_mismatch(
        expected: bray_symbols::TypeId,
        actual: bray_symbols::TypeId,
        behavior: CheckedTemplateBehavior,
    ) {
        let mut builder =
            CheckedTemplateBuilder::new(CheckedTemplateKind::RuntimeDefault, behavior);

        let first = push_declaration(&mut builder, expected);
        let second = push_declaration(&mut builder, actual);

        assert_eq!(
            builder.push_node(CheckedTemplateNode::new(
                CheckedTemplateOperation::array([first, second]),
                actual,
            )),
            Err(CheckedTemplateBuildError::ArrayElementTypeMismatch {
                node: CheckedTemplateNodeId::new(2),
                element: second,
                expected,
                actual,
            })
        );
    }

    fn external_symbol() -> SymbolKey {
        let Some(package) = PackageIdentity::try_new("example.package") else {
            panic!("test package identity must be valid");
        };

        SymbolKey::external(bray_symbols::ExternalSymbolKey::package(package))
    }
}
