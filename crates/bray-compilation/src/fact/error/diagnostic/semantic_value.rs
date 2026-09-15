use bray_diagnostics::DiagnosticSemanticValueFailure;

pub const fn diagnostic_semantic_value_failure(
    error: bray_symbols::SemanticValueStoreError,
) -> DiagnosticSemanticValueFailure {
    use bray_symbols::SemanticValueStoreError as Error;

    match error {
        Error::CapacityExhausted { kind } => DiagnosticSemanticValueFailure::CapacityExhausted {
            kind: diagnostic_semantic_value_kind(kind),
        },
    }
}

const fn diagnostic_semantic_value_kind(kind: bray_symbols::SemanticValueKind) -> &'static str {
    use bray_symbols::SemanticValueKind as Kind;

    match kind {
        Kind::Type => "type",
        Kind::ConstantValue => "constant_value",
        Kind::ConstantTerm => "constant_term",
        Kind::GenericSubstitution => "generic_substitution",
        Kind::TraitApplication => "trait_application",
        Kind::CallableInstance => "callable_instance",
        Kind::ImplementationInstance => "implementation_instance",
        Kind::DependencyContractTemplate => "dependency_contract_template",
    }
}
