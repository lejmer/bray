use bray_binder::SemanticUnitContextError;
use bray_checker::CheckerInfrastructureError;
use bray_diagnostics::{
    DiagnosticBindingFailure, DiagnosticCheckerFailure, DiagnosticSemanticValueFailure,
};
use bray_lowering::{LoweringError, LoweringInputError};
use bray_source::SourceSpan;

use super::CompilationFactKey;

/// One detected cycle in the compilation fact dependency graph.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct FactCycle {
    facts: Box<[CompilationFactKey]>,
}

impl FactCycle {
    pub(crate) fn new(facts: impl Into<Box<[CompilationFactKey]>>) -> Self {
        Self {
            facts: canonical_cycle(facts.into()),
        }
    }

    pub(crate) fn facts(&self) -> &[CompilationFactKey] {
        &self.facts
    }
}

fn canonical_cycle(facts: Box<[CompilationFactKey]>) -> Box<[CompilationFactKey]> {
    let mut facts = facts.into_vec();

    let Some(closing) = facts.last().cloned() else {
        return facts.into_boxed_slice();
    };

    let Some(start) = facts[..facts.len().saturating_sub(1)]
        .iter()
        .position(|fact| fact == &closing)
    else {
        return facts.into_boxed_slice();
    };

    facts.drain(..start);

    let cycle_len = facts.len().saturating_sub(1);

    let Some((canonical_start, _)) = facts[..cycle_len]
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| left.cmp(right))
    else {
        return facts.into_boxed_slice();
    };

    let mut canonical = facts[..cycle_len]
        .iter()
        .cycle()
        .skip(canonical_start)
        .take(cycle_len)
        .cloned()
        .collect::<Vec<_>>();

    if let Some(first) = canonical.first().cloned() {
        canonical.push(first);
    }

    canonical.into_boxed_slice()
}

#[cfg(test)]
mod tests {
    use bray_source::SourceId;

    use super::{CompilationFactKey, FactCycle};

    #[test]
    fn cycle_paths_remove_prefixes_and_use_a_canonical_start() {
        let syntax = CompilationFactKey::SyntaxTree;
        let declaration = CompilationFactKey::DeclarationTable;
        let prefix = CompilationFactKey::SourceUnitSyntax(SourceId::new(0));

        let cycle = FactCycle::new([prefix, syntax.clone(), declaration.clone(), syntax.clone()]);

        assert_eq!(cycle.facts(), &[declaration.clone(), syntax, declaration]);
    }
}

/// A compiler-owned lowering failure and the Bray source construct being compiled.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct LocatedLoweringFailure<E> {
    cause: E,
    source: SourceSpan,
}

impl<E> LocatedLoweringFailure<E> {
    pub(crate) const fn new(cause: E, source: SourceSpan) -> Self {
        Self { cause, source }
    }

    /// Returns the exact compiler contract failure.
    pub const fn cause(&self) -> &E {
        &self.cause
    }

    /// Returns the closest Bray source construct affected by the failure.
    pub const fn source(&self) -> SourceSpan {
        self.source
    }
}

/// An outer compiler-query outcome that must not be represented as a source diagnostic.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum FactQueryError {
    /// The requesting operation was cancelled before completion.
    Cancelled,
    /// Evaluation encountered a same-worker or cross-worker dependency cycle.
    Cycle(FactCycle),
    /// The fact request could not complete because compiler coordination failed.
    InfrastructureFailure,
    /// Name binding could not obtain a required semantic dependency.
    BindingDependencyUnavailable,
    /// Binding one semantic unit violated a typed binding contract.
    Binding(bray_binder::BoundUnitBindingError),
    /// A selected constant callable has no body available for durable evaluation.
    ConstantCallableBodyUnavailable,
    /// A selected constant callable body has no evaluable result expression.
    ConstantCallableRootUnavailable,
    /// The atomic initializer argument has no available compile-time value.
    AtomicInitializerArgumentUnavailable,
    /// The atomic initializer result cannot be retained as a compile-time value.
    AtomicInitializerResultUnavailable,
    /// The uninitialized-storage initializer result cannot be retained as a compile-time value.
    UninitInitializerResultUnavailable,
    /// An imported native operation does not match its compiled definition.
    ImportedExecutableTemplateMismatch,
    /// Semantic-context construction found an inconsistent bound unit.
    SemanticUnitContext(SemanticUnitContextError),
    /// Semantic checking could not complete because a typed dependency was unavailable.
    CheckerInfrastructure(CheckerInfrastructureError),
    /// Checked lowering inputs violated the lowering boundary contract.
    LoweringInput(LocatedLoweringFailure<LoweringInputError>),
    /// MIR lowering violated a checked semantic or MIR construction contract.
    Lowering(LocatedLoweringFailure<LoweringError>),
}

