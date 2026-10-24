use bray_compiler_known::RepresentationRole;
use bray_declarations::SyntaxAnchor;
use bray_runtime_interface::{PlatformServiceRole, RuntimeAbiRole};
use bray_source::SourceId;
use bray_symbols::{AnySymbolId, FunctionSymbolId, GenericSubstitutionId, StaticSymbolId, TypeId};

/// Exact failure retained by foreign-boundary compilation queries.
///
/// Retains numeric capacity and conflicts in configured native source roles.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ForeignQueryError {
    cause: Box<ForeignQueryFailure>,
}

impl ForeignQueryError {
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
    #[track_caller]
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
    NumericOverflow {
        context: ForeignQueryContext,
        value: usize,
        target: ForeignIntegerWidth,
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
    #[track_caller]
    pub(crate) fn missing(context: ForeignQueryContext, data: ForeignDataKind) -> Self {
        panic!("foreign missing contract failed: context={context:?}, data={data:?}")
    }

    #[track_caller]
    pub(crate) fn count_mismatch(
        context: ForeignQueryContext,
        data: ForeignDataKind,
        expected: usize,
        actual: usize,
    ) -> Self {
        panic!(
            "foreign count_mismatch contract failed: context={context:?}, data={data:?}, expected={expected}, actual={actual}"
        )
    }
}

#[cfg(test)]
mod tests {
    use bray_runtime_interface::{PlatformServiceRole, RuntimeAbiRole};
    use bray_source::SourceId;
    use bray_symbols::{FunctionSymbolId, SymbolId};

    use super::{
        ForeignIntegerWidth, ForeignQueryContext, ForeignQueryError, ForeignQueryFailure,
        ForeignSourceRole,
    };
    use crate::fact::FactQueryError;

    #[test]
    fn foreign_query_errors_box_and_retain_exact_domain_causes() {
        let function = FunctionSymbolId::from_symbol_id(SymbolId::new(13));

        let cases = [
            ForeignQueryFailure::ConflictingSourceRoles {
                function,
                runtime: RuntimeAbiRole::RootExecution,
                platform: PlatformServiceRole::StandardOutputFlush,
            },
            ForeignQueryFailure::DuplicateSourceRole {
                function,
                first: ForeignSourceRole::Runtime(RuntimeAbiRole::RootExecution),
                duplicate: ForeignSourceRole::Runtime(RuntimeAbiRole::SynchronousRootExecution),
            },
            ForeignQueryFailure::NumericOverflow {
                context: ForeignQueryContext::Source(SourceId::new(3)),
                value: usize::MAX,
                target: ForeignIntegerWidth::U64,
            },
        ];

        assert_eq!(
            std::mem::size_of::<ForeignQueryError>(),
            std::mem::size_of::<usize>()
        );

        for cause in cases {
            let error = ForeignQueryError::from(cause.clone());

            assert_eq!(error.cause(), &cause);

            let FactQueryError::Foreign(error) = FactQueryError::from(cause.clone()) else {
                panic!("foreign cause must retain its boundary")
            };

            assert_eq!(error.cause(), &cause);
        }
    }
}
