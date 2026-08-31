use bray_binder::BindingQueryError;

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
        BindingQueryError::DependencyUnavailable => FactQueryError::BindingDependencyUnavailable,
        BindingQueryError::Upstream(error) => error.into(),
    }
}

#[cfg(test)]
mod tests {
    use bray_checker::CheckerInfrastructureError;

    use super::binding_query_error;
    use crate::compilation::binder::symbol::binder_error;
    use crate::fact::FactQueryError;

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
}