impl From<std::convert::Infallible> for FactQueryError {
    fn from(error: std::convert::Infallible) -> Self {
        match error {}
    }
}

impl From<CheckerInfrastructureError> for FactQueryError {
    fn from(error: CheckerInfrastructureError) -> Self {
        Self::CheckerInfrastructure(error)
    }
}

impl<Upstream> From<bray_checker::CheckerQueryError<Upstream>> for FactQueryError
where
    Upstream: Into<Self>,
{
    fn from(error: bray_checker::CheckerQueryError<Upstream>) -> Self {
        match error {
            bray_checker::CheckerQueryError::Cancelled => Self::Cancelled,
            bray_checker::CheckerQueryError::Infrastructure(error) => {
                Self::CheckerInfrastructure(error)
            }
            bray_checker::CheckerQueryError::Upstream(error) => error.into(),
        }
    }
}

pub(crate) fn diagnostic_binding_failure(
    error: &bray_binder::BoundUnitBindingError,
) -> DiagnosticBindingFailure {
    use bray_binder::BoundUnitBindingError as Error;

    match error {
        Error::Cancelled | Error::CheckerInfrastructure(_) => {
            unreachable!("fact binding failures contain only binding-owned causes")
        }
        Error::Upstream(error) => match *error {},
        Error::InvalidUnitKey => DiagnosticBindingFailure::InvalidUnitKey,
        Error::MissingSyntax => DiagnosticBindingFailure::MissingSyntax,
        Error::MissingOwner => DiagnosticBindingFailure::MissingOwner,
        Error::MissingModule => DiagnosticBindingFailure::MissingModule,
        Error::SemanticValue(error) => {
            DiagnosticBindingFailure::SemanticValue(diagnostic_semantic_value_failure(*error))
        }
        Error::Construction => DiagnosticBindingFailure::Construction,
        Error::Binding => DiagnosticBindingFailure::Binding,
        Error::Assembly => DiagnosticBindingFailure::Assembly,
    }
}

const fn diagnostic_semantic_value_failure(
    error: bray_symbols::SemanticValueStoreError,
) -> DiagnosticSemanticValueFailure {
    use bray_symbols::SemanticValueStoreError as Error;

    match error {
        Error::ForeignId { .. } => DiagnosticSemanticValueFailure::ForeignId,
        Error::UnknownId { .. } => DiagnosticSemanticValueFailure::UnknownId,
        Error::CapacityExhausted { .. } => DiagnosticSemanticValueFailure::CapacityExhausted,
        Error::GenericOwnerMismatch { .. } => DiagnosticSemanticValueFailure::GenericOwnerMismatch,
        Error::OpenSubstitution => DiagnosticSemanticValueFailure::OpenSubstitution,
    }
}

