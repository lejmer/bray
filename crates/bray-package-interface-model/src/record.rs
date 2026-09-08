/// Symbol-owned semantic record category addressable through the record directory.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InterfaceSemanticRecordKind {
    /// Complete source-independent callable signature template.
    CallableSignature,
    /// Ordered generic declaration template.
    GenericDeclaration,
    /// Callable parameter default-template presence.
    CallableParameterDefault,
    /// Validated predicate definition form.
    PredicateDefinition,
    /// Checked type owned by a declaration.
    DeclaredType,
    /// Complete declared type representation contract.
    TypeRepresentation,
    /// Checked generic constraint.
    GenericConstraint,
    /// Complete callable contract set.
    CallableContracts,
    /// Source-independent checked declaration-owned template.
    DeclarationTemplate,
    /// Public implementation subject and applied trait.
    Implementation,
    /// Required target property value.
    TargetProperty,
    /// Required callable ABI.
    Abi,
    /// Required portable runtime ABI and protected-frame compatibility.
    Runtime,
}

impl InterfaceSemanticRecordKind {
    /// Returns the stable machine key for this semantic record category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CallableSignature => "callable_signature",
            Self::GenericDeclaration => "generic_declaration",
            Self::CallableParameterDefault => "callable_parameter_default",
            Self::PredicateDefinition => "predicate_definition",
            Self::DeclaredType => "declared_type",
            Self::TypeRepresentation => "type_representation",
            Self::GenericConstraint => "generic_constraint",
            Self::CallableContracts => "callable_contracts",
            Self::DeclarationTemplate => "declaration_template",
            Self::Implementation => "implementation",
            Self::TargetProperty => "target_property",
            Self::Abi => "abi",
            Self::Runtime => "runtime",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::InterfaceSemanticRecordKind;

    #[test]
    fn semantic_record_keys_are_stable_and_distinct() {
        let records = [
            (
                InterfaceSemanticRecordKind::CallableSignature,
                "callable_signature",
            ),
            (
                InterfaceSemanticRecordKind::GenericDeclaration,
                "generic_declaration",
            ),
            (
                InterfaceSemanticRecordKind::CallableParameterDefault,
                "callable_parameter_default",
            ),
            (
                InterfaceSemanticRecordKind::PredicateDefinition,
                "predicate_definition",
            ),
            (InterfaceSemanticRecordKind::DeclaredType, "declared_type"),
            (
                InterfaceSemanticRecordKind::TypeRepresentation,
                "type_representation",
            ),
            (
                InterfaceSemanticRecordKind::GenericConstraint,
                "generic_constraint",
            ),
            (
                InterfaceSemanticRecordKind::CallableContracts,
                "callable_contracts",
            ),
            (
                InterfaceSemanticRecordKind::DeclarationTemplate,
                "declaration_template",
            ),
            (
                InterfaceSemanticRecordKind::Implementation,
                "implementation",
            ),
            (
                InterfaceSemanticRecordKind::TargetProperty,
                "target_property",
            ),
            (InterfaceSemanticRecordKind::Abi, "abi"),
            (InterfaceSemanticRecordKind::Runtime, "runtime"),
        ];

        for (kind, key) in records {
            assert_eq!(kind.as_str(), key);
        }

        assert_eq!(
            records
                .into_iter()
                .map(|(kind, _)| kind.as_str())
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            records.len(),
        );
    }
}
