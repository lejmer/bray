use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use bray_binder::SymbolQueryProvider;
use bray_bound_tree::{BoundUnitKey, BoundUnitKind};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_lowering::LoweredUnit;
use bray_symbols::{
    CallableContractSet, CallableContractsQuery, CallableResultDependenciesQuery, CallableSymbolId,
    DependencyContractTemplateId, SymbolQueryRequest,
};

use super::Compilation;
use super::diagnostics::UnitDiagnosticOrder;
use crate::fact::{
    CancellationToken, CompilationFactKey, FactCell, FactQueryError, FactRuntimeFailure,
    QueryPriority, SynchronizationComponent,
};

/// Outputs own their completed source facts until their compilation is released.
/// Only native generation and interface publication request these units. Interactive
/// analysis and check-only requests continue using the bounded working caches.
pub(in crate::compilation) struct SourceOutputs {
    units: Mutex<BTreeMap<BoundUnitKey, Arc<FactCell<Arc<SourceOutputUnit>>>>>,
}

#[derive(Hash)]
pub(in crate::compilation) struct SourceOutputUnit {
    pub(in crate::compilation) lowered: Option<Arc<DiagnosticResult<Option<LoweredUnit>>>>,
    pub(in crate::compilation) diagnostics: Box<[DiagnosticBag]>,
    pub(in crate::compilation) nested: Box<[UnitDiagnosticOrder]>,
    pub(in crate::compilation) callable_contract:
        Option<Arc<DiagnosticResult<CallableContractSet>>>,
    pub(in crate::compilation) result_dependencies:
        Option<Arc<DiagnosticResult<DependencyContractTemplateId>>>,
}

impl SourceOutputUnit {
    pub(in crate::compilation) fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(DiagnosticBag::has_errors)
            || self
                .callable_contract
                .as_ref()
                .is_some_and(|contract| contract.diagnostics().has_errors())
            || self
                .lowered
                .as_ref()
                .is_some_and(|lowered| lowered.diagnostics().has_errors())
            || self
                .result_dependencies
                .as_ref()
                .is_some_and(|dependencies| dependencies.diagnostics().has_errors())
    }

    pub(in crate::compilation) fn diagnostic_bag(&self) -> DiagnosticBag {
        DiagnosticBag::merged_all(
            self.diagnostics
                .iter()
                .chain(
                    self.callable_contract
                        .iter()
                        .map(|contract| contract.diagnostics()),
                )
                .chain(self.lowered.iter().map(|lowered| lowered.diagnostics()))
                .chain(
                    self.result_dependencies
                        .iter()
                        .map(|dependencies| dependencies.diagnostics()),
                ),
        )
    }
}

impl SourceOutputs {
    pub(in crate::compilation) fn new() -> Self {
        Self {
            units: Mutex::new(BTreeMap::new()),
        }
    }

    pub(in crate::compilation) fn updated(
        &self,
        reusable: &std::collections::BTreeSet<CompilationFactKey>,
    ) -> Self {
        let units = self
            .units
            .lock()
            .expect("source output ownership must remain available");

        Self {
            units: Mutex::new(
                units
                    .iter()
                    .filter_map(|(key, cell)| {
                        let fact = CompilationFactKey::SourceOutputUnit(key.clone());

                        if reusable.contains(&fact) && cell.get_if_published(&fact).is_some() {
                            // Revised snapshots share outputs only after exact dependency reuse is proven.
                            Some((key.clone(), Arc::clone(cell)))
                        } else {
                            None
                        }
                    })
                    .collect(),
            ),
        }
    }

    fn requested_cell(
        &self,
        key: &BoundUnitKey,
    ) -> Result<Arc<FactCell<Arc<SourceOutputUnit>>>, FactQueryError> {
        let mut units =
            self.units
                .lock()
                .map_err(|_| FactRuntimeFailure::SynchronizationPoisoned {
                    component: SynchronizationComponent::CellMap,
                    fact: None,
                    task: None,
                })?;

        // Each actual output demand owns one stable unit identity and one shared publication.
        Ok(Arc::clone(
            units
                .entry(key.clone())
                .or_insert_with(|| Arc::new(FactCell::new())),
        ))
    }