pub(crate) fn diagnostic_checker_failure(
    error: CheckerInfrastructureError,
) -> DiagnosticCheckerFailure {
    use CheckerInfrastructureError as Error;

    match error {
        Error::MissingSource { source_id } => DiagnosticCheckerFailure::MissingSource { source_id },
        Error::SourceVersionMismatch {
            source_id,
            expected,
            actual,
        } => DiagnosticCheckerFailure::SourceVersionMismatch {
            source_id,
            expected,
            actual,
        },
        Error::InvalidSourceRange { span } => DiagnosticCheckerFailure::InvalidSourceRange { span },
        Error::SemanticQueryUnavailable { symbol, kind } => {
            DiagnosticCheckerFailure::SemanticQueryUnavailable {
                symbol: bray_diagnostics::DiagnosticCheckerSymbol::new(
                    symbol.kind().as_str(),
                    symbol.symbol_id().raw(),
                ),
                query: diagnostic_symbol_query_kind(kind),
            }
        }
        Error::SemanticValueUnavailable => DiagnosticCheckerFailure::SemanticValueUnavailable,
        Error::AtomicRepresentationTypeUnavailable => {
            DiagnosticCheckerFailure::AtomicRepresentationTypeUnavailable
        }
        Error::AtomicRepresentationArgumentsUnavailable => {
            DiagnosticCheckerFailure::AtomicRepresentationArgumentsUnavailable
        }
        Error::AtomicInitializerArgumentUnavailable => {
            DiagnosticCheckerFailure::AtomicInitializerArgumentUnavailable
        }
        Error::AtomicInitializerResultUnavailable => {
            DiagnosticCheckerFailure::AtomicInitializerResultUnavailable
        }
        Error::UninitInitializerResultUnavailable => {
            DiagnosticCheckerFailure::UninitInitializerResultUnavailable
        }
        Error::ImportedExecutableTemplateMismatch => {
            DiagnosticCheckerFailure::ImportedExecutableTemplateMismatch
        }
        Error::CompilerKnownRepresentationUnavailable { role } => {
            DiagnosticCheckerFailure::CompilerKnownRepresentationUnavailable(role.as_str())
        }
        Error::InvalidExpressionTypeInput { expression } => {
            DiagnosticCheckerFailure::InvalidExpressionTypeInput {
                expression: diagnostic_bound_node(expression.into()),
            }
        }
        Error::InvalidSemanticSelectionInput => {
            DiagnosticCheckerFailure::InvalidSemanticSelectionInput
        }
        Error::InvalidLiteralValueInput => DiagnosticCheckerFailure::InvalidLiteralValueInput,
        Error::InvalidConstantEvaluationInput => {
            DiagnosticCheckerFailure::InvalidConstantEvaluationInput
        }
        Error::InvalidPatternCheckInput => DiagnosticCheckerFailure::InvalidPatternCheckInput,
        Error::InvalidStoragePlan => DiagnosticCheckerFailure::InvalidStoragePlan,
        Error::InvalidLiveness => DiagnosticCheckerFailure::InvalidLiveness,
        Error::InvalidRefinementInput => DiagnosticCheckerFailure::InvalidRefinementInput,
        Error::RefinementCapacityUnrepresentable => {
            DiagnosticCheckerFailure::RefinementCapacityUnrepresentable
        }
        Error::RefinementStorageUnavailable => {
            DiagnosticCheckerFailure::RefinementStorageUnavailable
        }
        Error::StorageFlow(failure) => DiagnosticCheckerFailure::StorageFlow(
            diagnostic_storage_flow_failure(failure),
        ),
        Error::InvalidStorageOperation {
            expression,
            access,
            status,
        } => DiagnosticCheckerFailure::InvalidStorageOperation {
            expression: diagnostic_bound_node(expression.into()),
            access: access.ordinal(),
            status: diagnostic_storage_operation_status(status),
        },
        Error::InvalidBodySemantics => DiagnosticCheckerFailure::InvalidBodySemantics,
        Error::InvalidBoundNode { node } => DiagnosticCheckerFailure::InvalidBoundNode {
            node: diagnostic_bound_node(node),
        },
        Error::ExpressionTypeCapacityExceeded => {
            DiagnosticCheckerFailure::ExpressionTypeCapacityExceeded
        }
        Error::InvalidUnitView(error) => DiagnosticCheckerFailure::InvalidUnitView(match error {
            bray_checker::CheckerUnitViewError::SemanticContextMismatch => {
                "semantic_context_mismatch"
            }
        }),
    }
}

