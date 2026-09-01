use bray_binder::{BindingError, BindingQueryError, BoundUnitBindingError};

use crate::fact::FactQueryError;

pub(in crate::compilation) type BindingQueryResult<T> =
    bray_binder::BindingQueryResult<T, FactQueryError>;

pub(in crate::compilation) fn binding_query_error<Upstream>(
    error: BindingQueryError<Upstream>,
) -> FactQueryError
where
    Upstream: Into<FactQueryError>,
{
    match error {
        BindingQueryError::Cancelled => FactQueryError::Cancelled,
        BindingQueryError::CheckerInfrastructure(error) => {
            FactQueryError::CheckerInfrastructure(error)
        }
        BindingQueryError::SemanticValue(error) => FactQueryError::SemanticValueStore(error),
        BindingQueryError::DependencyUnavailable => FactQueryError::BindingDependencyUnavailable,
        BindingQueryError::MissingSyntax { source } => FactQueryError::Binding(
            BoundUnitBindingError::Binding(BindingError::MissingSyntax { source }),
        ),
        BindingQueryError::MissingOwner {
            source,
            owner,
            symbol,
        } => FactQueryError::Binding(BoundUnitBindingError::Binding(BindingError::MissingOwner {
            source,
            owner,
            symbol,
        })),
        BindingQueryError::MissingModule { source, owner } => FactQueryError::Binding(
            BoundUnitBindingError::Binding(BindingError::MissingModule { source, owner }),
        ),
        BindingQueryError::InvalidSurfaceName { source, symbol } => FactQueryError::Binding(
            BoundUnitBindingError::Binding(BindingError::InvalidSurfaceName { source, symbol }),
        ),
        BindingQueryError::Construction(error) => {
            FactQueryError::Binding(BoundUnitBindingError::Construction(error))
        }
        BindingQueryError::Binding(error) => binding_error(error),
        BindingQueryError::Assembly(error) => {
            FactQueryError::Binding(BoundUnitBindingError::Assembly(error))
        }
        BindingQueryError::Upstream(error) => error.into(),
    }
}

