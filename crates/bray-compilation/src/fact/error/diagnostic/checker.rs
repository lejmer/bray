use bray_checker::CheckerInfrastructureError;
use bray_diagnostics::{
    DiagnosticCheckerConstantOperationFailure, DiagnosticCheckerFailure, DiagnosticLivenessFailure,
    DiagnosticMemoryOperationsFailure, DiagnosticSemanticSelectionFailure,
    DiagnosticSemanticSnapshotFailure, DiagnosticStoragePlanFailure,
};
use bray_source::SourceSpan;

use super::diagnostic_semantic_value_failure;
use crate::fact::diagnostic_generic_substitution_failure;

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
        Error::GenericSubstitution(error) => DiagnosticCheckerFailure::GenericSubstitution(
            diagnostic_generic_substitution_failure(error),
        ),
        Error::SemanticValueStore(error) => {
            DiagnosticCheckerFailure::SemanticValue(diagnostic_semantic_value_failure(error))
        }
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
        Error::InvalidAtomicOperationInput {
            hook,
            argument_count,
        } => DiagnosticCheckerFailure::InvalidAtomicOperationInput {
            hook: hook.as_str(),
            argument_count,
        },
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
        Error::IncompatibleInput {
            input,
            expected_unit,
            expected_kind,
            actual_unit,
            actual_kind,
        } => DiagnosticCheckerFailure::IncompatibleInput {
            input: input.as_str(),
            expected_unit: expected_unit.raw(),
            expected_kind: expected_kind.as_str(),
            actual_unit: actual_unit.raw(),
            actual_kind: actual_kind.as_str(),
        },
        Error::CheckedConstantTerms(
            bray_checker::CheckedConstantTermsBuildError::DuplicateOccurrence(key),
        ) => DiagnosticCheckerFailure::CheckedConstantTerms {
            owner: diagnostic_checker_symbol(key.owner()),
            source: SourceSpan::new(key.syntax().source_id(), key.syntax().full_range()),
        },
        Error::LiteralValue(error) => {
            DiagnosticCheckerFailure::LiteralValue(diagnostic_literal_value_failure(error))
        }
        Error::PatternInput(error) => {
            DiagnosticCheckerFailure::PatternInput(diagnostic_pattern_input_failure(error))
        }
        Error::ConstantInput(error) => {
            DiagnosticCheckerFailure::ConstantInput(diagnostic_constant_input_failure(error))
        }
        Error::ConstantEvaluation(error) => DiagnosticCheckerFailure::ConstantEvaluation(
            diagnostic_constant_evaluation_failure(error),
        ),
        Error::ConstantOperation(error) => DiagnosticCheckerFailure::ConstantOperation(
            diagnostic_constant_operation_failure(error),
        ),
        Error::SelectionInputCapacityExceeded { count } => {
            DiagnosticCheckerFailure::SelectionInputCapacityExceeded { count }
        }
        Error::SelectionInputOrdinalUnrepresentable { ordinal } => {
            DiagnosticCheckerFailure::SelectionInputOrdinalUnrepresentable { ordinal }
        }
        Error::SelectionDiagnosticCapacityExceeded { kind, count } => {
            DiagnosticCheckerFailure::SelectionDiagnosticCapacityExceeded { kind, count }
        }
        Error::CallbackParameterOrdinalUnrepresentable { ordinal } => {
            DiagnosticCheckerFailure::CallbackParameterOrdinalUnrepresentable { ordinal }
        }
        Error::ConstantArrayLengthCapacityExceeded { length } => {
            DiagnosticCheckerFailure::ConstantArrayLengthCapacityExceeded { length }
        }
        Error::InvalidSemanticSelectionInput => {
            DiagnosticCheckerFailure::InvalidSemanticSelectionInput
        }
        Error::InvalidMemoryOperationInput { hook } => {
            DiagnosticCheckerFailure::InvalidMemoryOperationInput {
                hook: hook.as_str(),
            }
        }
        Error::InvalidMemoryGenericArgument { ordinal, actual } => {
            DiagnosticCheckerFailure::InvalidMemoryGenericArgument {
                ordinal,
                actual: actual.as_str(),
            }
        }
        Error::InvalidCallbackSignatureInput { actual } => {
            DiagnosticCheckerFailure::InvalidCallbackSignatureInput { actual }
        }
        Error::SemanticSelection(error) => DiagnosticCheckerFailure::SemanticSelection(
            diagnostic_semantic_selection_failure(error),
        ),
        Error::InvalidConstantEvaluationInput => {
            DiagnosticCheckerFailure::InvalidConstantEvaluationInput
        }
        Error::InvalidStoragePlan => DiagnosticCheckerFailure::InvalidStoragePlan,
        Error::StoragePlan(error) => {
            DiagnosticCheckerFailure::StoragePlan(diagnostic_storage_plan_failure(error))
        }
        Error::MemoryOperations(error) => DiagnosticCheckerFailure::MemoryOperations(match error {
            bray_bound_tree::CheckedMemoryOperationsBuildError::ForeignUnit => {
                DiagnosticMemoryOperationsFailure::ForeignUnit
            }
            bray_bound_tree::CheckedMemoryOperationsBuildError::DuplicateExpression => {
                DiagnosticMemoryOperationsFailure::DuplicateExpression
            }
        }),
        Error::InvalidLiveness => DiagnosticCheckerFailure::InvalidLiveness,
        Error::Liveness(error) => DiagnosticCheckerFailure::Liveness(match error {
            bray_bound_tree::LivenessBuildError::ForeignUnit => {
                DiagnosticLivenessFailure::ForeignUnit
            }
            bray_bound_tree::LivenessBuildError::UnsupportedSubject => {
                DiagnosticLivenessFailure::UnsupportedSubject
            }
        }),
        Error::InvalidRefinementInput => DiagnosticCheckerFailure::InvalidRefinementInput,
        Error::RefinementCapacityUnrepresentable => {
            DiagnosticCheckerFailure::RefinementCapacityUnrepresentable
        }
        Error::RefinementStorageUnavailable => {
            DiagnosticCheckerFailure::RefinementStorageUnavailable
        }
        Error::StorageFlow(failure) => {
            DiagnosticCheckerFailure::StorageFlow(diagnostic_storage_flow_failure(failure))
        }
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
        Error::SemanticSnapshot(error) => {
            DiagnosticCheckerFailure::SemanticSnapshot(DiagnosticSemanticSnapshotFailure::new(
                error.input().as_str(),
                error.expected_unit().raw(),
                error.expected_kind().as_str(),
                error.actual_unit().raw(),
                error.actual_kind().as_str(),
            ))
        }
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