fn diagnostic_storage_flow_failure(
    failure: bray_checker::CheckerStorageFlowFailure,
) -> bray_diagnostics::DiagnosticStorageFlowFailure {
    use bray_checker::CheckerStorageFlowFailure as Failure;
    use bray_diagnostics::DiagnosticStorageFlowFailure as DiagnosticFailure;

    match failure {
        Failure::IncompatibleInput {
            input,
            expected_unit,
            expected_kind,
            actual_unit,
            actual_kind,
        } => DiagnosticFailure::IncompatibleInput {
            input: diagnostic_storage_flow_input(input),
            expected_unit: expected_unit.raw(),
            expected_kind: expected_kind.as_str(),
            actual_unit: actual_unit.raw(),
            actual_kind: actual_kind.as_str(),
        },
        Failure::FlowConstruction(error) => {
            DiagnosticFailure::FlowConstruction(match error {
                bray_bound_tree::StorageFlowBuildError::ForeignUnit => "foreign_unit",
                bray_bound_tree::StorageFlowBuildError::DuplicateSuspension => {
                    "duplicate_suspension"
                }
                bray_bound_tree::StorageFlowBuildError::DuplicateMemoryOperation => {
                    "duplicate_memory_operation"
                }
            })
        }
        Failure::ForeignDependencyContract => DiagnosticFailure::ForeignDependencyContract,
        Failure::DependencyContractsConstruction(error) => {
            DiagnosticFailure::DependencyContractsConstruction(match error {
                bray_bound_tree::DependencyContractsBuildError::ForeignStoragePlan => {
                    "foreign_storage_plan"
                }
                bray_bound_tree::DependencyContractsBuildError::InvalidExpression => {
                    "invalid_expression"
                }
                bray_bound_tree::DependencyContractsBuildError::InvalidAccess => "invalid_access",
                bray_bound_tree::DependencyContractsBuildError::InvalidBorrow => "invalid_borrow",
                bray_bound_tree::DependencyContractsBuildError::ForeignContract => {
                    "foreign_contract"
                }
                bray_bound_tree::DependencyContractsBuildError::ContractCapacityExceeded => {
                    "contract_capacity_exceeded"
                }
            })
        }
        Failure::AsyncConstruction(error) => DiagnosticFailure::AsyncConstruction(match error {
            bray_bound_tree::AsyncAnalysisBuildError::ForeignUnit => "foreign_unit",
        }),
        Failure::MissingAwaitDependencyContract { expression } => {
            DiagnosticFailure::MissingAwaitDependencyContract {
                expression: diagnostic_bound_node(expression.into()),
            }
        }
        Failure::MissingDependencyContract {
            expression,
            contract,
        } => DiagnosticFailure::MissingDependencyContract {
            expression: diagnostic_bound_node(expression.into()),
            contract_unit: contract.unit().raw(),
            contract: contract.ordinal(),
        },
        Failure::CallableParameterCountMismatch {
            callable,
            signature_parameters,
            type_parameters,
        } => DiagnosticFailure::CallableParameterCountMismatch {
            callable: diagnostic_checker_symbol(callable),
            signature_parameters,
            type_parameters,
        },
        Failure::CallableTypeNotCallable { callable } => {
            DiagnosticFailure::CallableTypeNotCallable {
                callable: diagnostic_checker_symbol(callable),
            }
        }
        Failure::MissingBorrowCapability { borrow } => {
            DiagnosticFailure::MissingBorrowCapability {
                unit: borrow.unit().raw(),
                borrow: borrow.ordinal(),
            }
        }
        Failure::MissingExitOrigin { exit } => DiagnosticFailure::MissingExitOrigin {
            exit: diagnostic_bound_node(exit),
        },
        Failure::MissingBlock { block } => DiagnosticFailure::MissingBlock {
            block: diagnostic_bound_node(block.into()),
        },
        Failure::MissingStorageAccess { access } => DiagnosticFailure::MissingStorageAccess {
            unit: access.unit().raw(),
            access: access.ordinal(),
        },
        Failure::MissingStorageIdentity { identity } => {
            DiagnosticFailure::MissingStorageIdentity {
                unit: identity.unit().raw(),
                identity: identity.ordinal(),
            }
        }
        Failure::MissingStorageSymbolName { symbol } => {
            DiagnosticFailure::MissingStorageSymbolName {
                symbol: diagnostic_checker_symbol(symbol),
            }
        }
        Failure::UnbalancedScopes { open_scope } => DiagnosticFailure::UnbalancedScopes {
            open_scope: open_scope.map(|block| diagnostic_bound_node(block.into())),
        },
        Failure::MissingPattern { pattern } => DiagnosticFailure::MissingPattern {
            pattern: diagnostic_bound_node(pattern.into()),
        },
    }
}

const fn diagnostic_storage_flow_input(
    input: bray_checker::StorageFlowInputKind,
) -> &'static str {
    use bray_checker::StorageFlowInputKind as Input;

    match input {
        Input::ExpressionTypes => "expression_types",
        Input::SemanticSelections => "semantic_selections",
        Input::StoragePlan => "storage_plan",
        Input::Liveness => "liveness",
        Input::Refinements => "refinements",
        Input::MemoryOperations => "memory_operations",
        Input::StorageFlow => "storage_flow",
        Input::DependencyContracts => "dependency_contracts",
    }
}

const fn diagnostic_checker_symbol(
    symbol: bray_symbols::AnySymbolId,
) -> bray_diagnostics::DiagnosticCheckerSymbol {
    bray_diagnostics::DiagnosticCheckerSymbol::new(
        symbol.kind().as_str(),
        symbol.symbol_id().raw(),
    )
}

