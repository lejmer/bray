use super::super::source::format_english_quoted_text;
use bray_diagnostics::{DiagnosticInterfaceSymbolIdentity, DiagnosticInterfaceSynthesizedIdentity};

pub(crate) fn format_english_interface_symbol_graph_problem(
    problem: &bray_diagnostics::DiagnosticInterfaceSymbolGraphProblem,
) -> String {
    use bray_diagnostics::DiagnosticInterfaceSymbolGraphProblem as Problem;

    match problem {
        Problem::DuplicateInterface(interface) => {
            format!("loaded interface {interface} is present more than once")
        }
        Problem::DuplicatePackage(package) => format!(
            "package {} is provided by more than one loaded interface",
            format_english_quoted_text(package)
        ),
        Problem::SymbolCapacityExceeded { actual, maximum } => format!(
            "imported symbols require {actual} compilation identities, but the compiler can represent at most {maximum}"
        ),
        Problem::DuplicateExternalIdentity(identity) => format!(
            "external symbol identity {} is defined more than once",
            format_english_interface_symbol_identity(identity)
        ),
        Problem::RelationshipSymbolOutOfBounds { interface, symbol } => format!(
            "relationship in loaded interface {interface} references missing symbol record {symbol}"
        ),
        Problem::InvalidRelationshipKinds {
            relationship,
            owner,
            member,
        } => format!(
            "{} relationship cannot connect a {} owner to a {} member",
            format_english_interface_relationship_kind(*relationship),
            format_english_interface_symbol_kind(*owner),
            format_english_interface_symbol_kind(*member)
        ),
        Problem::RelationshipContainmentMismatch { owner, member } => format!(
            "relationship from symbol {owner} to symbol {member} disagrees with the member identity"
        ),
        Problem::NonCanonicalRelationshipOrdinal {
            relationship,
            owner,
            expected,
            actual,
        } => format!(
            "{} relationship for owner {owner} has ordinal {actual}, but ordinal {expected} is required",
            format_english_interface_relationship_kind(*relationship)
        ),
        Problem::MissingContainment { symbol, kind } => format!(
            "{} symbol {symbol} has no unique containment relationship",
            format_english_interface_symbol_kind(*kind)
        ),
        Problem::DuplicateContainment { symbol, kind } => format!(
            "{} symbol {symbol} has more than one containment relationship",
            format_english_interface_symbol_kind(*kind)
        ),
        Problem::LookupOwnerOutOfBounds { interface, owner } => format!(
            "lookup in loaded interface {interface} references missing owner record {owner}"
        ),
        Problem::InvalidLookupOwner { owner, kind } => format!(
            "{} symbol {owner} cannot own an exported name lookup",
            format_english_interface_symbol_kind(*kind)
        ),
        Problem::MissingLookupTarget(identity) => format!(
            "exported lookup target {} is not defined by the loaded interfaces",
            format_english_interface_symbol_identity(identity)
        ),
        Problem::DuplicateLookupName { owner, name } => format!(
            "lookup owner {owner} exports name {} more than once",
            format_english_quoted_text(name)
        ),
        Problem::UnsupportedSymbolKind(kind) => format!(
            "{} cannot be represented as an imported symbol",
            format_english_interface_symbol_kind(*kind)
        ),
        Problem::InvalidRecordRelationships { symbol, kind } => format!(
            "relationships for {} symbol {symbol} do not form a valid record",
            format_english_interface_symbol_kind(*kind)
        ),
    }
}

pub(crate) fn format_english_interface_semantic_problem(
    problem: &bray_diagnostics::DiagnosticInterfaceSemanticProblem,
) -> String {
    use bray_diagnostics::DiagnosticInterfaceSemanticProblem as Problem;

    match problem {
        Problem::UnresolvedSymbol(reference) => format!(
            "semantic content references unavailable {}",
            format_english_interface_symbol_reference(reference)
        ),
        Problem::InvalidSymbolKind(reference) => format!(
            "semantic content uses {} as an incompatible declaration category",
            format_english_interface_symbol_reference(reference)
        ),
        Problem::UnresolvedValueGraph => {
            "semantic values contain an unresolved dependency cycle".to_owned()
        }
        Problem::SemanticContent(problem) => format_english_semantic_content_problem(problem),
        Problem::InvalidTemplate(problem) => format_english_checked_template_problem(problem),
        Problem::InvalidSupportEntity(entity) => {
            format!("semantic content references missing private support entity {entity}")
        }
    }
}

