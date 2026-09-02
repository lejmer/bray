use bray_compiler_known::RepresentationRole;
use bray_declarations::SyntaxAnchor;
use bray_runtime_interface::{PlatformServiceRole, RuntimeAbiRole};
use bray_source::SourceId;
use bray_symbols::{
    AnySymbolId, CallableSignatureTemplateError, FunctionSymbolId, GenericArgumentKind,
    GenericSubstitutionId, StaticSymbolId, TypeData, TypeExpressionTemplate, TypeId,
};

/// Stable category for an exact foreign-boundary query failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ForeignQueryErrorKind {
    /// Required foreign-boundary data was unavailable.
    MissingData,
    /// A retained value violated an exact foreign-boundary contract.
    ContractViolation,
    /// Source-role selection was ambiguous or contradictory.
    SourceRole,
    /// An exact count could not fit its required stable representation.
    NumericOverflow,
    /// A callable signature violated its declared structural contract.
    CallableSignature,
}

/// Exact failure retained by foreign-boundary compilation queries.
///
/// The stable category is public while exact compiler identities remain private to the compilation
/// that owns them.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ForeignQueryError {
    cause: Box<ForeignQueryFailure>,
}

impl ForeignQueryError {
    /// Returns the stable foreign-query failure category.
    pub fn kind(&self) -> ForeignQueryErrorKind {
        self.cause.kind()
    }

    pub(crate) fn cause(&self) -> &ForeignQueryFailure {
        &self.cause
    }
}

impl std::fmt::Display for ForeignQueryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "compiler foreign-query failure: {:?}",
            self.cause()
        )
    }
}

impl std::error::Error for ForeignQueryError {}

impl From<ForeignQueryFailure> for ForeignQueryError {
    fn from(cause: ForeignQueryFailure) -> Self {
        Self {
            cause: Box::new(cause),
        }
    }
}

/// One exact foreign-boundary operation or identity that failed.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ForeignQueryContext {
    Symbol(AnySymbolId),
    Function(FunctionSymbolId),
    Static(StaticSymbolId),
    Directive(SyntaxAnchor),
    Source(SourceId),
    Substitution(GenericSubstitutionId),
    CompilerKnownRepresentation {
        role: RepresentationRole,
        ty: Option<TypeId>,
    },
    PlatformService(PlatformServiceRole),
}

/// One required foreign-boundary value.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ForeignDataKind {
    ContainingModule,
    SourceAnchor,
    FunctionBindingRecord,
    StaticBindingRecord,
    FunctionDeclarationSyntax,
    StaticDeclarationSyntax,
    SourceSnapshot,
    SourceText,
    StructureRecord,
    UnionRecord,
    UnionVariantRecord,
    UnaryRepresentationArgument,
    RepresentationSymbol,
}

/// One semantic type category required by a foreign-boundary operation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ForeignTypeKind {
    Callable,
}

/// Stable integer representation required by one foreign-boundary contract.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ForeignIntegerWidth {
    U64,
}

/// One exact source role selected from the configured runtime or platform interface.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ForeignSourceRole {
    Runtime(RuntimeAbiRole),
    Platform(PlatformServiceRole),
}

/// One typed foreign-boundary query failure.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ForeignQueryFailure {
    Missing {
        context: ForeignQueryContext,
        data: ForeignDataKind,
    },
    CountMismatch {
        context: ForeignQueryContext,
        data: ForeignDataKind,
        expected: usize,
        actual: usize,
    },
    UnexpectedSemanticType {
        ty: TypeId,
        expected: ForeignTypeKind,
        actual: TypeData,
    },
    UnexpectedTypeTemplate {
        context: ForeignQueryContext,
        expected: ForeignTypeKind,
        actual: TypeExpressionTemplate,
    },
    UnexpectedGenericArgument {
        substitution: GenericSubstitutionId,
        expected: GenericArgumentKind,
        actual: GenericArgumentKind,
    },
    NumericOverflow {
        context: ForeignQueryContext,
        value: usize,
        target: ForeignIntegerWidth,
    },
    InvalidPlatformServiceRole {
        role: PlatformServiceRole,
    },
    CallableSignature {
        function: FunctionSymbolId,
        cause: CallableSignatureTemplateError,
    },
    ConflictingSourceRoles {
        function: FunctionSymbolId,
        runtime: RuntimeAbiRole,
        platform: PlatformServiceRole,
    },
    DuplicateSourceRole {
        function: FunctionSymbolId,
        first: ForeignSourceRole,
        duplicate: ForeignSourceRole,
    },
}