const fn diagnostic_constant_operation_failure(
    failure: bray_checker::CheckerConstantOperationFailure,
) -> DiagnosticCheckerConstantOperationFailure {
    use bray_checker::CheckerConstantOperationFailure as Failure;

    match failure {
        Failure::Invalid => DiagnosticCheckerConstantOperationFailure::Invalid,
        Failure::DivisionByZero => DiagnosticCheckerConstantOperationFailure::DivisionByZero,
        Failure::NotRepresentable => DiagnosticCheckerConstantOperationFailure::NotRepresentable,
        Failure::ResourceLimitExceeded { actual, maximum } => {
            DiagnosticCheckerConstantOperationFailure::ResourceLimitExceeded { actual, maximum }
        }
    }
}

const fn diagnostic_storage_plan_failure(
    failure: bray_bound_tree::StoragePlanBuildError,
) -> DiagnosticStoragePlanFailure {
    use bray_bound_tree::StoragePlanBuildError as Failure;

    match failure {
        Failure::ForeignUnit => DiagnosticStoragePlanFailure::ForeignUnit,
        Failure::CapacityExceeded => DiagnosticStoragePlanFailure::CapacityExceeded,
        Failure::MissingIdentity => DiagnosticStoragePlanFailure::MissingIdentity,
        Failure::MissingAccess => DiagnosticStoragePlanFailure::MissingAccess,
        Failure::MissingBorrowCapability => DiagnosticStoragePlanFailure::MissingBorrowCapability,
        Failure::DuplicateBinding => DiagnosticStoragePlanFailure::DuplicateBinding,
        Failure::BindingIdentityMismatch => DiagnosticStoragePlanFailure::BindingIdentityMismatch,
    }
}

