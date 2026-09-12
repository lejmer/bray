use bray_bound_tree::{
    AnyBoundNodeId, BoundExpressionId, BoundUnitId, BoundUnitKey, BoundWalkOutcome,
};
use bray_checker::CheckedConstantTermsBuildError;
use bray_source::{SourceId, SourceSpan};
use bray_symbols::{
    AnySymbolId, CallableSignatureTemplateError, GenericOwnerId, GenericSubstitutionShapeError,
    ImplementationAmbiguityError, ImplementationCandidateError, ImplementationCandidateSetError,
    ImplementationCoherenceDomainKey, ImplementationCoherenceEvidenceError,
    ImplementationParticipationSetError, ImplementationRequirementKey, ImplementationSymbolId,
    NamedTypeSymbolId, PackageIdentity, SymbolKind, SymbolQueryKind,
    TypeAssociatedSurfaceBuildError, TypeId,
};
use bray_syntax::PreparsedSyntaxFragmentError;

use super::implementation::ImplementationMatchError;
use crate::fact::{CompilationFactKey, SymbolQueryKey};

/// Stable category for an exact binding or semantic-query failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SemanticQueryErrorKind {
    /// A query input or retained semantic relationship violated its compiler contract.
    ContractViolation,
    /// A callable signature template violated its structural contract.
    CallableSignature,
    /// A generic substitution violated its declared parameter shape.
    GenericSubstitution,
    /// A bound unit violated its key, tree, or root contract.
    BoundUnit,
    /// Implementation matching or durable implementation evidence was malformed.
    Implementation,
    /// Checked constant-term publication rejected malformed occurrence input.
    CheckedConstantTerms,
    /// A type-associated surface rejected malformed member input.
    TypeSurface,
    /// Generated preparsed syntax violated its event contract.
    PreparsedSyntax,
}

/// Exact failure retained by binding and semantic compilation queries.
///
/// The stable category is public while compilation-local query keys and source identities remain
/// private to the compilation that owns them.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SemanticQueryError {
    cause: Box<SemanticQueryFailure>,
}

impl SemanticQueryError {
    /// Returns the stable semantic-query failure category.
    pub fn kind(&self) -> SemanticQueryErrorKind {
        self.cause.kind()
    }

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
    fn from(cause: SemanticQueryFailure) -> Self {
        Self {
            cause: Box::new(cause),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum SemanticQueryFailure {
    Contract(SemanticQueryContractFailure),
    CallableSignature {
        callable: Option<AnySymbolId>,
        cause: CallableSignatureTemplateError,
    },
    GenericSubstitution {
        owner: Option<GenericOwnerId>,
        cause: GenericSubstitutionShapeError,
    },
    BoundUnit {
        unit: BoundUnitKey,
        cause: bray_bound_tree::BoundUnitBuildError,
    },
    ImplementationMatch {
        implementation: Option<ImplementationSymbolId>,
        cause: ImplementationMatchError,
    },
    ImplementationAmbiguity {
        requirement: ImplementationRequirementKey,
        cause: ImplementationAmbiguityError,
    },
    ImplementationCandidateSet {
        requirement: ImplementationRequirementKey,
        cause: ImplementationCandidateSetError,
    },
    ImplementationCoherenceEvidence {
        requirement: ImplementationRequirementKey,
        cause: ImplementationCoherenceEvidenceError,
    },
    ImplementationCandidate {
        requirement: ImplementationRequirementKey,
        implementation: ImplementationSymbolId,
        cause: ImplementationCandidateError,
    },
    ImplementationParticipation {
        domain: ImplementationCoherenceDomainKey,
        cause: ImplementationParticipationSetError,
    },
    CheckedConstantTerms {
        unit: Option<BoundUnitKey>,
        cause: CheckedConstantTermsBuildError,
    },
    TypeAssociatedSurface {
        subject: NamedTypeSymbolId,
        cause: TypeAssociatedSurfaceBuildError,
    },
    PreparsedSyntax {
        symbol: AnySymbolId,
        source: Option<SourceSpan>,
        cause: PreparsedSyntaxFragmentError,
    },
}

impl SemanticQueryFailure {
    pub(crate) const fn contract(
        context: SemanticQueryContext,
        violation: SemanticQueryViolation,
    ) -> Self {
        Self::Contract(SemanticQueryContractFailure::new(context, violation, None))
    }

    pub(crate) const fn located_contract(
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

    const fn kind(&self) -> SemanticQueryErrorKind {
        match self {
            Self::Contract(_) => SemanticQueryErrorKind::ContractViolation,
            Self::CallableSignature { .. } => SemanticQueryErrorKind::CallableSignature,
            Self::GenericSubstitution { .. } => SemanticQueryErrorKind::GenericSubstitution,
            Self::BoundUnit { .. } => SemanticQueryErrorKind::BoundUnit,
            Self::ImplementationMatch { .. }
            | Self::ImplementationAmbiguity { .. }
            | Self::ImplementationCandidateSet { .. }
            | Self::ImplementationCoherenceEvidence { .. }
            | Self::ImplementationCandidate { .. }
            | Self::ImplementationParticipation { .. } => SemanticQueryErrorKind::Implementation,
            Self::CheckedConstantTerms { .. } => SemanticQueryErrorKind::CheckedConstantTerms,
            Self::TypeAssociatedSurface { .. } => SemanticQueryErrorKind::TypeSurface,
            Self::PreparsedSyntax { .. } => SemanticQueryErrorKind::PreparsedSyntax,
        }
    }

    fn source(&self) -> Option<SourceSpan> {
        match self {
            Self::Contract(failure) => failure.source().or_else(|| failure.context().source()),
            Self::BoundUnit { unit, .. } => Some(unit_source(unit)),
            Self::CheckedConstantTerms {
                cause: CheckedConstantTermsBuildError::DuplicateOccurrence(key),
                ..
            } => {
                let syntax = key.syntax();

                Some(SourceSpan::new(syntax.source_id(), syntax.full_range()))
            }
            Self::PreparsedSyntax { source, .. } => *source,
            Self::CallableSignature { .. }
            | Self::GenericSubstitution { .. }
            | Self::ImplementationMatch { .. }
            | Self::ImplementationAmbiguity { .. }
            | Self::ImplementationCandidateSet { .. }
            | Self::ImplementationCoherenceEvidence { .. }
            | Self::ImplementationCandidate { .. }
            | Self::ImplementationParticipation { .. }
            | Self::TypeAssociatedSurface { .. } => None,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct SemanticQueryContractFailure {
    context: SemanticQueryContext,
    violation: SemanticQueryViolation,
    source: Option<SourceSpan>,
}

impl SemanticQueryContractFailure {
    pub(crate) const fn new(
        context: SemanticQueryContext,
        violation: SemanticQueryViolation,
        source: Option<SourceSpan>,
    ) -> Self {
        Self {
            context,
            violation,
            source,
        }
    }

    pub(crate) const fn context(&self) -> &SemanticQueryContext {
        &self.context
    }

    pub(crate) const fn violation(&self) -> &SemanticQueryViolation {
        &self.violation
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
    UnexpectedBoundUnitRoot {
        expected: bray_bound_tree::BoundNodeKind,
        actual: AnyBoundNodeId,
    },
    UnexpectedWalkOutcome(BoundWalkOutcome),
    TypeMismatch {
        expected: TypeId,
        actual: TypeId,
    },
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
    DependencyContract,
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