pub(in crate::compilation) fn binding_error<Upstream>(
    error: BindingError<Upstream>,
) -> FactQueryError
where
    Upstream: Into<FactQueryError>,
{
    match error {
        BindingError::Cancelled => FactQueryError::Cancelled,
        BindingError::CheckerInfrastructure(error) => FactQueryError::CheckerInfrastructure(error),
        BindingError::SemanticValue(error) => FactQueryError::SemanticValueStore(error),
        BindingError::Upstream(error) => error.into(),
        BindingError::DependencyUnavailable => FactQueryError::BindingDependencyUnavailable,
        BindingError::MissingSyntax { source } => FactQueryError::Binding(
            BoundUnitBindingError::Binding(BindingError::MissingSyntax { source }),
        ),
        BindingError::MissingOwner {
            source,
            owner,
            symbol,
        } => FactQueryError::Binding(BoundUnitBindingError::Binding(BindingError::MissingOwner {
            source,
            owner,
            symbol,
        })),
        BindingError::MissingModule { source, owner } => FactQueryError::Binding(
            BoundUnitBindingError::Binding(BindingError::MissingModule { source, owner }),
        ),
        BindingError::InvalidSurfaceName { source, symbol } => FactQueryError::Binding(
            BoundUnitBindingError::Binding(BindingError::InvalidSurfaceName { source, symbol }),
        ),
        BindingError::SyntaxContract(source) => FactQueryError::Binding(
            BoundUnitBindingError::Binding(BindingError::SyntaxContract(source)),
        ),
        BindingError::GenericOwnerUnavailable(owner) => FactQueryError::Binding(
            BoundUnitBindingError::Binding(BindingError::GenericOwnerUnavailable(owner)),
        ),
        BindingError::CompilerKnownRepresentationUnavailable(role) => {
            FactQueryError::Binding(BoundUnitBindingError::Binding(
                BindingError::CompilerKnownRepresentationUnavailable(role),
            ))
        }
        BindingError::ModulePartRecordUnavailable(module_part) => FactQueryError::Binding(
            BoundUnitBindingError::Binding(BindingError::ModulePartRecordUnavailable(module_part)),
        ),
        BindingError::DeclarationRecordUnavailable(declaration) => FactQueryError::Binding(
            BoundUnitBindingError::Binding(BindingError::DeclarationRecordUnavailable(declaration)),
        ),
        BindingError::SymbolRecordUnavailable(symbol) => FactQueryError::Binding(
            BoundUnitBindingError::Binding(BindingError::SymbolRecordUnavailable(symbol)),
        ),
        BindingError::UnresolvedTraitApplication(source) => FactQueryError::Binding(
            BoundUnitBindingError::Binding(BindingError::UnresolvedTraitApplication(source)),
        ),
        BindingError::ContextualSelfUnavailable(source) => FactQueryError::Binding(
            BoundUnitBindingError::Binding(BindingError::ContextualSelfUnavailable(source)),
        ),
        BindingError::UnresolvedTypeTemplate => FactQueryError::Binding(
            BoundUnitBindingError::Binding(BindingError::UnresolvedTypeTemplate),
        ),
        BindingError::InvalidUnitKey { source, owner } => FactQueryError::Binding(
            BoundUnitBindingError::Binding(BindingError::InvalidUnitKey { source, owner }),
        ),
        BindingError::CallableParameterCountMismatch {
            source,
            callable,
            syntax_count,
            symbol_count,
        } => FactQueryError::Binding(BoundUnitBindingError::Binding(
            BindingError::CallableParameterCountMismatch {
                source,
                callable,
                syntax_count,
                symbol_count,
            },
        )),
        BindingError::CallableParameterOwnerMismatch {
            source,
            callable,
            parameter,
        } => FactQueryError::Binding(BoundUnitBindingError::Binding(
            BindingError::CallableParameterOwnerMismatch {
                source,
                callable,
                parameter,
            },
        )),
        BindingError::ReceiverParameterOwnerMismatch {
            source,
            callable,
            receiver,
        } => FactQueryError::Binding(BoundUnitBindingError::Binding(
            BindingError::ReceiverParameterOwnerMismatch {
                source,
                callable,
                receiver,
            },
        )),
        BindingError::ReceiverContextMismatch {
            source,
            callable,
            receiver_present,
            mode_present,
            self_type_present,
        } => FactQueryError::Binding(BoundUnitBindingError::Binding(
            BindingError::ReceiverContextMismatch {
                source,
                callable,
                receiver_present,
                mode_present,
                self_type_present,
            },
        )),
        BindingError::CallableTypeExpected { source, ty } => FactQueryError::Binding(
            BoundUnitBindingError::Binding(BindingError::CallableTypeExpected { source, ty }),
        ),
        BindingError::CallableTypeTemplateExpected(source) => FactQueryError::Binding(
            BoundUnitBindingError::Binding(BindingError::CallableTypeTemplateExpected(source)),
        ),
        BindingError::CompilerKnownHeapStoragePolicyUnavailable => FactQueryError::Binding(
            BoundUnitBindingError::Binding(BindingError::CompilerKnownHeapStoragePolicyUnavailable),
        ),
        BindingError::ImportedPackageUnavailable(package) => FactQueryError::Binding(
            BoundUnitBindingError::Binding(BindingError::ImportedPackageUnavailable(package)),
        ),
        BindingError::ImportedPathUnavailable {
            package,
            component_count,
        } => FactQueryError::Binding(BoundUnitBindingError::Binding(
            BindingError::ImportedPathUnavailable {
                package,
                component_count,
            },
        )),
        BindingError::BoundWalkStopped(root) => FactQueryError::Binding(
            BoundUnitBindingError::Binding(BindingError::BoundWalkStopped(root)),
        ),
        BindingError::Construction(error) => {
            FactQueryError::Binding(BoundUnitBindingError::Construction(error))
        }
        BindingError::Assembly(error) => {
            FactQueryError::Binding(BoundUnitBindingError::Assembly(error))
        }
        BindingError::CallableSignature(error) => error.into(),
        BindingError::GenericSubstitution(cause) => {
            crate::compilation::SemanticQueryFailure::GenericSubstitution { owner: None, cause }
                .into()
        }
        BindingError::IdentityCapacityExceeded => FactQueryError::Binding(
            BoundUnitBindingError::Binding(BindingError::IdentityCapacityExceeded),
        ),
        BindingError::RollbackFailed => {
            FactQueryError::Binding(BoundUnitBindingError::Binding(BindingError::RollbackFailed))
        }
        BindingError::TransactionContextMismatch => FactQueryError::Binding(
            BoundUnitBindingError::Binding(BindingError::TransactionContextMismatch),
        ),
        BindingError::ControlTargetMismatch => FactQueryError::Binding(
            BoundUnitBindingError::Binding(BindingError::ControlTargetMismatch),
        ),
        BindingError::UnsupportedSyntax => FactQueryError::Binding(BoundUnitBindingError::Binding(
            BindingError::UnsupportedSyntax,
        )),
    }
}