fn format_english_interface_symbol_reference(
    reference: &bray_diagnostics::DiagnosticInterfaceSymbolReference,
) -> String {
    use bray_diagnostics::DiagnosticInterfaceSymbolReference as Reference;

    match reference {
        Reference::Local(symbol) => format!("local symbol record {symbol}"),
        Reference::Dependency {
            dependency,
            identity,
        } => format!(
            "symbol {} from dependency slot {dependency}",
            format_english_interface_symbol_identity(identity)
        ),
        Reference::CompilerKnown(identity) => format!(
            "compiler-provided declaration {}",
            format_english_interface_symbol_identity(identity)
        ),
    }
}

pub(crate) fn format_english_interface_symbol_identity(
    identity: &DiagnosticInterfaceSymbolIdentity,
) -> String {
    format_english_quoted_text(&interface_symbol_identity_text(identity))
}

pub(crate) fn interface_symbol_identity_text(
    identity: &DiagnosticInterfaceSymbolIdentity,
) -> String {
    use bray_diagnostics::DiagnosticInterfaceDeclarationIdentity as DeclarationIdentity;
    use bray_diagnostics::DiagnosticInterfaceSymbolIdentity as Identity;

    match identity {
        Identity::CompilerKnownEnvironment => "compiler environment".to_owned(),
        Identity::Package(package) => package.clone(),
        Identity::Module { owner, path } => {
            format!(
                "{}::{}",
                interface_symbol_identity_text(owner),
                path.join("::")
            )
        }
        Identity::CompilerKnownDeclaration { key, kind } => format!(
            "compiler {} {}",
            format_english_interface_symbol_kind(*kind),
            key
        ),
        Identity::SourceDeclaration {
            owner,
            kind,
            declaration,
        } => format!(
            "{}::{} declaration {declaration}",
            interface_symbol_identity_text(owner),
            format_english_interface_symbol_kind(*kind)
        ),
        Identity::Declaration {
            owner,
            kind,
            identity,
        } => {
            let declaration = match identity {
                DeclarationIdentity::Name(name) => name.clone(),
                DeclarationIdentity::Ordinal(ordinal) => format!("declaration {ordinal}"),
            };

            format!(
                "{}::{} {declaration}",
                interface_symbol_identity_text(owner),
                format_english_interface_symbol_kind(*kind)
            )
        }
        Identity::Synthesized { owner, identity } => format!(
            "{}::{}",
            interface_symbol_identity_text(owner),
            format_english_interface_synthesized_identity(*identity)
        ),
    }
}

fn format_english_interface_synthesized_identity(
    identity: DiagnosticInterfaceSynthesizedIdentity,
) -> String {
    use DiagnosticInterfaceSynthesizedIdentity as Identity;

    match identity {
        Identity::ReceiverParameter => "receiver parameter".to_owned(),
        Identity::DeclaredGenericTypeParameter(ordinal) => {
            format!("generic type parameter {ordinal}")
        }
        Identity::DeclaredGenericConstParameter(ordinal) => {
            format!("generic constant parameter {ordinal}")
        }
        Identity::CallableParameter(ordinal) => format!("callable parameter {ordinal}"),
        Identity::PredicateParameter(ordinal) => format!("predicate parameter {ordinal}"),
        Identity::InferredImplementationTypeParameter(ordinal) => {
            format!("inferred implementation type parameter {ordinal}")
        }
        Identity::InferredImplementationConstParameter(ordinal) => {
            format!("inferred implementation constant parameter {ordinal}")
        }
        Identity::CallableParameterDefaultProvider => {
            "callable parameter default provider".to_owned()
        }
        Identity::StructFieldDefaultProvider => "structure field default provider".to_owned(),
        Identity::UnionPayloadDefaultProvider => "union payload default provider".to_owned(),
    }
}

