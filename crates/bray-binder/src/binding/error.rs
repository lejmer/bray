use std::hash::{Hash, Hasher};

use crate::{
    BindingQueryError, publication::BoundUnitAssemblyError, unit::BoundUnitConstructionError,
};

/// A typed failure encountered while binding one source construct.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BindingError<Upstream = std::convert::Infallible> {
    /// Cancellation was observed before binding completed.
    Cancelled,
    /// Semantic checking could not complete because a typed dependency was unavailable.
    CheckerInfrastructure(bray_checker::CheckerInfrastructureError),
    /// The canonical semantic-value store rejected an operation.
    SemanticValue(bray_symbols::SemanticValueStoreError),
    /// The coordinating query layer returned one of its own exact failures.
    Upstream(Upstream),
    /// A required binding dependency could not be supplied.
    DependencyUnavailable,
    /// The exact source construct selected for a dependent bound unit is unavailable.
    MissingSyntax {
        /// The exact versioned source construct that could not be recovered.
        source: bray_bound_tree::BoundSourceAnchor,
    },
    /// The expected dependent unit owner or a known related owner record is absent.
    MissingOwner {
        /// The exact versioned source construct being bound.
        source: bray_bound_tree::BoundSourceAnchor,
        /// The stable identity expected to resolve as the unit owner.
        owner: bray_symbols::SymbolKey,
        /// A resolved related symbol whose record or owner relationship is absent, when known.
        symbol: Option<bray_symbols::AnySymbolId>,
    },
    /// A resolved dependent unit owner has no containing logical module.
    MissingModule {
        /// The exact versioned source construct being bound.
        source: bray_bound_tree::BoundSourceAnchor,
        /// The resolved owner whose containing module is absent.
        owner: bray_symbols::AnySymbolId,
    },
    /// A surface symbol supplied to a dependent bound unit has no valid local lookup name.
    InvalidSurfaceName {
        /// The exact versioned source construct being bound.
        source: bray_bound_tree::BoundSourceAnchor,
        /// The surface symbol whose name violates the lookup contract.
        symbol: bray_symbols::AnySymbolId,
    },
    /// A local symbol identity exceeded its compact representation.
    IdentityCapacityExceeded,
    /// A transactional binding mutation could not be rolled back.
    RollbackFailed,
    /// A binding transaction was committed against another context.
    TransactionContextMismatch,
    /// A control transfer did not match its selected target.
    ControlTargetMismatch,
    /// Binding reached syntax unsupported by the selected binding operation.
    UnsupportedSyntax,
    /// Parsed syntax violated a shape required by the selected binding operation.
    SyntaxContract(bray_declarations::SyntaxAnchor),
    /// A symbol expected to own generic substitution data cannot serve as a generic owner.
    GenericOwnerUnavailable(bray_symbols::AnySymbolId),
    /// No compiler-known declaration supplies a required representation role.
    CompilerKnownRepresentationUnavailable(bray_compiler_known::RepresentationRole),
    /// A symbol identity expected to have a local or imported record has no record.
    SymbolRecordUnavailable(bray_symbols::AnySymbolId),
    /// A module-part identity referenced by symbol metadata has no declaration record.
    ModulePartRecordUnavailable(bray_declarations::ModulePartId),
    /// A declaration identity referenced by module metadata has no declaration record.
    DeclarationRecordUnavailable(bray_declarations::DeclarationId),
    /// A contextual `Self` type was requested without an enclosing self-type context.
    ContextualSelfUnavailable(bray_declarations::SyntaxAnchor),
    /// An operation requiring a canonical type received a deferred type template.
    UnresolvedTypeTemplate,
    /// A requested transient unit category is incompatible with its resolved owner.
    InvalidUnitKey {
        /// The exact versioned source construct selected for the unit.
        source: bray_bound_tree::BoundSourceAnchor,
        /// The resolved surface owner rejected by the unit category.
        owner: bray_symbols::AnySymbolId,
    },
    /// Callable syntax and its declared parameter identities disagree in count.
    CallableParameterCountMismatch {
        /// The parameter-list syntax whose shape disagrees with symbol metadata.
        source: bray_declarations::SyntaxAnchor,
        /// The callable whose signature is being bound.
        callable: bray_symbols::CallableSymbolId,
        /// The number of parameters present in source syntax.
        syntax_count: usize,
        /// The number of declared parameter symbols.
        symbol_count: usize,
    },
    /// A declared parameter identity belongs to another callable.
    CallableParameterOwnerMismatch {
        /// The parameter-list syntax being bound.
        source: bray_declarations::SyntaxAnchor,
        /// The callable whose signature is being bound.
        callable: bray_symbols::CallableSymbolId,
        /// The mismatched parameter identity.
        parameter: bray_symbols::CallableParameterSymbolId,
    },
    /// A declared receiver identity belongs to another callable.
    ReceiverParameterOwnerMismatch {
        /// The parameter-list syntax being bound.
        source: bray_declarations::SyntaxAnchor,
        /// The callable whose signature is being bound.
        callable: bray_symbols::CallableSymbolId,
        /// The mismatched receiver identity.
        receiver: bray_symbols::ReceiverParameterSymbolId,
    },
    /// Callable receiver syntax, identity, and contextual self information disagree.
    ReceiverContextMismatch {
        /// The parameter-list syntax being bound.
        source: bray_declarations::SyntaxAnchor,
        /// The callable whose signature is being bound.
        callable: bray_symbols::CallableSymbolId,
        /// Whether a receiver symbol was supplied.
        receiver_present: bool,
        /// Whether source qualifiers declared a receiver mode.
        mode_present: bool,
        /// Whether the binding scope supplied a contextual self type.
        self_type_present: bool,
    },
    /// A canonical type expected to describe a callable has another type form.
    CallableTypeExpected {
        /// The callable type syntax being inspected.
        source: bray_declarations::SyntaxAnchor,
        /// The canonical type whose data is not callable.
        ty: bray_symbols::TypeId,
    },
    /// A deferred type template expected to describe a callable has another template form.
    CallableTypeTemplateExpected(bray_declarations::SyntaxAnchor),
    /// The compiler-known environment has no default heap-storage policy declaration.
    CompilerKnownHeapStoragePolicyUnavailable,
    /// An imported dependency identity has no corresponding package symbol record.
    ImportedPackageUnavailable(bray_symbols::PackageIdentity),
    /// An imported package cannot form the requested source-path root.
    ImportedPathUnavailable {
        /// The selected imported package symbol.
        package: bray_symbols::PackageSymbolId,
        /// The number of source path components supplied to the lookup.
        component_count: usize,
    },
    /// A bound-tree walk stopped without a more specific retained failure.
    BoundWalkStopped(bray_bound_tree::AnyBoundNodeId),
    /// Bound-tree or local-symbol construction rejected an exact relationship.
    Construction(BoundUnitConstructionError),
    /// Bound-unit assembly rejected the completed local structures.
    Assembly(BoundUnitAssemblyError),
    /// A callable signature template violated its typed contract.
    CallableSignature(bray_symbols::CallableSignatureTemplateError),
    /// A generic substitution violated its ordered parameter and argument shape.
    GenericSubstitution(bray_symbols::GenericSubstitutionShapeError),
}

