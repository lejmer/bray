use bray_bound_tree::{
    AnyBoundNodeId, BoundExpressionId, BoundUnitId, BoundUnitKey, BoundWalkOutcome,
};
use bray_source::{SourceId, SourceSpan};
use bray_symbols::{
    AnySymbolId, GenericOwnerId, GenericSubstitutionShapeError, ImplementationCoherenceDomainKey,
    PackageIdentity, SymbolKind, SymbolQueryKind, TypeId,
};

use crate::fact::{CompilationFactKey, SymbolQueryKey};

/// Exact failure retained by binding and semantic compilation queries.
///
/// Capacity limits remain recoverable. Violated semantic relationships fail at their owning query.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SemanticQueryError {
    cause: Box<SemanticQueryFailure>,
}

impl SemanticQueryError {
    pub(crate) fn source(&self) -> Option<SourceSpan> {
        self.cause.source()
    }

    pub(crate) fn cause(&self) -> &SemanticQueryFailure {
        &self.cause
    }
}

impl std::fmt::Display for SemanticQueryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "compiler semantic-query failure: {:?}",
            self.cause()
        )
    }
}

impl std::error::Error for SemanticQueryError {}

impl From<SemanticQueryFailure> for SemanticQueryError {
    #[track_caller]
    fn from(cause: SemanticQueryFailure) -> Self {
        match &cause {
            SemanticQueryFailure::Contract(_) => {}
            SemanticQueryFailure::GenericSubstitution {
                cause: GenericSubstitutionShapeError::OrdinalOverflow,
                ..
            } => {}
            _ => panic!("semantic query construction violated its established contract: {cause:?}"),
        }

        Self {
            cause: Box::new(cause),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum SemanticQueryFailure {
    Contract(SemanticQueryContractFailure),
    GenericSubstitution {
        owner: Option<GenericOwnerId>,
        cause: GenericSubstitutionShapeError,
    },
}

impl SemanticQueryFailure {
    #[track_caller]
    pub(crate) fn contract(
        context: SemanticQueryContext,
        violation: SemanticQueryViolation,
    ) -> Self {
        Self::Contract(SemanticQueryContractFailure::new(context, violation, None))
    }

    #[track_caller]
    pub(crate) fn located_contract(
        context: SemanticQueryContext,
        violation: SemanticQueryViolation,
        source: SourceSpan,
    ) -> Self {
        Self::Contract(SemanticQueryContractFailure::new(
            context,
            violation,
            Some(source),
        ))
    }

    fn source(&self) -> Option<SourceSpan> {
        match self {
            Self::Contract(failure) => failure.source().or_else(|| failure.context().source()),
            Self::GenericSubstitution { .. } => None,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct SemanticQueryContractFailure {
    context: SemanticQueryContext,
    data: SemanticDataKind,
    value: u128,
    source: Option<SourceSpan>,
}

impl SemanticQueryContractFailure {
    #[track_caller]
    pub(crate) fn new(
        context: SemanticQueryContext,
        violation: SemanticQueryViolation,
        source: Option<SourceSpan>,
    ) -> Self {
        let (data, value) = match violation {
            SemanticQueryViolation::CapacityExceeded { data, value } => (
                data,
                u128::try_from(value).expect("a host index must fit the compiler's widest count"),
            ),
            SemanticQueryViolation::CountOverflow { data, value } => (data, u128::from(value)),
            _ => panic!(
                "semantic query contract failed: context={context:?}, violation={violation:?}, source={source:?}"
            ),
        };

        Self {
            context,
            data,
            value,
            source,
        }
    }

    pub(crate) const fn data(&self) -> SemanticDataKind {
        self.data
    }

    pub(crate) const fn value(&self) -> u128 {
        self.value
    }

    pub(crate) const fn context(&self) -> &SemanticQueryContext {
        &self.context
    }

    pub(crate) const fn source(&self) -> Option<SourceSpan> {
        self.source
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum SemanticQueryContext {
    Fact(CompilationFactKey),
    SymbolQuery(SymbolQueryKey),
    Symbol(AnySymbolId),
    SymbolKey(bray_symbols::SymbolKey),
    Unit(BoundUnitKey),
    Expression {
        unit: BoundUnitKey,
        expression: BoundExpressionId,
    },
    BoundExpression {
        unit: BoundUnitId,
        expression: BoundExpressionId,
    },
    CompilerKnownDeclaration(bray_compiler_known::CompilerKnownDeclarationKey),
    CompilerKnownDeclarationName(&'static str),
    CompilerKnownRepresentation(bray_compiler_known::RepresentationRole),
    Declaration(bray_declarations::DeclarationId),
    LocalReference {
        unit: BoundUnitKey,
        expression: BoundExpressionId,
        local: bray_symbols::AnyLocalSymbolId,
    },
    Source(SourceId),
    Type(TypeId),
    ImplementationDomain(ImplementationCoherenceDomainKey),
}

impl SemanticQueryContext {
    fn source(&self) -> Option<SourceSpan> {
        match self {
            Self::Unit(unit)
            | Self::Expression { unit, .. }
            | Self::LocalReference { unit, .. } => Some(unit_source(unit)),
            Self::Fact(_)
            | Self::SymbolQuery(_)
            | Self::Symbol(_)
            | Self::SymbolKey(_)
            | Self::BoundExpression { .. }
            | Self::CompilerKnownDeclaration(_)
            | Self::CompilerKnownDeclarationName(_)
            | Self::CompilerKnownRepresentation(_)
            | Self::Declaration(_)
            | Self::Source(_)
            | Self::Type(_)
            | Self::ImplementationDomain(_) => None,
        }
    }
}

fn unit_source(unit: &BoundUnitKey) -> SourceSpan {
    let syntax = unit.source().syntax();

    SourceSpan::new(syntax.source_id(), syntax.full_range())
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum SemanticQueryViolation {
    Missing(SemanticDataKind),
    UnexpectedSymbolKind {
        expected: SemanticSymbolCategory,
        actual: SymbolKind,
    },
    UnexpectedConstantValueKind(bray_symbols::ConstantValueKind),
    UnexpectedSymbolOrigin(bray_symbols::SymbolOrigin),
    GenericParameterKindMismatch {
        expected: bray_compiler_known::CatalogGenericParameterKind,
        actual: bray_compiler_known::CatalogGenericParameterKind,
    },
    DuplicateSymbol(AnySymbolId),
    SymbolKindMismatch {
        expected: SymbolKind,
        actual: SymbolKind,
    },
    MissingBoundNode(AnyBoundNodeId),
    MissingCheckedTemplateNode(bray_bound_tree::CheckedTemplateNodeId),
    UnexpectedWalkOutcome(BoundWalkOutcome),
    CountMismatch {
        data: SemanticDataKind,
        expected: usize,
        actual: usize,
    },
    CapacityExceeded {
        data: SemanticDataKind,
        value: usize,
    },
    CountOverflow {
        data: SemanticDataKind,
        value: u64,
    },
    QueryStackMismatch {
        expected: AnySymbolId,
        actual: Option<AnySymbolId>,
    },
    PackageMismatch {
        expected: PackageIdentity,
        actual: PackageIdentity,
    },
    CompilerKnownOperationUnavailable {
        storage: TypeId,
        target: TypeId,
        role: bray_compiler_known::CompilerKnownOperationRole,
    },
    ConstantExpectationMismatch {
        expected: bray_symbols::ConstantExpressionExpectedType,
        actual: bray_symbols::ConstantExpressionExpectedType,
    },
    ImportedRecordKindMismatch {
        expected: bray_package_interface::InterfaceSemanticRecordKind,
        actual: bray_package_interface::InterfaceSemanticRecordKind,
    },
    UnexpectedOrder(SemanticDataKind),
    Unsupported(SemanticDataKind),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum SemanticSymbolCategory {
    Callable,
    CallableParameter,
    GenericConstParameter,
    GenericOwner,
    GenericParameter,
    Implementation,
    TraitImplementation,
    NamedType,
    PredicateDefinition,
    RuntimeDefaultProvider,
    RuntimeDefaultSubject,
    StructField,
    UnionPayloadField,
    TrustedCapability,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum SemanticDataKind {
    LifecycleMember,
    BoundExpression,
    BoundUnit,
    CallableSignature,
    ConstantDefinition,
    ConstantTerm,
    DeclarationRecord,
    ExecutionEvidence,
    GenericConstraint,
    GenericSubstitution,
    Implementation,
    ImplementationCandidate,
    ImplementationComparison,
    ImplementationUsing,
    ImportedTemplate,
    IterationProtocol,
    IterationSource,
    LiteralValue,
    MemberName,
    OperationSelection,
    OverloadComparison,
    RuntimeDefault,
    SourceAnchor,
    SourceSnapshot,
    Symbol,
    SymbolKey,
    SymbolQuery(SymbolQueryKind),
    Syntax,
    ContainingModule,
    TraitApplication,
    Type,
    TypeSurface,
    Diagnostic,
}

#[cfg(test)]
mod tests {
    use super::{
        SemanticDataKind, SemanticQueryContext, SemanticQueryError, SemanticQueryFailure,
        SemanticQueryViolation,
    };

    #[test]
    fn capacity_survives_while_missing_semantic_relationships_panic() {
        let context = SemanticQueryContext::Source(bray_source::SourceId::new(19));

        let error = SemanticQueryError::from(SemanticQueryFailure::contract(
            context.clone(),
            SemanticQueryViolation::CountOverflow {
                data: SemanticDataKind::Type,
                value: u64::MAX,
            },
        ));

        let SemanticQueryFailure::Contract(capacity) = error.cause() else {
            panic!("capacity must retain its exact contract")
        };

        assert_eq!(capacity.value(), u128::from(u64::MAX));
        assert_eq!(capacity.context(), &context);

        let panic = std::panic::catch_unwind(|| {
            SemanticQueryFailure::contract(
                context,
                SemanticQueryViolation::Missing(SemanticDataKind::Type),
            )
        })
        .err()
        .expect("missing semantic relationship must panic");

        let message = bray_testing::panic_payload_text(panic.as_ref());

        assert!(message.contains("Missing(Type)"));
        assert!(message.contains("19"));
    }
}