pub(in crate::compilation) fn semantic_value_binding_error(
    error: bray_symbols::SemanticValueStoreError,
) -> BindingQueryError<FactQueryError> {
    BindingQueryError::SemanticValue(error)
}

pub(in crate::compilation) fn callable_signature_binding_error(
    error: bray_symbols::CallableSignatureTemplateError,
) -> BindingQueryError<FactQueryError> {
    BindingQueryError::Upstream(error.into())
}

pub(in crate::compilation) fn semantic_query_binding_error(
    error: crate::compilation::SemanticQueryFailure,
) -> BindingQueryError<FactQueryError> {
    BindingQueryError::Upstream(error.into())
}

pub(in crate::compilation) fn semantic_contract_binding_error(
    context: crate::compilation::SemanticQueryContext,
    violation: crate::compilation::SemanticQueryViolation,
) -> BindingQueryError<FactQueryError> {
    semantic_query_binding_error(crate::compilation::SemanticQueryFailure::contract(
        context, violation,
    ))
}

pub(in crate::compilation) fn symbol_query_contract_binding_error(
    symbol: bray_symbols::AnySymbolId,
    kind: bray_symbols::SymbolQueryKind,
    violation: crate::compilation::SemanticQueryViolation,
) -> BindingQueryError<FactQueryError> {
    semantic_contract_binding_error(
        crate::compilation::SemanticQueryContext::SymbolQuery(crate::fact::SymbolQueryKey::new(
            symbol, kind,
        )),
        violation,
    )
}

#[cfg(test)]
mod tests {
    use bray_checker::CheckerInfrastructureError;
    use bray_symbols::{
        FunctionSymbolId, PackageIdentity, SemanticValueKind, SemanticValueStoreError, SymbolId,
        SymbolQueryKind,
    };

    use super::{binding_error, binding_query_error, symbol_query_contract_binding_error};
    use crate::compilation::binder::symbol::binder_error;
    use crate::compilation::{
        SemanticDataKind, SemanticQueryContext, SemanticQueryFailure, SemanticQueryViolation,
    };
    use crate::fact::{FactQueryError, SymbolQueryKey};

    #[test]
    fn checker_infrastructure_causes_survive_binding_query_boundaries() {
        for cause in [
            CheckerInfrastructureError::StorageFlow(
                bray_checker::CheckerStorageFlowFailure::FlowConstruction(
                    bray_bound_tree::StorageFlowBuildError::ForeignUnit,
                ),
            ),
            CheckerInfrastructureError::SemanticValueUnavailable,
            CheckerInfrastructureError::AtomicInitializerResultUnavailable,
        ] {
            let error = FactQueryError::CheckerInfrastructure(cause);

            assert_eq!(binding_query_error(binder_error(error.clone())), error);
        }
    }

    #[test]
    fn semantic_value_causes_survive_binding_query_boundaries() {
        let cause = SemanticValueStoreError::UnknownId {
            kind: SemanticValueKind::Type,
        };

        assert_eq!(
            binding_query_error(
                bray_binder::BindingQueryError::<FactQueryError>::SemanticValue(cause,)
            ),
            FactQueryError::SemanticValueStore(cause)
        );
    }

    #[test]
    fn exact_import_causes_survive_compilation_binding_boundaries() {
        let package = PackageIdentity::try_new("dependency.package")
            .unwrap_or_else(|| panic!("test package identity must be valid"));

        let cause = bray_binder::BindingError::ImportedPackageUnavailable(package);

        assert_eq!(
            binding_error(cause.clone()),
            FactQueryError::Binding(bray_binder::BoundUnitBindingError::Binding(cause)),
        );
    }

    #[test]
    fn symbol_query_contract_causes_retain_their_exact_query_identity() {
        let symbol = FunctionSymbolId::from_symbol_id(SymbolId::new(17)).into();
        let kind = SymbolQueryKind::CallableSignature;
        let violation = SemanticQueryViolation::Missing(SemanticDataKind::CallableSignature);

        let error = binding_query_error(symbol_query_contract_binding_error(
            symbol,
            kind,
            violation.clone(),
        ));

        let FactQueryError::SemanticQuery(error) = error else {
            panic!("symbol query contract failures must retain semantic-query ownership");
        };

        assert_eq!(
            error.cause(),
            &SemanticQueryFailure::contract(
                SemanticQueryContext::SymbolQuery(SymbolQueryKey::new(symbol, kind)),
                violation,
            )
        );
    }
}