impl<Upstream: Hash> Hash for BindingError<Upstream> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);

        match self {
            Self::CheckerInfrastructure(error) => error.hash(state),
            Self::SemanticValue(error) => error.hash(state),
            Self::Upstream(error) => error.hash(state),
            Self::MissingSyntax { source } => source.hash(state),
            Self::MissingOwner {
                source,
                owner,
                symbol,
            } => {
                source.hash(state);
                owner.hash(state);
                symbol.hash(state);
            }
            Self::MissingModule { source, owner }
            | Self::InvalidSurfaceName {
                source,
                symbol: owner,
            } => {
                source.hash(state);
                owner.hash(state);
            }
            Self::Construction(error) => error.hash(state),
            Self::Assembly(error) => error.hash(state),
            Self::CallableSignature(error) => hash_callable_signature_error(*error, state),
            Self::GenericSubstitution(error) => hash_generic_substitution_error(*error, state),
            Self::SyntaxContract(source) => source.hash(state),
            Self::GenericOwnerUnavailable(symbol) => symbol.hash(state),
            Self::CompilerKnownRepresentationUnavailable(role) => role.hash(state),
            Self::SymbolRecordUnavailable(symbol) => symbol.hash(state),
            Self::ModulePartRecordUnavailable(part) => part.hash(state),
            Self::DeclarationRecordUnavailable(declaration) => declaration.hash(state),
            Self::ContextualSelfUnavailable(source) => {
                source.hash(state);
            }
            Self::InvalidUnitKey { source, owner } => {
                source.hash(state);
                owner.hash(state);
            }
            Self::CallableParameterCountMismatch {
                source,
                callable,
                syntax_count,
                symbol_count,
            } => {
                source.hash(state);
                callable.hash(state);
                syntax_count.hash(state);
                symbol_count.hash(state);
            }
            Self::CallableParameterOwnerMismatch {
                source,
                callable,
                parameter,
            } => {
                source.hash(state);
                callable.hash(state);
                parameter.hash(state);
            }
            Self::ReceiverParameterOwnerMismatch {
                source,
                callable,
                receiver,
            } => {
                source.hash(state);
                callable.hash(state);
                receiver.hash(state);
            }
            Self::ReceiverContextMismatch {
                source,
                callable,
                receiver_present,
                mode_present,
                self_type_present,
            } => {
                source.hash(state);
                callable.hash(state);
                receiver_present.hash(state);
                mode_present.hash(state);
                self_type_present.hash(state);
            }
            Self::CallableTypeExpected { source, ty } => {
                source.hash(state);
                ty.hash(state);
            }
            Self::CallableTypeTemplateExpected(source) => source.hash(state),
            Self::ImportedPackageUnavailable(package) => package.hash(state),
            Self::ImportedPathUnavailable {
                package,
                component_count,
            } => {
                package.hash(state);
                component_count.hash(state);
            }
            Self::BoundWalkStopped(root) => root.hash(state),
            Self::Cancelled
            | Self::DependencyUnavailable
            | Self::IdentityCapacityExceeded
            | Self::RollbackFailed
            | Self::TransactionContextMismatch
            | Self::ControlTargetMismatch
            | Self::UnsupportedSyntax
            | Self::UnresolvedTypeTemplate
            | Self::CompilerKnownHeapStoragePolicyUnavailable => {}
        }
    }
}