fn diagnostic_semantic_selection_failure(
    failure: bray_bound_tree::SemanticSelectionTableBuildError,
) -> DiagnosticSemanticSelectionFailure {
    use bray_bound_tree::SemanticSelectionTableBuildError as Failure;

    match failure {
        Failure::ForeignExpressionTypes {
            expected_unit,
            expected_kind,
            actual_unit,
            actual_kind,
        } => DiagnosticSemanticSelectionFailure::ForeignExpressionTypes {
            expected_unit: expected_unit.raw(),
            expected_kind: expected_kind.as_str(),
            actual_unit: actual_unit.raw(),
            actual_kind: actual_kind.as_str(),
        },
        Failure::InvalidExpression(expression) => {
            DiagnosticSemanticSelectionFailure::InvalidExpression(diagnostic_bound_node(
                expression.into(),
            ))
        }
        Failure::DuplicateExpression(expression) => {
            DiagnosticSemanticSelectionFailure::DuplicateExpression(diagnostic_bound_node(
                expression.into(),
            ))
        }
        Failure::SelectionKindMismatch(expression) => {
            DiagnosticSemanticSelectionFailure::SelectionKindMismatch(diagnostic_bound_node(
                expression.into(),
            ))
        }
        Failure::ResultTypeMismatch(expression) => {
            DiagnosticSemanticSelectionFailure::ResultTypeMismatch(diagnostic_bound_node(
                expression.into(),
            ))
        }
        Failure::OperandTypeMismatch(expression) => {
            DiagnosticSemanticSelectionFailure::OperandTypeMismatch(diagnostic_bound_node(
                expression.into(),
            ))
        }
        Failure::SubjectTypeMismatch(expression) => {
            DiagnosticSemanticSelectionFailure::SubjectTypeMismatch(diagnostic_bound_node(
                expression.into(),
            ))
        }
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
        Failure::FlowConstruction(error) => DiagnosticFailure::FlowConstruction(match error {
            bray_bound_tree::StorageFlowBuildError::ForeignUnit => "foreign_unit",
            bray_bound_tree::StorageFlowBuildError::DuplicateSuspension => "duplicate_suspension",
            bray_bound_tree::StorageFlowBuildError::DuplicateMemoryOperation => {
                "duplicate_memory_operation"
            }
        }),
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
        Failure::MissingBorrowCapability { borrow } => DiagnosticFailure::MissingBorrowCapability {
            unit: borrow.unit().raw(),
            borrow: borrow.ordinal(),
        },
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
        Failure::MissingStorageIdentity { identity } => DiagnosticFailure::MissingStorageIdentity {
            unit: identity.unit().raw(),
            identity: identity.ordinal(),
        },
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

const fn diagnostic_storage_flow_input(input: bray_checker::StorageFlowInputKind) -> &'static str {
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
    bray_diagnostics::DiagnosticCheckerSymbol::new(symbol.kind().as_str(), symbol.symbol_id().raw())
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

fn diagnostic_literal_value_failure(
    failure: bray_checker::CheckerLiteralValueFailure,
) -> bray_diagnostics::DiagnosticLiteralValueFailure {
    use bray_checker::CheckerLiteralValueFailure as Failure;
    use bray_diagnostics::DiagnosticLiteralValueFailure as DiagnosticFailure;

    match failure {
        Failure::ForeignExpressionTypes => DiagnosticFailure::ForeignExpressionTypes,
        Failure::InvalidLiteral { expression } => {
            DiagnosticFailure::InvalidLiteral(diagnostic_bound_node(expression.into()))
        }
        Failure::MissingExpressionType { expression } => {
            DiagnosticFailure::MissingExpressionType(diagnostic_bound_node(expression.into()))
        }
        Failure::MissingLiteralValue { expression } => {
            DiagnosticFailure::MissingLiteralValue(diagnostic_bound_node(expression.into()))
        }
        Failure::ValueTypeMismatch { expression } => {
            DiagnosticFailure::ValueTypeMismatch(diagnostic_bound_node(expression.into()))
        }
        Failure::DuplicateExpression { expression } => {
            DiagnosticFailure::DuplicateExpression(diagnostic_bound_node(expression.into()))
        }
    }
}

fn diagnostic_pattern_input_failure(
    failure: bray_checker::CheckerPatternInputFailure,
) -> bray_diagnostics::DiagnosticPatternInputFailure {
    use bray_checker::CheckerPatternInputFailure as Failure;
    use bray_diagnostics::DiagnosticPatternInputFailure as DiagnosticFailure;

    match failure {
        Failure::ConflictingDeclaredPattern { pattern } => {
            DiagnosticFailure::ConflictingDeclaredPattern(diagnostic_bound_node(pattern.into()))
        }
        Failure::ConflictingConstantPattern { pattern } => {
            DiagnosticFailure::ConflictingConstantPattern(diagnostic_bound_node(pattern.into()))
        }
        Failure::ConflictingGuard { expression } => {
            DiagnosticFailure::ConflictingGuard(diagnostic_bound_node(expression.into()))
        }
    }
}

fn diagnostic_constant_input_failure(
    failure: bray_checker::CheckerConstantInputFailure,
) -> bray_diagnostics::DiagnosticConstantInputFailure {
    use bray_checker::CheckerConstantInputFailure as Failure;
    use bray_diagnostics::DiagnosticConstantInputFailure as DiagnosticFailure;

    match failure {
        Failure::ConflictingReference { expression } => {
            DiagnosticFailure::ConflictingReference(diagnostic_bound_node(expression.into()))
        }
        Failure::ConflictingLocalTerm { local } => {
            DiagnosticFailure::ConflictingLocalTerm(diagnostic_local_symbol(local))
        }
    }
}

fn diagnostic_constant_evaluation_failure(
    failure: bray_checker::CheckerConstantEvaluationFailure,
) -> bray_diagnostics::DiagnosticConstantEvaluationFailure {
    use bray_checker::CheckerConstantEvaluationFailure as Failure;
    use bray_diagnostics::DiagnosticConstantEvaluationFailure as DiagnosticFailure;

    match failure {
        Failure::InvalidExpressionRoot { expression } => {
            DiagnosticFailure::InvalidExpressionRoot(diagnostic_bound_node(expression.into()))
        }
        Failure::InvalidBlockRoot { block } => {
            DiagnosticFailure::InvalidBlockRoot(diagnostic_bound_node(block.into()))
        }
        Failure::MissingExpressionType { expression } => {
            DiagnosticFailure::MissingExpressionType(diagnostic_bound_node(expression.into()))
        }
        Failure::MissingBlockResultType { block } => {
            DiagnosticFailure::MissingBlockResultType(diagnostic_bound_node(block.into()))
        }
        Failure::MissingExpression { expression } => {
            DiagnosticFailure::MissingExpression(diagnostic_bound_node(expression.into()))
        }
        Failure::MissingBlock { block } => {
            DiagnosticFailure::MissingBlock(diagnostic_bound_node(block.into()))
        }
        Failure::MissingPatternInput { pattern } => {
            DiagnosticFailure::MissingPatternInput(diagnostic_bound_node(pattern.into()))
        }
        Failure::MissingPattern { pattern } => {
            DiagnosticFailure::MissingPattern(diagnostic_bound_node(pattern.into()))
        }
        Failure::MissingPatternBinding { binding } => {
            DiagnosticFailure::MissingPatternBinding(diagnostic_local_symbol(binding.into()))
        }
        Failure::UnexpectedPropagation { term } => DiagnosticFailure::UnexpectedPropagation {
            store: term.store_id().raw(),
            slot: term.slot(),
        },
    }
}

fn diagnostic_local_symbol(
    local: bray_symbols::AnyLocalSymbolId,
) -> bray_diagnostics::DiagnosticCheckerLocal {
    use bray_symbols::AnyLocalSymbolId as Local;

    let (region, ordinal) = match local {
        Local::Binding(local) => (local.region(), local.ordinal()),
        Local::Constant(local) => (local.region(), local.ordinal()),
        Local::AnonymousCallable(local) => (local.region(), local.ordinal()),
        Local::AnonymousCallableParameter(local) => (local.region(), local.ordinal()),
        Local::PostconditionResult(local) => (local.region(), local.ordinal()),
    };

    bray_diagnostics::DiagnosticCheckerLocal::new(local.kind().as_str(), region.raw(), ordinal)
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
    kind.as_str()
}