const fn diagnostic_storage_operation_status(
    status: bray_bound_tree::StorageOperationStatus,
) -> &'static str {
    use bray_bound_tree::StorageOperationStatus as Status;

    match status {
        Status::Unreachable => "unreachable",
        Status::Valid => "valid",
        Status::Recovered => "recovered",
        Status::Uninitialized => "uninitialized",
        Status::Moved => "moved",
        Status::ConflictingBorrow => "conflicting_borrow",
        Status::MissingMutationAuthority => "missing_mutation_authority",
        Status::MissingOwnership => "missing_ownership",
        Status::InactiveProjection => "inactive_projection",
        Status::NotCopyable => "not_copyable",
    }
}

const fn diagnostic_bound_node(
    node: bray_bound_tree::AnyBoundNodeId,
) -> bray_diagnostics::DiagnosticCheckerNode {
    bray_diagnostics::DiagnosticCheckerNode::new(
        node.kind().as_str(),
        node.unit().raw(),
        node.ordinal(),
    )
}

const fn diagnostic_symbol_query_kind(kind: bray_symbols::SymbolQueryKind) -> &'static str {
    use bray_symbols::SymbolQueryKind as Kind;

    match kind {
        Kind::Members => "members",
        Kind::Imports => "imports",
        Kind::Directives => "directives",
        Kind::GenericParameters => "generic_parameters",
        Kind::GenericDeclarationTemplate => "generic_declaration_template",
        Kind::GenericConstraints => "generic_constraints",
        Kind::CallableSignature => "callable_signature",
        Kind::CallableContracts => "callable_contracts",
        Kind::CallableContractTemplate => "callable_contract_template",
        Kind::PredicateSignatureTemplate => "predicate_signature_template",
        Kind::CallableContractType => "callable_contract_type",
        Kind::ConstantDeclaredType => "constant_declared_type",
        Kind::ConstantDefinition => "constant_definition",
        Kind::StaticInstanceTemplate => "static_instance_template",
        Kind::CallableParameterDefault => "callable_parameter_default",
        Kind::UnevaluatedDefaultTemplate => "unevaluated_default_template",
        Kind::StructFieldType => "struct_field_type",
        Kind::TypeMemberValue => "type_member_value",
        Kind::StructFieldDefault => "struct_field_default",
        Kind::UnionPayloadFieldType => "union_payload_field_type",
        Kind::UnionPayloadFieldDefault => "union_payload_field_default",
        Kind::PredicateDefinition => "predicate_definition",
        Kind::UnionVariantPayload => "union_variant_payload",
        Kind::ImplementationSubject => "implementation_subject",
        Kind::ImplementedTraitApplication => "implemented_trait_application",
        Kind::ImplementationHeadTemplate => "implementation_head_template",
        Kind::ImplementationCoherence => "implementation_coherence",
        Kind::OverloadArms => "overload_arms",
        Kind::OverloadSignatureTemplate => "overload_signature_template",
    }
}

impl std::fmt::Display for FactQueryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("fact evaluation was cancelled"),
            Self::Cycle(cycle) => write!(
                formatter,
                "fact evaluation encountered a dependency cycle: {:?}",
                cycle.facts()
            ),
            Self::InfrastructureFailure => {
                formatter.write_str("fact evaluation encountered an infrastructure failure")
            }
            Self::BindingDependencyUnavailable => {
                formatter.write_str("name binding could not obtain a required dependency")
            }
            Self::Binding(error) => write!(formatter, "semantic unit binding failed: {error:?}"),
            Self::ConstantCallableBodyUnavailable => {
                formatter.write_str("the constant callable has no available body")
            }
            Self::ConstantCallableRootUnavailable => {
                formatter.write_str("the constant callable body has no result expression")
            }
            Self::AtomicInitializerArgumentUnavailable => {
                formatter.write_str("the atomic initializer argument is unavailable")
            }
            Self::AtomicInitializerResultUnavailable => {
                formatter.write_str("the atomic initializer result cannot be retained")
            }
            Self::UninitInitializerResultUnavailable => formatter
                .write_str("the uninitialized-storage initializer result cannot be retained"),
            Self::ImportedExecutableTemplateMismatch => {
                formatter.write_str("an imported native operation has a mismatched template")
            }
            Self::SemanticUnitContext(error) => {
                write!(formatter, "semantic unit context failed: {error:?}")
            }
            Self::CheckerInfrastructure(error) => {
                write!(
                    formatter,
                    "semantic checking infrastructure failed: {error:?}"
                )
            }
            Self::LoweringInput(error) => {
                write!(
                    formatter,
                    "lowering input validation failed: {:?}",
                    error.cause()
                )
            }
            Self::Lowering(error) => {
                write!(formatter, "MIR lowering failed: {:?}", error.cause())
            }
        }
    }
}

impl std::error::Error for FactQueryError {}