fn format_english_semantic_content_problem(
    problem: &bray_diagnostics::DiagnosticSemanticContentProblem,
) -> String {
    use bray_diagnostics::DiagnosticSemanticContentProblem as Problem;

    match problem {
        Problem::ForeignId { expected, actual } => format!(
            "semantic value content identity is {actual}, but identity {expected} is required"
        ),
        Problem::UnknownId { value_kind } => format!(
            "{} identity does not address a stored value",
            format_english_semantic_value_kind(*value_kind)
        ),
        Problem::CapacityExhausted { value_kind } => format!(
            "the {} value table cannot represent another entry",
            format_english_semantic_value_kind(*value_kind)
        ),
        Problem::GenericOwnerMismatch { expected, actual } => format!(
            "generic substitution belongs to declaration {actual}, but declaration {expected} is required"
        ),
        Problem::OpenSubstitution => {
            "an unresolved generic substitution was required to be concrete".to_owned()
        }
    }
}

fn format_english_checked_template_problem(
    problem: &bray_diagnostics::DiagnosticCheckedTemplateProblem,
) -> String {
    use bray_diagnostics::DiagnosticCheckedTemplateProblem as Problem;

    match problem {
        Problem::CapacityExceeded => {
            "the executable template cannot represent another entry".to_owned()
        }
        Problem::RecoveredTemplate => {
            "the executable template contains recovered semantic content".to_owned()
        }
        Problem::MissingInput(input) => {
            format!("executable template references missing input {input}")
        }
        Problem::DuplicateInput { first, duplicate } => {
            format!("template input {duplicate} duplicates the role declared by input {first}")
        }
        Problem::InputTypeMismatch {
            node,
            input,
            expected_type,
            actual_type,
        } => format!(
            "template node {node} reads input {input} as type {actual_type}, but type {expected_type} is declared"
        ),
        Problem::MissingNode(node) => format!("template references missing node {node}"),
        Problem::ForwardNodeReference { node, referenced } => {
            format!("template node {node} references node {referenced} before it is available")
        }
        Problem::MissingTemporary(temporary) => {
            format!("template references missing temporary {temporary}")
        }
        Problem::UninitializedTemporary { node, temporary } => {
            format!("template node {node} reads temporary {temporary} before initialization")
        }
        Problem::TemporaryInitializerTypeMismatch {
            initializer,
            expected_type,
            actual_type,
        } => format!(
            "temporary initializer {initializer} produces type {actual_type}, but type {expected_type} is declared"
        ),
        Problem::TemporaryTypeMismatch {
            node,
            temporary,
            expected_type,
            actual_type,
        } => format!(
            "template node {node} reads temporary {temporary} as type {actual_type}, but it stores type {expected_type}"
        ),
        Problem::ConversionTypeMismatch {
            node,
            expected_type,
            actual_type,
        } => format!(
            "conversion node {node} produces type {actual_type}, but its destination is type {expected_type}"
        ),
        Problem::ConditionalBranchTypeMismatch {
            node,
            when_true_type,
            when_false_type,
        } => format!(
            "conditional node {node} has branch types {when_true_type} and {when_false_type}"
        ),
        Problem::ConditionalResultTypeMismatch {
            node,
            expected_type,
            actual_type,
        } => format!(
            "conditional node {node} produces type {actual_type}, but its branches produce type {expected_type}"
        ),
        Problem::ShortCircuitOperandTypeMismatch {
            node,
            left_type,
            right_type,
        } => format!("short-circuit node {node} has operand types {left_type} and {right_type}"),
        Problem::ShortCircuitResultTypeMismatch {
            node,
            expected_type,
            actual_type,
        } => format!(
            "short-circuit node {node} produces type {actual_type}, but its operands produce type {expected_type}"
        ),
        Problem::ArrayElementTypeMismatch {
            node,
            element,
            expected_type,
            actual_type,
        } => format!(
            "array node {node} has element {element} of type {actual_type}, but type {expected_type} is required"
        ),
    }
}

const fn format_english_interface_relationship_kind(
    kind: bray_diagnostics::DiagnosticInterfaceRelationshipKind,
) -> &'static str {
    use bray_diagnostics::DiagnosticInterfaceRelationshipKind as Kind;

    match kind {
        Kind::PackageModule => "package-module",
        Kind::ModuleMember => "module-member",
        Kind::TypeMember => "type-member",
        Kind::TraitMember => "trait-member",
        Kind::ImplementationMember => "implementation-member",
        Kind::StructField => "structure-field",
        Kind::UnionVariant => "union-variant",
        Kind::UnionPayloadField => "union-payload-field",
        Kind::GenericParameter => "generic-parameter",
        Kind::CallableParameter => "callable-parameter",
        Kind::PredicateParameter => "predicate-parameter",
        Kind::OverloadArm => "overload-arm",
        Kind::ImplementationFulfillment => "implementation-fulfillment",
        Kind::DefaultProvider => "default-provider",
    }
}