impl ForeignQueryFailure {
    pub(crate) const fn missing(context: ForeignQueryContext, data: ForeignDataKind) -> Self {
        Self::Missing { context, data }
    }

    pub(crate) const fn count_mismatch(
        context: ForeignQueryContext,
        data: ForeignDataKind,
        expected: usize,
        actual: usize,
    ) -> Self {
        Self::CountMismatch {
            context,
            data,
            expected,
            actual,
        }
    }

    const fn kind(&self) -> ForeignQueryErrorKind {
        match self {
            Self::Missing { .. } => ForeignQueryErrorKind::MissingData,
            Self::ConflictingSourceRoles { .. } | Self::DuplicateSourceRole { .. } => {
                ForeignQueryErrorKind::SourceRole
            }
            Self::NumericOverflow { .. } => ForeignQueryErrorKind::NumericOverflow,
            Self::CallableSignature { .. } => ForeignQueryErrorKind::CallableSignature,
            Self::CountMismatch { .. }
            | Self::UnexpectedSemanticType { .. }
            | Self::UnexpectedTypeTemplate { .. }
            | Self::UnexpectedGenericArgument { .. }
            | Self::InvalidPlatformServiceRole { .. } => ForeignQueryErrorKind::ContractViolation,
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_runtime_interface::{PlatformServiceRole, RuntimeAbiRole};
    use bray_source::SourceId;
    use bray_symbols::{FunctionSymbolId, SymbolId};

    use super::{
        ForeignDataKind, ForeignIntegerWidth, ForeignQueryContext, ForeignQueryError,
        ForeignQueryErrorKind, ForeignQueryFailure, ForeignSourceRole,
    };
    use crate::fact::FactQueryError;

    #[test]
    fn foreign_query_errors_box_and_retain_exact_domain_causes() {
        let function = FunctionSymbolId::from_symbol_id(SymbolId::new(7));

        let cases = [
            (
                ForeignQueryFailure::missing(
                    ForeignQueryContext::Function(function),
                    ForeignDataKind::SourceAnchor,
                ),
                ForeignQueryErrorKind::MissingData,
            ),
            (
                ForeignQueryFailure::ConflictingSourceRoles {
                    function,
                    runtime: RuntimeAbiRole::RootExecution,
                    platform: PlatformServiceRole::StandardOutputFlush,
                },
                ForeignQueryErrorKind::SourceRole,
            ),
            (
                ForeignQueryFailure::DuplicateSourceRole {
                    function,
                    first: ForeignSourceRole::Runtime(RuntimeAbiRole::RootExecution),
                    duplicate: ForeignSourceRole::Runtime(RuntimeAbiRole::SynchronousRootExecution),
                },
                ForeignQueryErrorKind::SourceRole,
            ),
            (
                ForeignQueryFailure::NumericOverflow {
                    context: ForeignQueryContext::Source(SourceId::new(3)),
                    value: usize::MAX,
                    target: ForeignIntegerWidth::U64,
                },
                ForeignQueryErrorKind::NumericOverflow,
            ),
            (
                ForeignQueryFailure::CallableSignature {
                    function,
                    cause: bray_symbols::CallableSignatureTemplateError::InvalidCallableType,
                },
                ForeignQueryErrorKind::CallableSignature,
            ),
            (
                ForeignQueryFailure::InvalidPlatformServiceRole {
                    role: PlatformServiceRole::StandardOutputFlush,
                },
                ForeignQueryErrorKind::ContractViolation,
            ),
        ];

        assert_eq!(
            std::mem::size_of::<ForeignQueryError>(),
            std::mem::size_of::<usize>()
        );

        for (cause, expected) in cases {
            let error = ForeignQueryError::from(cause.clone());

            assert_eq!(error.kind(), expected);
            assert_eq!(error.cause(), &cause);

            let FactQueryError::Foreign(error) = FactQueryError::from(cause.clone()) else {
                panic!("foreign failure must retain the foreign-query boundary")
            };

            assert_eq!(error.cause(), &cause);
        }
    }
}