    pub(in crate::compilation) fn published(
        &self,
        key: &BoundUnitKey,
    ) -> Result<Option<Arc<SourceOutputUnit>>, FactQueryError> {
        let units = self
            .units
            .lock()
            .map_err(|_| FactRuntimeFailure::SynchronizationPoisoned {
                component: SynchronizationComponent::CellMap,
                fact: None,
                task: None,
            })?;

        Ok(units.get(key).and_then(|cell| {
            cell.get_if_published(&CompilationFactKey::SourceOutputUnit(key.clone()))
                .map(Arc::clone)
        }))
    }
}

impl Compilation {
    pub(in crate::compilation) fn source_output_unit(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<SourceOutputUnit>, FactQueryError> {
        let cell = self.state.source_outputs.requested_cell(&key)?;

        let result = cell.get_or_compute_requested(
            &self.state.fact_runtime,
            CompilationFactKey::SourceOutputUnit(key.clone()),
            cancellation,
            |cancellation| {
                let callable = if key.kind() == BoundUnitKind::CallableBody {
                    self.symbol_graph()?
                        .symbol_for_key(key.declared_owner())
                        .and_then(CallableSymbolId::try_from_any)
                } else {
                    None
                };

                let binder = self.binding_context(cancellation)?;

                let callable_contract = callable
                    .map(|callable| {
                        match self.published_callable_contract(callable, cancellation)? {
                            Some(contract) => Ok(contract),
                            None => binder
                                .resolve_symbol_query(
                                    SymbolQueryRequest::<CallableContractsQuery>::new(callable),
                                )
                                .map_err(super::binder::binding_query_error),
                        }
                    })
                    .transpose()?;

                let (diagnostics, nested) =
                    self.semantic_unit_diagnostic_work(&key, cancellation)?;

                let has_errors = diagnostics.iter().any(DiagnosticBag::has_errors)
                    || callable_contract
                        .as_ref()
                        .is_some_and(|contract| contract.diagnostics().has_errors());

                // Invalid source cannot cross the checked-unit boundary into lowering.
                let lowered = if has_errors {
                    None
                } else {
                    // Lower while this unit's checking prerequisites are still in the working set.
                    Some(
                        self.lowered_unit_with_priority(
                            key.clone(),
                            cancellation,
                            self.state
                                .fact_runtime
                                .current_priority()?
                                .unwrap_or(QueryPriority::Normal),
                        )?,
                    )
                };

                let result_dependencies =
                    if has_errors || self.package_interface_export_request().is_none() {
                        None
                    } else {
                        callable
                            .map(|callable| {
                                binder
                                    .resolve_symbol_query(SymbolQueryRequest::<
                                        CallableResultDependenciesQuery,
                                    >::new(
                                        callable
                                    ))
                                    .map_err(super::binder::binding_query_error)
                            })
                            .transpose()?
                    };

                Ok(Arc::new(SourceOutputUnit {
                    lowered,
                    diagnostics: diagnostics.into_boxed_slice(),
                    nested: nested.into_boxed_slice(),
                    callable_contract,
                    result_dependencies,
                }))
            },
        )?;

        Ok(Arc::clone(result))
    }

    pub(in crate::compilation) fn published_source_output(
        &self,
        key: &BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Option<Arc<SourceOutputUnit>>, FactQueryError> {
        let Some(output) = self.state.source_outputs.published(key)? else {
            return Ok(None);
        };

        let fact = CompilationFactKey::SourceOutputUnit(key.clone());

        self.state.fact_runtime.check_request_cycle(&fact)?;
        cancellation.check()?;
        self.state.fact_runtime.record_completed_request(&fact)?;

        Ok(Some(output))
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write;
    use std::sync::Arc;

    use bray_bound_tree::BoundUnitKind;
    use bray_package_interface::{
        InterfaceLanguageRevision, InterfaceProductIdentity, InterfaceProductKind,
        PackageInterfaceIdentity,
    };

    use crate::fact::{CancellationToken, FactQueryError};
    use crate::test_support::{
        compilation, package_identity, package_version, source_function_body_key, source_input,
    };
    use crate::{
        Compilation, CompilationProfileConfiguration, CompilationProfileMode, CompilationRequest,
        PackageInterfaceExportRequest,
    };

    const IDENTITY_SOURCE: &str = r#"
        module test.package;

        func identity(pos input: bool) -> bool
        {
            return input;
        }
    "#;

    #[test]
    fn output_demand_reuses_source_work_beyond_the_analysis_working_set() {
        let count = 4_112;
        let mut source = String::from("module test.package;\n");

        for index in 0..count {
            writeln!(
                source,
                r#"
                func f{index}(pos input: bool) -> bool
                {{
                    return input;
                }}
                "#
            )
            .expect("fixture text must be writable");
        }

        let identity = PackageInterfaceIdentity::try_new(
            package_identity(),
            package_version(),
            InterfaceProductIdentity::try_new("library").expect("fixture product must be valid"),
            InterfaceProductKind::Library,
            "public",
        )
        .expect("fixture interface identity must be valid");

        let compilation = Compilation::load(
            CompilationRequest::new(package_identity(), vec![source_input(&source, 1)])
                .with_package_interface_export(PackageInterfaceExportRequest::new(
                    identity,
                    InterfaceLanguageRevision::new(0),
                ))
                .with_profile(CompilationProfileConfiguration::new(
                    CompilationProfileMode::Summary,
                )),
        )
        .expect("source-output fixture must load");

        let keys = compilation
            .declared_unit_keys()
            .expect("source-output inventory must resolve")
            .iter()
            .filter(|key| key.kind() == BoundUnitKind::CallableBody)
            .cloned()
            .collect::<Vec<_>>();

        assert_eq!(keys.len(), count);

        for key in &keys {
            let output = compilation
                .source_output_unit(key.clone(), &compilation.state.cancellation)
                .expect("requested source output must complete");

            assert!(
                !output
                    .lowered
                    .as_ref()
                    .expect("valid unit must lower")
                    .diagnostics()
                    .has_errors()
            );
        }

        let bundle = compilation
            .package_interface_export_bundle()
            .expect("requested interface must be available")
            .as_ref()
            .expect("requested interface must export");

        assert_eq!(bundle.executable_templates().len(), count);
        assert!(compilation.check_diagnostics().is_empty());

        for key in keys.iter().rev() {
            let first = compilation
                .source_output_unit(key.clone(), &compilation.state.cancellation)
                .expect("source output must remain owned");

            let second = compilation
                .lowered_unit(key.clone())
                .expect("later lowering must reuse the output publication");

            assert!(Arc::ptr_eq(
                first.lowered.as_ref().expect("valid unit must lower"),
                &second
            ));
        }

        let report = compilation
            .profile_report()
            .expect("fixture profile must be available");

        for id in [1_028, 1_032, 1_033, 1_073, 1_078] {
            let query = report
                .queries
                .iter()
                .find(|query| query.id == id)
                .expect("source output must record its source work");

            assert_eq!(
                query.evaluations,
                u64::try_from(count).expect("fixture count must fit u64")
            );
        }

        let guarantees = report
            .queries
            .iter()
            .find(|query| query.id == 1_079)
            .expect("whole-source guarantees must be checked");

        assert_eq!(guarantees.evaluations, 1);
    }

    #[test]
    fn check_only_does_not_create_source_outputs_or_lower_bodies() {
        let compilation = compilation(
            IDENTITY_SOURCE,
        );

        let key = source_function_body_key(&compilation, "identity");

        assert!(compilation.check_diagnostics().is_empty());

        assert!(
            compilation
                .state
                .source_outputs
                .published(&key)
                .expect("output lookup must succeed")
                .is_none()
        );

        assert_eq!(
            compilation.state.lowered_units.is_published(&key),
            Ok(false)
        );
    }

    #[test]
    fn invalid_source_outputs_preserve_diagnostics_without_lowering() {
        let source = r#"
            module test.package;

            func broken() -> bool
            {
                return missing;
            }
        "#;

        let baseline = compilation(source);
        let prepared = compilation(source);
        let key = source_function_body_key(&prepared, "broken");

        let output = prepared
            .source_output_unit(key.clone(), &prepared.state.cancellation)
            .expect("invalid source must retain its completed diagnostics");

        assert!(output.lowered.is_none());
        assert_eq!(prepared.state.lowered_units.is_published(&key), Ok(false));
        assert_eq!(prepared.check_diagnostics(), baseline.check_diagnostics());
    }

    #[test]
    fn unchanged_snapshots_share_dependency_validated_source_outputs() {
        let source =
            IDENTITY_SOURCE;

        let request = CompilationRequest::new(package_identity(), vec![source_input(source, 1)]);
        let compilation = Compilation::load(request).expect("source fixture must load");
        let key = source_function_body_key(&compilation, "identity");

        let original = compilation
            .source_output_unit(key, &compilation.state.cancellation)
            .expect("original source output must complete");

        let updated = compilation
            .updated(CompilationRequest::new(
                package_identity(),
                vec![source_input(source, 1)],
            ))
            .expect("unchanged snapshot must load");

        let key = source_function_body_key(&updated, "identity");

        let reused = updated
            .source_output_unit(key, &updated.state.cancellation)
            .expect("unchanged source output must remain available");

        assert!(original.result_dependencies.is_none());
        assert!(Arc::ptr_eq(&original, &reused));
    }

    #[test]
    fn source_outputs_are_shared_by_concurrent_demand_and_respect_cancellation() {
        let compilation = compilation(
            IDENTITY_SOURCE,
        );

        let key = source_function_body_key(&compilation, "identity");

        let outputs = std::thread::scope(|scope| {
            let workers = (0..4)
                .map(|_| {
                    scope.spawn(|| {
                        compilation
                            .source_output_unit(key.clone(), &compilation.state.cancellation)
                            .expect("concurrent source output must complete")
                    })
                })
                .collect::<Vec<_>>();

            workers
                .into_iter()
                .map(|worker| worker.join().expect("source worker must finish"))
                .collect::<Vec<_>>()
        });

        assert!(
            outputs
                .iter()
                .all(|output| Arc::ptr_eq(&outputs[0], output))
        );

        let cancellation = CancellationToken::new();

        cancellation.cancel();

        assert!(matches!(
            compilation.source_output_unit(key, &cancellation),
            Err(FactQueryError::Cancelled)
        ));
    }

    #[test]
    fn revised_sources_do_not_inherit_previous_output_publications() {
        let compilation = compilation(r#"
            module test.package;

            func value() -> bool
            {
                return true;
            }
        "#);

        let key = source_function_body_key(&compilation, "value");

        let original = compilation
            .source_output_unit(key, &compilation.state.cancellation)
            .expect("original source output must complete");

        let updated = compilation
            .updated_sources(vec![source_input(
                r#"
                module test.package;

                func value() -> bool
                {
                    return false;
                }
            "#,
                2,
            )])
            .expect("revised sources must load");

        let key = source_function_body_key(&updated, "value");

        assert!(
            updated
                .state
                .source_outputs
                .published(&key)
                .expect("output lookup must succeed")
                .is_none()
        );

        let revised = updated
            .source_output_unit(key, &updated.state.cancellation)
            .expect("revised source output must complete");

        assert_ne!(
            original
                .lowered
                .as_ref()
                .expect("original unit must lower")
                .value(),
            revised
                .lowered
                .as_ref()
                .expect("revised unit must lower")
                .value()
        );

        assert!(!Arc::ptr_eq(&original, &revised));
    }
}