const fn format_english_semantic_value_kind(
    kind: bray_diagnostics::DiagnosticSemanticValueKind,
) -> &'static str {
    use bray_diagnostics::DiagnosticSemanticValueKind as Kind;

    match kind {
        Kind::Type => "type",
        Kind::ConstantValue => "constant value",
        Kind::ConstantTerm => "constant term",
        Kind::GenericSubstitution => "generic substitution",
        Kind::TraitApplication => "trait application",
        Kind::CallableInstance => "callable instance",
        Kind::ImplementationInstance => "implementation instance",
        Kind::DependencyContractTemplate => "dependency-contract template",
    }
}

pub(crate) const fn format_english_interface_symbol_kind(
    kind: bray_diagnostics::DiagnosticInterfaceSymbolKind,
) -> &'static str {
    use bray_diagnostics::DiagnosticInterfaceSymbolKind as Kind;

    match kind {
        Kind::CompilerKnownEnvironment => "compiler-provided environment",
        Kind::Package => "package",
        Kind::Module => "module",
        Kind::TrustedCapability => "trusted capability",
        Kind::Constant => "constant",
        Kind::Static => "static",
        Kind::Function => "function",
        Kind::Predicate => "predicate",
        Kind::CallableContract => "callable contract",
        Kind::CallableOverload => "callable overload",
        Kind::ImplementationOverload => "implementation overload",
        Kind::Struct => "structure",
        Kind::Union => "union",
        Kind::Trait => "trait",
        Kind::InherentImplementation => "inherent implementation",
        Kind::UnnamedTraitImplementation => "unnamed trait implementation",
        Kind::NamedTraitImplementation => "named trait implementation",
        Kind::StructField => "structure field",
        Kind::UnionVariant => "union variant",
        Kind::UnionPayloadField => "union payload field",
        Kind::TypeCallableMember => "type callable member",
        Kind::Constructor => "constructor",
        Kind::Finalizer => "finalizer",
        Kind::Destructor => "destructor",
        Kind::ScopeEnter => "scope-enter operation",
        Kind::ScopeExit => "scope-exit operation",
        Kind::InherentTypeMember => "inherent type-valued member",
        Kind::TraitCallableMember => "trait callable requirement",
        Kind::TraitConstantMember => "trait constant requirement",
        Kind::TraitTypeMember => "trait type-valued requirement",
        Kind::TraitPredicateMember => "trait predicate requirement",
        Kind::TraitFinalizerRequirement => "trait finalizer requirement",
        Kind::TraitDestructorRequirement => "trait destructor requirement",
        Kind::TraitScopeEnterRequirement => "trait scope-enter requirement",
        Kind::TraitScopeExitRequirement => "trait scope-exit requirement",
        Kind::TraitCallableFulfillment => "callable trait fulfillment",
        Kind::TraitConstantFulfillment => "constant trait fulfillment",
        Kind::TraitTypeFulfillment => "type-valued trait fulfillment",
        Kind::TraitPredicateFulfillment => "predicate trait fulfillment",
        Kind::TraitScopeEnterFulfillment => "scope-enter trait fulfillment",
        Kind::TraitScopeExitFulfillment => "scope-exit trait fulfillment",
        Kind::GenericTypeParameter => "generic type parameter",
        Kind::GenericConstParameter => "generic constant parameter",
        Kind::CallableParameter => "callable parameter",
        Kind::PredicateParameter => "predicate parameter",
        Kind::ReceiverParameter => "receiver parameter",
        Kind::CallableParameterDefaultProvider => "callable-parameter default provider",
        Kind::StructFieldDefaultProvider => "structure-field default provider",
        Kind::UnionPayloadDefaultProvider => "union-payload default provider",
        Kind::LocalBinding => "local binding",
        Kind::LocalConstant => "local constant",
        Kind::AnonymousCallable => "anonymous callable",
        Kind::AnonymousCallableParameter => "anonymous-callable parameter",
        Kind::PostconditionResult => "postcondition result",
    }
}
