use bray_symbols::{DependencyContractTemplateId, TypeId};

use super::{
    CheckedTemplate, CheckedTemplateBehavior, CheckedTemplateCompletion, CheckedTemplateInput,
    CheckedTemplateInputId, CheckedTemplateKind, CheckedTemplateNode, CheckedTemplateNodeId,
    CheckedTemplateOperation, CheckedTemplateTemporary, CheckedTemplateTemporaryId,
};

/// A typed failure while validating a source-independent checked template.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckedTemplateBuildError {
    /// A template-local identity table cannot represent another entry.
    CapacityExceeded,
    /// Semantic recovery contributed to a template that must be portable and lowerable.
    RecoveredTemplate,
    /// An operation references an input that is not declared by the template.
    MissingInput(CheckedTemplateInputId),
    /// Two inputs declare the same contextual or generic role.
    DuplicateInput {
        /// The first declaration of the input role.
        first: CheckedTemplateInputId,
        /// The repeated declaration of the input role.
        duplicate: CheckedTemplateInputId,
    },
    /// An operation or result references a node outside the template.
    MissingNode(CheckedTemplateNodeId),
    /// An operation references a node that has not yet been evaluated.
    ForwardNodeReference {
        /// The operation containing the invalid reference.
        node: CheckedTemplateNodeId,
        /// The referenced node at or after the operation.
        referenced: CheckedTemplateNodeId,
    },
    /// An operation references a temporary that is not declared by the template.
    MissingTemporary(CheckedTemplateTemporaryId),
    /// A temporary is read before its initializer has been evaluated.
    UninitializedTemporary {
        /// The operation reading the temporary.
        node: CheckedTemplateNodeId,
        /// The temporary whose initializer is not available.
        temporary: CheckedTemplateTemporaryId,
    },
}

/// Task-local validated construction for one portable checked template.
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

    /// Freezes a complete checked template after validating its result.
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
    inputs: &[CheckedTemplateInput],
    nodes: &[CheckedTemplateNode],
    temporaries: &[CheckedTemplateTemporary],
) -> Result<(), CheckedTemplateBuildError> {
    if let CheckedTemplateOperation::Input(input) = operation {
        validate_input(*input, inputs.len())?;
    }

    for referenced in operation.node_references() {
        validate_prior_node(node, *referenced, nodes.len())?;
    }

    if let CheckedTemplateOperation::Temporary(temporary) = operation {
        validate_temporary(node, *temporary, temporaries)?;
    }

    Ok(())
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
        DependencyContractTemplateData, ExternalSymbolKey, PackageIdentity, SemanticValueStore,
        TypeData,
    };

    use super::{CheckedTemplateBuildError, CheckedTemplateBuilder};
    use crate::{
        CheckedTemplateBehavior, CheckedTemplateCompletion, CheckedTemplateInput,
        CheckedTemplateInputId, CheckedTemplateInputKind, CheckedTemplateKind, CheckedTemplateNode,
        CheckedTemplateNodeId, CheckedTemplateOperation, CheckedTemplateTemporaryId,
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
        let Ok(store) = SemanticValueStore::try_new() else {
            panic!("semantic value store identity must be available");
        };

        let Ok(ty) = store.intern_type(TypeData::tuple([])) else {
            panic!("unit tuple type must be valid test data");
        };

        let Ok(dependencies) =
            store.intern_dependency_contract_template(DependencyContractTemplateData::new([]))
        else {
            panic!("empty dependency contract must be valid");
        };

        (
            ty,
            CheckedTemplateBehavior::new([], [], [], [], dependencies, []),
        )
    }

    fn external_symbol() -> ExternalSymbolKey {
        let Some(package) = PackageIdentity::try_new("example.package") else {
            panic!("test package identity must be valid");
        };

        ExternalSymbolKey::package(package)
    }
}