fn hash_generic_substitution_error<H: Hasher>(
    error: bray_symbols::GenericSubstitutionShapeError,
    state: &mut H,
) {
    std::mem::discriminant(&error).hash(state);

    match error {
        bray_symbols::GenericSubstitutionShapeError::ArgumentCountMismatch {
            parameter_count,
            argument_count,
        } => {
            parameter_count.hash(state);
            argument_count.hash(state);
        }
        bray_symbols::GenericSubstitutionShapeError::ArgumentKindMismatch {
            ordinal,
            expected,
            actual,
        } => {
            ordinal.hash(state);
            expected.hash(state);
            actual.hash(state);
        }
        bray_symbols::GenericSubstitutionShapeError::OrdinalOverflow => {}
    }
}

fn hash_callable_signature_error<H: Hasher>(
    error: bray_symbols::CallableSignatureTemplateError,
    state: &mut H,
) {
    std::mem::discriminant(&error).hash(state);

    if let bray_symbols::CallableSignatureTemplateError::SemanticValue(error) = error {
        error.hash(state);
    }
}

impl<Upstream> From<BindingQueryError<Upstream>> for BindingError<Upstream> {
    fn from(error: BindingQueryError<Upstream>) -> Self {
        match error {
            BindingQueryError::Cancelled => Self::Cancelled,
            BindingQueryError::CheckerInfrastructure(error) => Self::CheckerInfrastructure(error),
            BindingQueryError::SemanticValue(error) => Self::SemanticValue(error),
            BindingQueryError::Upstream(error) => Self::Upstream(error),
            BindingQueryError::DependencyUnavailable => Self::DependencyUnavailable,
            BindingQueryError::MissingSyntax { source } => Self::MissingSyntax { source },
            BindingQueryError::MissingOwner {
                source,
                owner,
                symbol,
            } => Self::MissingOwner {
                source,
                owner,
                symbol,
            },
            BindingQueryError::MissingModule { source, owner } => {
                Self::MissingModule { source, owner }
            }
            BindingQueryError::InvalidSurfaceName { source, symbol } => {
                Self::InvalidSurfaceName { source, symbol }
            }
            BindingQueryError::Construction(error) => Self::Construction(error),
            BindingQueryError::Binding(error) => error,
            BindingQueryError::Assembly(error) => Self::Assembly(error),
        }
    }
}

