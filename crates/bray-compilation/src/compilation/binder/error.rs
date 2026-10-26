use bray_binder::{BindingError, BindingQueryError};

use crate::fact::FactQueryError;

pub(in crate::compilation) type BindingQueryResult<T> =
    bray_binder::BindingQueryResult<T, FactQueryError>;

pub(in crate::compilation) fn binding_query_error<Upstream>(
    error: BindingQueryError<Upstream>,
) -> FactQueryError
where
    Upstream: Into<FactQueryError> + std::fmt::Debug,
{
    binding_error(error.into())
}

pub(in crate::compilation) fn binding_error<Upstream>(
    error: BindingError<Upstream>,
) -> FactQueryError
where
    Upstream: Into<FactQueryError> + std::fmt::Debug,
{
    match error {
        BindingError::Cancelled => FactQueryError::Cancelled,
        BindingError::SemanticValue(error) => FactQueryError::SemanticValueStore(error),
        BindingError::Upstream(error) => error.into(),
        BindingError::Construction(error) => FactQueryError::BindingCapacity(error),
        BindingError::CallableSignature(error) => error.into(),
        BindingError::GenericSubstitution(cause) => {
            crate::compilation::SemanticQueryFailure::GenericSubstitution { owner: None, cause }
                .into()
        }
        BindingError::IdentityCapacityExceeded => {
            FactQueryError::BindingCapacity(bray_binder::BoundUnitConstructionError::LocalSymbol(
                bray_symbols::LocalSymbolBuildError::CapacityExceeded,
            ))
        }
        cause @ (BindingError::DependencyUnavailable
        | BindingError::MissingSyntax { .. }
        | BindingError::MissingOwner { .. }
        | BindingError::MissingModule { .. }
        | BindingError::InvalidSurfaceName { .. }
        | BindingError::SyntaxContract(_)
        | BindingError::GenericOwnerUnavailable(_)
        | BindingError::CompilerKnownRepresentationUnavailable(_)
        | BindingError::ModulePartRecordUnavailable(_)
        | BindingError::DeclarationRecordUnavailable(_)
        | BindingError::SymbolRecordUnavailable(_)
        | BindingError::ContextualSelfUnavailable(_)
        | BindingError::UnresolvedTypeTemplate
        | BindingError::InvalidUnitKey { .. }
        | BindingError::CallableParameterCountMismatch { .. }
        | BindingError::CallableParameterOwnerMismatch { .. }
        | BindingError::ReceiverParameterOwnerMismatch { .. }
        | BindingError::ReceiverContextMismatch { .. }
        | BindingError::CallableTypeExpected { .. }
        | BindingError::CallableTypeTemplateExpected(_)
        | BindingError::CompilerKnownHeapStoragePolicyUnavailable
        | BindingError::ImportedPackageUnavailable(_)
        | BindingError::ImportedPathUnavailable { .. }
        | BindingError::BoundWalkStopped(_)
        | BindingError::RollbackFailed
        | BindingError::ControlTargetMismatch
        | BindingError::UnsupportedSyntax
        | BindingError::TransactionContextMismatch) => {
            panic!("checked binding contract failed: {cause:?}")
        }
    }
}

pub(in crate::compilation) fn callable_signature_binding_error(
    error: bray_symbols::CallableSignatureTemplateError,
) -> BindingQueryError<FactQueryError> {
    BindingQueryError::Upstream(error.into())
}

pub(in crate::compilation) fn semantic_contract_binding_error(
    context: crate::compilation::SemanticQueryContext,
    violation: crate::compilation::SemanticQueryViolation,
) -> BindingQueryError<FactQueryError> {
    BindingQueryError::Upstream(
        crate::compilation::SemanticQueryFailure::contract(context, violation).into(),
    )
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

    use bray_symbols::{
        FunctionSymbolId, PackageIdentity, SemanticValueKind, SemanticValueStoreError, SymbolId,
        SymbolQueryKind,
    };

    use super::{binding_error, binding_query_error, symbol_query_contract_binding_error};
    use crate::compilation::{SemanticDataKind, SemanticQueryViolation};
    use crate::fact::FactQueryError;

    #[test]
    fn semantic_value_causes_survive_binding_query_boundaries() {
        let cause = SemanticValueStoreError::CapacityExhausted {
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
    fn checked_import_relationship_failures_panic_at_binding_boundary() {
        let cause = bray_binder::BindingError::<FactQueryError>::ImportedPackageUnavailable(
            PackageIdentity::try_new("dependency.package").expect("fixture package identity must be valid"),
        );

        let panic = std::panic::catch_unwind(|| binding_error(cause))
            .expect_err("compiler invariant must panic");

        let message = bray_testing::panic_payload_text(panic.as_ref());

        assert!(message.contains("ImportedPackageUnavailable"));
        assert!(message.contains("dependency.package"));
    }

    #[test]
    fn symbol_query_invariants_fail_at_the_owning_query_with_identity() {
        let symbol = FunctionSymbolId::from_symbol_id(SymbolId::new(17)).into();
        let kind = SymbolQueryKind::CallableSignature;

        let panic = std::panic::catch_unwind(|| {
            symbol_query_contract_binding_error(
                symbol,
                kind,
                SemanticQueryViolation::Missing(SemanticDataKind::CallableSignature),
            )
        })
        .expect_err("compiler invariant must panic");

        let message = bray_testing::panic_payload_text(panic.as_ref());

        assert!(message.contains("CallableSignature"));
        assert!(message.contains("17"));
    }
}