impl BindingError {
    /// Widens a locally produced failure to a binding boundary with an upstream error type.
    pub fn with_upstream<Upstream>(self) -> BindingError<Upstream> {
        match self {
            Self::Cancelled => BindingError::Cancelled,
            Self::CheckerInfrastructure(error) => BindingError::CheckerInfrastructure(error),
            Self::SemanticValue(error) => BindingError::SemanticValue(error),
            Self::Upstream(error) => match error {},
            Self::DependencyUnavailable => BindingError::DependencyUnavailable,
            Self::MissingSyntax { source } => BindingError::MissingSyntax { source },
            Self::MissingOwner {
                source,
                owner,
                symbol,
            } => BindingError::MissingOwner {
                source,
                owner,
                symbol,
            },
            Self::MissingModule { source, owner } => BindingError::MissingModule { source, owner },
            Self::InvalidSurfaceName { source, symbol } => {
                BindingError::InvalidSurfaceName { source, symbol }
            }
            Self::IdentityCapacityExceeded => BindingError::IdentityCapacityExceeded,
            Self::RollbackFailed => BindingError::RollbackFailed,
            Self::TransactionContextMismatch => BindingError::TransactionContextMismatch,
            Self::ControlTargetMismatch => BindingError::ControlTargetMismatch,
            Self::UnsupportedSyntax => BindingError::UnsupportedSyntax,
            Self::SyntaxContract(source) => BindingError::SyntaxContract(source),
            Self::GenericOwnerUnavailable(symbol) => BindingError::GenericOwnerUnavailable(symbol),
            Self::CompilerKnownRepresentationUnavailable(role) => {
                BindingError::CompilerKnownRepresentationUnavailable(role)
            }
            Self::SymbolRecordUnavailable(symbol) => BindingError::SymbolRecordUnavailable(symbol),
            Self::ModulePartRecordUnavailable(part) => {
                BindingError::ModulePartRecordUnavailable(part)
            }
            Self::DeclarationRecordUnavailable(declaration) => {
                BindingError::DeclarationRecordUnavailable(declaration)
            }
            Self::ContextualSelfUnavailable(source) => {
                BindingError::ContextualSelfUnavailable(source)
            }
            Self::UnresolvedTypeTemplate => BindingError::UnresolvedTypeTemplate,
            Self::InvalidUnitKey { source, owner } => {
                BindingError::InvalidUnitKey { source, owner }
            }
            Self::CallableParameterCountMismatch {
                source,
                callable,
                syntax_count,
                symbol_count,
            } => BindingError::CallableParameterCountMismatch {
                source,
                callable,
                syntax_count,
                symbol_count,
            },
            Self::CallableParameterOwnerMismatch {
                source,
                callable,
                parameter,
            } => BindingError::CallableParameterOwnerMismatch {
                source,
                callable,
                parameter,
            },
            Self::ReceiverParameterOwnerMismatch {
                source,
                callable,
                receiver,
            } => BindingError::ReceiverParameterOwnerMismatch {
                source,
                callable,
                receiver,
            },
            Self::ReceiverContextMismatch {
                source,
                callable,
                receiver_present,
                mode_present,
                self_type_present,
            } => BindingError::ReceiverContextMismatch {
                source,
                callable,
                receiver_present,
                mode_present,
                self_type_present,
            },
            Self::CallableTypeExpected { source, ty } => {
                BindingError::CallableTypeExpected { source, ty }
            }
            Self::CallableTypeTemplateExpected(source) => {
                BindingError::CallableTypeTemplateExpected(source)
            }
            Self::CompilerKnownHeapStoragePolicyUnavailable => {
                BindingError::CompilerKnownHeapStoragePolicyUnavailable
            }
            Self::ImportedPackageUnavailable(package) => {
                BindingError::ImportedPackageUnavailable(package)
            }
            Self::ImportedPathUnavailable {
                package,
                component_count,
            } => BindingError::ImportedPathUnavailable {
                package,
                component_count,
            },
            Self::BoundWalkStopped(root) => BindingError::BoundWalkStopped(root),
            Self::Construction(error) => BindingError::Construction(error),
            Self::Assembly(error) => BindingError::Assembly(error),
            Self::CallableSignature(error) => BindingError::CallableSignature(error),
            Self::GenericSubstitution(error) => BindingError::GenericSubstitution(error),
        }
    }
}

impl<Upstream> From<bray_symbols::SemanticValueStoreError> for BindingError<Upstream> {
    fn from(error: bray_symbols::SemanticValueStoreError) -> Self {
        Self::SemanticValue(error)
    }
}

impl<Upstream> From<BoundUnitConstructionError> for BindingError<Upstream> {
    fn from(error: BoundUnitConstructionError) -> Self {
        Self::Construction(error)
    }
}

impl<Upstream> From<BoundUnitAssemblyError> for BindingError<Upstream> {
    fn from(error: BoundUnitAssemblyError) -> Self {
        Self::Assembly(error)
    }
}

pub(crate) type BindingResult<T, Upstream = std::convert::Infallible> =
    Result<T, BindingError<Upstream>>;

#[cfg(test)]
mod tests {
    use bray_symbols::{CallableSymbolId, FunctionSymbolId, PackageIdentity, SymbolId};

    use super::BindingError;
    use crate::unit::test_support::fixture;

    #[test]
    fn detailed_callable_and_import_failures_survive_upstream_widening() {
        let callable =
            CallableSymbolId::Function(FunctionSymbolId::from_symbol_id(SymbolId::new(17)));

        let source = fixture().key.source().syntax();

        let callable_error = BindingError::CallableParameterCountMismatch {
            source,
            callable,
            syntax_count: 2,
            symbol_count: 3,
        };

        assert_eq!(
            callable_error.with_upstream::<u8>(),
            BindingError::CallableParameterCountMismatch {
                source,
                callable,
                syntax_count: 2,
                symbol_count: 3,
            },
        );

        let package = PackageIdentity::try_new("dependency.package")
            .unwrap_or_else(|| panic!("test package identity must be valid"));

        assert_eq!(
            BindingError::ImportedPackageUnavailable(package.clone()).with_upstream::<u8>(),
            BindingError::ImportedPackageUnavailable(package),
        );
    }
}
