use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_bound_tree::{
    BoundUnitKey, CallableProofKey, CallableProofObligation, CallableProofResult,
};
use bray_checker::{CallableProofFailure, check_callable_proof_dependencies};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticResult,
    SeverityKind,
};
use bray_source::SourceSpan;

use crate::compilation::Compilation;
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError, PublishedUnitResult};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum ProofOwner {
    Source(bray_bound_tree::BoundUnitId),
    Declaration(bray_symbols::CallableSymbolId),
}

impl Compilation {
    pub(in crate::compilation) fn declaration_callable_proofs(
        &self,
        definition: bray_symbols::CallableDefinitionId,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Vec<CallableProofObligation>>, FactQueryError> {
        if let Some(key) = self.callable_body_key(definition)? {
            let proofs = self.callable_proofs_with_cancellation(key, cancellation)?;

            // The caller retains the certified obligations independently of the cached query.
            return Ok(proofs.result().as_ref().clone());
        }

        let callable = definition.callable_symbol();
        let mut graph = BTreeMap::new();
        let mut diagnostics = DiagnosticBag::new();

        self.extend_declared_callable_proofs(
            &mut graph,
            BTreeSet::from([callable]),
            &mut diagnostics,
            cancellation,
        )?;

        let failures = check_callable_proof_dependencies(&graph, &BTreeMap::new());

        let proofs = graph
            .keys()
            .filter(|key| {
                key.owner() == ProofOwner::Declaration(callable) && !failures.contains_key(key)
            })
            .map(|key| key.obligation())
            .collect();

        Ok(DiagnosticResult::new(proofs, diagnostics))
    }

    pub(in crate::compilation) fn callable_proofs_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitResult<Vec<CallableProofObligation>>>, FactQueryError> {
        self.unit_query(
            &self.state.callable_proofs,
            CompilationFactKey::CallableProofs(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                Ok((
                    self.compute_callable_proofs(&key, cancellation)?,
                    Box::new([]),
                ))
            },
        )
    }

    fn compute_callable_proofs(
        &self,
        root: &BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Vec<CallableProofObligation>>, FactQueryError> {
        // The iterative dependency walk owns Arc-backed unit keys independently of cached bodies.
        let mut pending = vec![root.clone()];
        let mut visited = BTreeSet::new();

        let mut graph =
            BTreeMap::<CallableProofKey<ProofOwner>, BTreeSet<CallableProofKey<ProofOwner>>>::new();

        let mut imported = BTreeSet::new();
        let mut unavailable = BTreeMap::new();
        let mut origins = BTreeMap::new();
        let mut roots = Vec::new();
        let mut diagnostics = DiagnosticBag::new();

        while let Some(key) = pending.pop() {
            cancellation.check()?;

            if !visited.insert(key.clone()) {
                continue;
            }

            let semantics = self.body_semantics_with_cancellation(key.clone(), cancellation)?;
            let body = semantics.result().value();

            origins.insert(ProofOwner::Source(body.unit()), key.clone());
            diagnostics = diagnostics.merged(semantics.result().diagnostics());

            if !semantics.result().diagnostics().has_errors() && body.execution_proofs().is_empty()
            {
                continue;
            }

            let expressions =
                self.expression_semantics_with_cancellation(key.clone(), cancellation)?;

            diagnostics = diagnostics.merged(expressions.result().diagnostics());

            if semantics.result().diagnostics().has_errors()
                || expressions.result().diagnostics().has_errors()
            {
                continue;
            }

            for checked in body.execution_proofs() {
                let CallableProofResult::Candidate(candidate) = checked else {
                    continue;
                };

                let proof =
                    CallableProofKey::new(ProofOwner::Source(body.unit()), candidate.obligation());

                let edges = graph.entry(proof).or_default();

                if key == *root {
                    roots.push(proof);
                }

                for dependency in candidate.dependencies() {
                    let definition = dependency
                        .target()
                        .definition(expressions.result().value().selections());

                    let target = match definition {
                        Some(definition) => self.callable_body_key(definition)?,
                        None => None,
                    };

                    let Some(target) = target else {
                        if let Some(definition) = definition {
                            let callable = definition.callable_symbol();

                            edges.insert(CallableProofKey::new(
                                ProofOwner::Declaration(callable),
                                dependency.obligation(),
                            ));

                            imported.insert(callable);
                            continue;
                        }

                        unavailable.insert(
                            proof,
                            CallableProofFailure::UnverifiedCall(dependency.target().site()),
                        );

                        continue;
                    };

                    let bound = self.bound_unit_with_cancellation(target.clone(), cancellation)?;

                    edges.insert(CallableProofKey::new(
                        ProofOwner::Source(bound.result().value().unit()),
                        dependency.obligation(),
                    ));

                    pending.push(target);
                }
            }
        }

        self.extend_declared_callable_proofs(&mut graph, imported, &mut diagnostics, cancellation)?;

        let failures = check_callable_proof_dependencies(&graph, &unavailable);
        let mut proven = Vec::new();

        for proof in roots {
            if let Some(failure) = failures.get(&proof) {
                if let CallableProofFailure::MissingCandidate(missing) = failure
                    && let Some(key) = origins.get(&missing.owner())
                {
                    let semantics =
                        self.body_semantics_with_cancellation(key.clone(), cancellation)?;

                    if let Some(CallableProofResult::Unproven { diagnostic, .. }) = semantics
                        .result()
                        .value()
                        .execution_proofs()
                        .iter()
                        .find(|checked| checked.obligation() == missing.obligation())
                    {
                        // Failure publication owns the diagnostic independently of the cached body.
                        diagnostics.add(diagnostic.clone());
                    }
                }

                let anchor = root.source().syntax();

                let diagnostic = match proof.obligation() {
                    CallableProofObligation::Execution(guarantee) => Diagnostic::new(
                        DiagnosticId::new(anchor.full_range().start().bytes()),
                        DiagnosticKind::CheckingUnprovenExecutionGuarantee,
                        SeverityKind::Error,
                    )
                    .with_arg(DiagnosticArg::referenced_name(
                        guarantee.property().as_str(),
                    )),
                    CallableProofObligation::Postcondition(_) => Diagnostic::new(
                        DiagnosticId::new(anchor.full_range().start().bytes()),
                        DiagnosticKind::CheckingUnprovenPostcondition,
                        SeverityKind::Error,
                    ),
                    CallableProofObligation::Finalization { .. }
                    | CallableProofObligation::TypeFinalization { .. } => Diagnostic::new(
                        DiagnosticId::new(anchor.full_range().start().bytes()),
                        DiagnosticKind::CheckingUnprovenFinalizationCompletion,
                        SeverityKind::Error,
                    ),
                    CallableProofObligation::SynchronousDestruction { .. } => Diagnostic::new(
                        DiagnosticId::new(anchor.full_range().start().bytes()),
                        DiagnosticKind::CheckingUnprovenExecutionGuarantee,
                        SeverityKind::Error,
                    )
                    .with_arg(DiagnosticArg::referenced_name(
                        bray_symbols::ExecutionProperty::Pure.as_str(),
                    )),
                };

                diagnostics.add(
                    diagnostic.with_primary_span(SourceSpan::new(
                        anchor.source_id(),
                        anchor.full_range(),
                    )),
                );
            } else {
                proven.push(proof.obligation());
            }
        }

        Ok(DiagnosticResult::new(proven, diagnostics))
    }

    fn extend_declared_callable_proofs(
        &self,
        graph: &mut BTreeMap<CallableProofKey<ProofOwner>, BTreeSet<CallableProofKey<ProofOwner>>>,
        mut pending: BTreeSet<bray_symbols::CallableSymbolId>,
        diagnostics: &mut DiagnosticBag,
        cancellation: &CancellationToken,
    ) -> Result<(), FactQueryError> {
        let binding = self.binding_context(cancellation)?;
        let mut visited = BTreeSet::new();

        let context = crate::compilation::checker::CompilationCheckerContext::new(
            self.binding_context(cancellation)?,
        );

        while let Some(callable) = pending.pop_first() {
            cancellation.check()?;

            if !visited.insert(callable) {
                continue;
            }

            let provided = crate::compilation::checker::checker_result(
                match bray_checker::compiler_projection_guarantees(&context, callable) {
                    Ok(properties) => bray_checker::CheckerOutcome::Complete(
                        DiagnosticResult::without_diagnostics(properties),
                    ),
                    Err(error) => error.into(),
                },
            )?;

            for guarantee in provided.value() {
                graph
                    .entry(CallableProofKey::new(
                        ProofOwner::Declaration(callable),
                        CallableProofObligation::Execution(*guarantee),
                    ))
                    .or_default();
            }

            let Some(address) = binding
                .imported_semantic_address(callable.into_any())
                .map_err(crate::compilation::binder::binding_query_error)?
            else {
                let assertions = self.foreign_callable_assertions(callable, cancellation)?;
                *diagnostics = diagnostics.merged(assertions.diagnostics());

                if !assertions.diagnostics().has_errors() {
                    for obligation in assertions.value() {
                        graph
                            .entry(CallableProofKey::new(
                                ProofOwner::Declaration(callable),
                                (*obligation).into(),
                            ))
                            .or_default();
                    }
                }

                continue;
            };

            let records = self.imported_semantics_with_cancellation(
                crate::fact::ImportedSemanticRecordKey::new(
                    address.interface(),
                    address.symbol(),
                    bray_package_interface::InterfaceSemanticRecordKind::CallableContracts,
                ),
                cancellation,
            )?;

            *diagnostics = diagnostics.merged(records.diagnostics());

            if records.diagnostics().has_errors() {
                continue;
            }

            let [bray_package_interface::ImportedSemanticRecord::CallableContracts(contract)] =
                records.value().as_ref()
            else {
                continue;
            };

            if contract.evidence().first().is_some_and(|evidence| {
                evidence.origin() == bray_symbols::CallableEvidenceOrigin::ForeignAssertion
            }) {
                let boundary = self.imported_native_boundary_with_cancellation(
                    callable.into_any(),
                    cancellation,
                )?;

                if !boundary.is_some_and(|boundary| {
                    boundary.direction() == bray_symbols::ForeignCallableDirection::Import
                        && matches!(
                            boundary.kind(),
                            bray_package_interface::InterfaceNativeBoundaryKind::Callable
                        )
                }) {
                    continue;
                }
            }

            for evidence in contract.evidence() {
                let key = CallableProofKey::new(
                    ProofOwner::Declaration(callable),
                    evidence.obligation().into(),
                );

                let dependencies = graph.entry(key).or_default();

                for (target, obligation) in evidence.dependencies() {
                    dependencies.insert(CallableProofKey::new(
                        ProofOwner::Declaration(*target),
                        (*obligation).into(),
                    ));

                    pending.insert(*target);
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::{compilation, source_function_body_key};
    use bray_diagnostics::DiagnosticKind;

    #[test]
    fn task_admission_rejection_retains_the_inactive_frame_for_caller_cleanup() {
        let compilation = compilation(
            "module app; async func leaf() {} async func main() { let task = leaf().start(); }",
        );

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "main"))
            .unwrap();

        assert!(
            !lowered.diagnostics().has_errors(),
            "{:?}",
            lowered.diagnostics()
        );

        let mir = lowered.value().as_ref().unwrap().mir().unwrap();

        let retained = mir
            .operations()
            .iter()
            .find_map(|operation| match operation.kind() {
                bray_ir::MirOperationKind::Async(bray_ir::MirAsyncOperation::StartTask {
                    value: bray_ir::MirOperand::Copy(frame),
                    ..
                }) => Some(frame.storage()),
                _ => None,
            })
            .expect("start must borrow retained inactive-frame storage until publication");

        assert!(mir.operations().iter().any(|operation| matches!(
            operation.kind(),
            bray_ir::MirOperationKind::PanicReport(bray_ir::MirPanicCause::TaskAdmission)
        )));

        assert!(
            mir.operations()
                .iter()
                .any(|operation| matches!(operation.kind(),
            bray_ir::MirOperationKind::Cleanup { place, .. } if place.storage() == retained))
        );
    }

    #[test]
    fn synchronous_destruction_requires_checked_remainder_evidence() {
        for (owner_contract, leaf_contract, body, accepted) in [
            ("executes(pure, total)", "executes(pure, total)", "", true),
            ("", "executes(pure, total)", "", true),
            ("executes(pure, total)", "", "", false),
            (
                "executes(pure, total)",
                "executes(pure, total)",
                "panic(1);",
                false,
            ),
        ] {
            let source = format!(
                concat!(
                    "module app; ",
                    "struct Owner {{ leaf: Leaf; destruct() {owner_contract} {{}} }} ",
                    "struct Leaf {{ async finalize() {leaf_contract} {{ {body} }} destruct() executes(pure, total) {{}} }} ",
                    "func main() {{ let owner = Owner {{ leaf = Leaf {{}} }}; }}",
                ),
                owner_contract = owner_contract,
                leaf_contract = leaf_contract,
                body = body
            );

            let compilation = compilation(&source);
            let root = source_function_body_key(&compilation, "main");

            let proofs = compilation
                .callable_proofs_with_cancellation(root, &compilation.state.cancellation)
                .unwrap();

            assert_eq!(
                !proofs.result().diagnostics().has_errors(),
                accepted,
                "{source}: {:?}",
                proofs.result()
            );

            assert_eq!(
                proofs.result().value().iter().any(|proof| matches!(
                    proof,
                    bray_bound_tree::CallableProofObligation::SynchronousDestruction { .. }
                )),
                accepted,
                "{source}: {:?}",
                proofs.result()
            );

            assert!(
                !proofs.result().value().iter().any(|proof| matches!(
                    proof,
                    bray_bound_tree::CallableProofObligation::Finalization { .. }
                )),
                "{source}: {:?}",
                proofs.result()
            );

            if accepted {
                let lowered = compilation
                    .lowered_unit(source_function_body_key(&compilation, "main"))
                    .unwrap();

                assert!(!lowered.diagnostics().has_errors(), "{source}: {lowered:?}");

                assert!(
                    lowered
                        .value()
                        .as_ref()
                        .and_then(bray_lowering::LoweredUnit::mir)
                        .is_some(),
                    "{source}: {lowered:?}"
                );
            }
        }
    }

    #[test]
    fn callable_proofs_preserve_dependency_expression_diagnostics() {
        let compilation = compilation(
            "module app; func leaf() -> bool executes(pure, total) { return 0; } func root() -> bool executes(pure, total) { return leaf(); }",
        );

        let leaf = source_function_body_key(&compilation, "leaf");

        let expressions = compilation
            .expression_semantics_with_cancellation(leaf, &compilation.state.cancellation)
            .unwrap();

        let errors = expressions
            .result()
            .diagnostics()
            .iter()
            .filter(|diagnostic| diagnostic.severity() == bray_diagnostics::SeverityKind::Error)
            .collect::<Vec<_>>();

        assert!(
            !errors.is_empty(),
            "{:?}",
            expressions.result().diagnostics()
        );

        let root = source_function_body_key(&compilation, "root");

        let proofs = compilation
            .callable_proofs_with_cancellation(root, &compilation.state.cancellation)
            .unwrap();

        for error in errors {
            assert!(
                proofs
                    .result()
                    .diagnostics()
                    .iter()
                    .any(|diagnostic| diagnostic == error),
                "missing {error:?}: {:?}",
                proofs.result().diagnostics()
            );
        }
    }

    #[test]
    fn required_dependency_proofs_publish_the_original_unproven_postcondition() {
        let compilation = compilation(
            "module app; func leaf() -> bool ensures(result) { return false; } func root() -> bool when(true) { ensures(result) } { return leaf(); }",
        );

        let leaf = source_function_body_key(&compilation, "leaf");

        let semantics = compilation
            .body_semantics_with_cancellation(leaf, &compilation.state.cancellation)
            .unwrap();

        assert!(
            !semantics.result().diagnostics().has_errors(),
            "{:?}",
            semantics.result().diagnostics()
        );

        let rejection = semantics
            .result()
            .value()
            .execution_proofs()
            .iter()
            .find_map(|proof| match proof {
                bray_bound_tree::CallableProofResult::Unproven { diagnostic, .. } => {
                    Some(diagnostic)
                }
                bray_bound_tree::CallableProofResult::Candidate(_) => None,
            })
            .expect("the optional proof retains its rejection reason");

        let root = source_function_body_key(&compilation, "root");

        let proofs = compilation
            .callable_proofs_with_cancellation(root, &compilation.state.cancellation)
            .unwrap();

        assert!(
            proofs
                .result()
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic == rejection),
            "{:?}",
            proofs.result().diagnostics()
        );

        assert!(rejection.primary_span().is_some());
    }

    #[test]
    fn certified_scope_completion_lowers_only_the_remaining_destruction() {
        let compilation = compilation(
            "module app; struct Resource { finalize() executes(pure, total) {} destruct() {} } func root(pos value: Resource) {}",
        );

        let key = source_function_body_key(&compilation, "root");

        let proofs = compilation
            .callable_proofs_with_cancellation(key.clone(), &compilation.state.cancellation)
            .unwrap();

        assert!(
            !proofs.result().diagnostics().has_errors(),
            "{:?}",
            proofs.result().diagnostics()
        );

        assert!(
            proofs.result().value().iter().any(|proof| matches!(
                proof,
                bray_bound_tree::CallableProofObligation::Finalization { .. }
            )),
            "{:?}",
            proofs.result().value()
        );

        let lowered = compilation.lowered_unit(key).unwrap();

        assert!(
            !lowered.diagnostics().has_errors(),
            "{:?}",
            lowered.diagnostics()
        );

        let mir = lowered.value().as_ref().unwrap().mir().unwrap();

        assert!(
            mir.operations()
                .iter()
                .any(|operation| matches!(operation.kind(), bray_ir::MirOperationKind::Destroy(_)))
        );
    }

    #[test]
    fn unverified_finalizer_promises_cannot_discharge_scope_cleanup() {
        let compilation = compilation(
            "module app; struct Resource { mut pending: bool; finalize() executes(pure, total) { self.pending = false; } } func root(pos value: Resource) {}",
        );

        let key = source_function_body_key(&compilation, "root");

        let proofs = compilation
            .callable_proofs_with_cancellation(key.clone(), &compilation.state.cancellation)
            .unwrap();

        assert!(
            proofs
                .result()
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.kind()
                    == DiagnosticKind::CheckingUnprovenFinalizationCompletion),
            "{:?}",
            proofs.result().diagnostics()
        );

        assert!(compilation.lowered_unit(key).unwrap().value().is_none());
    }

    #[test]
    fn domain_completion_selects_a_fallible_finalizers_successful_no_work_domain() {
        let compilation = compilation(
            "module app; struct Resource { mut pending: bool; mut func close() ensures(!self.pending) { self.pending = false; } finalize() -> Result<unit, unit> when(!self.pending) { executes(pure, total) ensures(result matches Ok(_)) } { if !self.pending { return Ok(unit); } return Error(unit); } } func root(pos mut value: Resource) { value.close(); }",
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{diagnostics:?}");

        let key = source_function_body_key(&compilation, "root");

        let proofs = compilation
            .callable_proofs_with_cancellation(key, &compilation.state.cancellation)
            .unwrap();

        assert!(
            !proofs.result().diagnostics().has_errors(),
            "{:?}",
            proofs.result().diagnostics()
        );

        assert!(
            proofs.result().value().iter().any(|proof| matches!(
                proof,
                bray_bound_tree::CallableProofObligation::Finalization { .. }
            )),
            "{:?}",
            proofs.result().value()
        );
    }

    #[test]
    fn reopened_values_and_transferred_values_have_no_scope_completion_certificate() {
        for body in ["value.close(); value.pending = true;", "return value;"] {
            let result = if body.starts_with("return") {
                "-> Resource"
            } else {
                ""
            };

            let source = format!(
                "module app; struct Resource {{ mut pending: bool; mut func close() ensures(!self.pending) {{ self.pending = false; }} finalize() -> Result<unit, unit> when(!self.pending) {{ executes(pure, total) ensures(result matches Ok(_)) }} {{ if !self.pending {{ return Ok(unit); }} return Error(unit); }} }} func root(pos mut value: Resource) {result} {{ {body} }}"
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(
                diagnostics.iter().any(|diagnostic| diagnostic.kind()
                    == DiagnosticKind::CheckingUnresolvedFinalization),
                !body.starts_with("return"),
                "{body}: {diagnostics:?}"
            );

            let key = source_function_body_key(&compilation, "root");

            let proofs = compilation
                .callable_proofs_with_cancellation(key, &compilation.state.cancellation)
                .unwrap();

            assert_eq!(
                proofs.result().diagnostics().has_errors(),
                !body.starts_with("return"),
                "{:?}",
                proofs.result().diagnostics()
            );

            assert!(
                !proofs.result().value().iter().any(|proof| matches!(
                    proof,
                    bray_bound_tree::CallableProofObligation::Finalization { .. }
                )),
                "{body}: {:?}",
                proofs.result().value()
            );
        }
    }

    #[test]
    fn possibly_fallible_finalization_requires_completion_on_ordinary_exits() {
        for (result, body) in [("", ""), ("-> Result<unit, unit>", "return Error(unit);")] {
            let source = format!(
                "module app; struct Resource {{ finalize() -> Result<unit, unit> {{ return Error(unit); }} }} func root(pos value: Resource) {result} {{ {body} }}"
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            let unresolved = diagnostics
                .iter()
                .find(|diagnostic| {
                    diagnostic.kind() == DiagnosticKind::CheckingUnresolvedFinalization
                })
                .unwrap_or_else(|| panic!("missing ordinary-exit diagnostic: {diagnostics:?}"));

            assert!(unresolved.primary_span().is_some());
            assert!(!unresolved.args().is_empty());

            assert!(
                compilation
                    .lowered_unit(source_function_body_key(&compilation, "root"))
                    .unwrap()
                    .value()
                    .is_none()
            );
        }
    }

    #[test]
    fn direct_field_updates_and_moves_establish_completion_without_a_domain_call() {
        for body in [
            "value.pending = false;",
            "value.pending = false; let moved = value;",
        ] {
            let source = format!(
                "module app; struct Resource {{ mut pending: bool; finalize() -> Result<unit, unit> when(!self.pending) {{ executes(pure, total) ensures(result matches Ok(_)) }} {{ if !self.pending {{ return Ok(unit); }} return Error(unit); }} }} func root(pos mut value: Resource) {{ {body} }}"
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert!(!diagnostics.has_errors(), "{body}: {diagnostics:?}");

            let key = source_function_body_key(&compilation, "root");

            let proofs = compilation
                .callable_proofs_with_cancellation(key, &compilation.state.cancellation)
                .unwrap();

            assert!(
                proofs.result().value().iter().any(|proof| matches!(
                    proof,
                    bray_bound_tree::CallableProofObligation::Finalization { .. }
                )),
                "{body}: {:?}",
                proofs.result().value()
            );
        }
    }

    #[test]
    fn represented_owners_retain_fallible_child_obligations() {
        for ty in [
            "Wrapper<Resource>",
            "(Resource, bool)",
            "[Resource; 2]",
            "Resource?",
            "Payload",
        ] {
            let source = format!(
                "module app; struct Resource {{ finalize() -> Result<unit, unit> {{ return Error(unit); }} }} struct Wrapper<T> {{ inner: T; }} union Payload {{ Present(pos value: Resource); Absent; }} func root(pos value: {ty}) {{}}"
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert!(
                diagnostics.iter().any(|diagnostic| diagnostic.kind()
                    == DiagnosticKind::CheckingUnresolvedFinalization),
                "{ty}: {diagnostics:?}"
            );

            assert!(
                compilation
                    .lowered_unit(source_function_body_key(&compilation, "root"))
                    .unwrap()
                    .value()
                    .is_none(),
                "{ty}"
            );
        }
    }

    #[test]
    fn borrowed_and_transferred_wrappers_do_not_end_child_ownership() {
        for (ty, result, body) in [
            ("&Wrapper", "", ""),
            ("Wrapper", "-> Wrapper", "return value;"),
        ] {
            let source = format!(
                "module app; struct Resource {{ finalize() -> Result<unit, unit> {{ return Error(unit); }} }} struct Wrapper {{ inner: Resource; }} func root(pos value: {ty}) {result} {{ {body} }}"
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert!(!diagnostics.has_errors(), "{ty}: {diagnostics:?}");
        }
    }

    #[test]
    fn represented_outcomes_retain_non_generic_owned_payload_cleanup() {
        for (ty, cleanup) in [
            ("RunResult<unit>", true),
            ("(RunResult<unit>, bool)", true),
            ("Result<unit, RunResult<unit>>", true),
            ("Result<unit, unit>", false),
            ("ConversionError", false),
            ("&RunResult<unit>", false),
        ] {
            let compilation = compilation(&format!("module app; func root(pos value: {ty}) {{}}"));
            let diagnostics = compilation.check_diagnostics();

            assert!(!diagnostics.has_errors(), "{ty}: {diagnostics:?}");

            let key = source_function_body_key(&compilation, "root");
            let analysis = compilation.async_analysis(key.clone()).unwrap();

            assert_eq!(
                analysis
                    .value()
                    .scope_exits()
                    .iter()
                    .any(|exit| !exit.lifecycle_resolution().is_empty()),
                cleanup,
                "{ty}: {analysis:?}"
            );

            let lowered = compilation.lowered_unit(key).unwrap();

            assert!(
                !lowered.diagnostics().has_errors(),
                "{ty}: {:?}",
                lowered.diagnostics()
            );

            let mir = lowered.value().as_ref().unwrap().mir().unwrap();

            assert_eq!(
                mir.operations().iter().any(|operation| matches!(
                    operation.kind(),
                    bray_ir::MirOperationKind::Cleanup {
                        phase: bray_ir::MirCleanupPhase::LifecycleResolution,
                        ..
                    }
                )),
                cleanup,
                "{ty}: {mir:?}"
            );
        }
    }

    #[test]
    fn panic_cleanup_retains_pending_fallible_finalization() {
        let compilation = compilation(
            "module app; struct Resource { finalize() -> Result<unit, unit> { return Error(unit); } } func root(pos value: Resource) { panic(\"stop\"); }",
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{diagnostics:?}");

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "root"))
            .unwrap();

        assert!(
            !lowered.diagnostics().has_errors(),
            "{:?}",
            lowered.diagnostics()
        );

        let mir = lowered.value().as_ref().unwrap().mir().unwrap();

        assert!(mir.operations().iter().any(|operation| matches!(
            operation.kind(),
            bray_ir::MirOperationKind::Cleanup {
                phase: bray_ir::MirCleanupPhase::LifecycleResolution,
                ..
            }
        )));
    }

    #[test]
    fn constructed_values_prove_completion_from_their_selected_fields() {
        for pending in [false, true] {
            let source = format!(
                "module app; struct Resource {{ mut pending: bool; finalize() -> Result<unit, unit> when(!self.pending) {{ executes(pure, total) ensures(result matches Ok(_)) }} {{ if !self.pending {{ return Ok(unit); }} return Error(unit); }} }} func root() {{ let value = Resource {{ pending = {pending} }}; }}"
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(
                diagnostics.has_errors(),
                pending,
                "{pending}: {diagnostics:?}"
            );

            if pending {
                assert!(
                    diagnostics.iter().any(|diagnostic| diagnostic.kind()
                        == DiagnosticKind::CheckingUnresolvedFinalization),
                    "{diagnostics:?}"
                );
            }
        }
    }

    #[test]
    fn completed_represented_children_keep_destruction_in_their_checked_partition() {
        for (declarations, ty, body) in [
            (
                "struct Wrapper { mut inner: Resource; }",
                "Wrapper",
                "value.inner.pending = false;",
            ),
            (
                "struct Wrapper { mut inner: Inner; } struct Inner { mut resource: Resource; }",
                "Wrapper",
                "value.inner.resource.pending = false;",
            ),
            ("", "(Resource, bool)", "value.0.pending = false;"),
        ] {
            let source = format!(
                "module app; struct Resource {{ mut pending: bool; finalize() -> Result<unit, unit> when(!self.pending) {{ executes(pure, total) ensures(result matches Ok(_)) }} {{ if !self.pending {{ return Ok(unit); }} return Error(unit); }} destruct() {{}} }} {declarations} func root(pos mut value: {ty}) {{ {body} }}"
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert!(!diagnostics.has_errors(), "{body}: {diagnostics:?}");

            let key = source_function_body_key(&compilation, "root");

            let proofs = compilation
                .callable_proofs_with_cancellation(key.clone(), &compilation.state.cancellation)
                .unwrap();

            assert!(
                proofs.result().value().iter().any(|proof| matches!(
                    proof,
                    bray_bound_tree::CallableProofObligation::Finalization { part: Some(_), .. }
                )),
                "{body}: {:?}",
                proofs.result()
            );

            let lowered = compilation.lowered_unit(key).unwrap();

            assert!(
                !lowered.diagnostics().has_errors(),
                "{body}: {:?}",
                lowered.diagnostics()
            );

            let mir = lowered.value().as_ref().unwrap().mir().unwrap();

            assert!(mir.operations().iter().any(|operation| matches!(operation.kind(), bray_ir::MirOperationKind::Destroy(place) if !place.projections().is_empty())), "{body}");
        }
    }

    #[test]
    fn boxed_completion_uses_the_stored_value_observation() {
        for pending in [false, true] {
            let source = format!(
                "module app; struct Resource {{ mut pending: bool; \
                 finalize() -> Result<unit, unit> when(!self.pending) \
                 {{ executes(pure, total) ensures(result matches Ok(_)) }} \
                 {{ if !self.pending {{ return Ok(unit); }} return Error(unit); }} }} \
                 func root(pos value: box Resource) \
                 requires(value matches box(Resource {{ pending = {pending} }})) {{}}"
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(
                diagnostics.has_errors(),
                pending,
                "{source}: {diagnostics:?}"
            );

            if !pending {
                let lowered = compilation
                    .lowered_unit(source_function_body_key(&compilation, "root"))
                    .unwrap();

                assert!(!lowered.diagnostics().has_errors(), "{lowered:?}");
                assert!(lowered.value().is_some(), "{lowered:?}");
            }
        }
    }

    #[test]
    fn boxed_completion_survives_projection_and_follows_mutation() {
        for (body, complete) in [
            ("", true),
            ("resource.pending = true;", false),
            ("resource.pending = true; resource.pending = false;", true),
        ] {
            let source = format!(
                "module app; struct Resource {{ mut pending: bool; \
                 finalize() -> Result<unit, unit> when(!self.pending) \
                 {{ executes(pure, total) ensures(result matches Ok(_)) }} \
                 {{ if !self.pending {{ return Ok(unit); }} return Error(unit); }} }} \
                 func root(pos mut value: box Resource) \
                 requires(value matches box(Resource {{ pending = false }})) {{ match value {{ case box(resource) {{ {body} }} }}; }}"
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                complete,
                "{body}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn observing_pattern_bindings_update_their_owned_completion_observation() {
        for (declarations, ty, pattern) in [
            (
                "struct Wrapper { mut inner: Resource; }",
                "Wrapper",
                "Wrapper { inner = resource }",
            ),
            ("", "(Resource, bool)", "(resource, _)"),
            ("", "[Resource; 1]", "[resource]"),
        ] {
            for pending in [false, true] {
                let source = format!(
                    "module app; struct Resource {{ mut pending: bool; \
                     finalize() -> Result<unit, unit> when(!self.pending) \
                     {{ executes(pure, total) ensures(result matches Ok(_)) }} \
                     {{ if !self.pending {{ return Ok(unit); }} return Error(unit); }} }} \
                     {declarations} func root(pos mut value: {ty}) \
                     {{ match value {{ case {pattern} {{ resource.pending = {pending}; }} }}; }}"
                );

                let compilation = compilation(&source);
                let diagnostics = compilation.check_diagnostics();

                assert_eq!(
                    diagnostics.has_errors(),
                    pending,
                    "{source}: {diagnostics:?}"
                );
            }
        }
    }

    #[test]
    fn one_completed_array_element_does_not_discharge_its_siblings() {
        let compilation = compilation(
            "module app; struct Resource { mut pending: bool; \
             finalize() -> Result<unit, unit> when(!self.pending) \
             { executes(pure, total) ensures(result matches Ok(_)) } \
             { if !self.pending { return Ok(unit); } return Error(unit); } } \
             func root(pos mut value: [Resource; 2]) \
             { match value { case [resource, _] { resource.pending = false; } }; }",
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.kind()
                    == DiagnosticKind::CheckingUnresolvedFinalization),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn nullable_child_completion_follows_checked_presence_queries() {
        for (query, unresolved) in [("is_absent", false), ("is_present", true)] {
            let source = format!(
                "module app; struct Resource {{ finalize() -> Result<unit, unit> {{ return Error(unit); }} }} func root(pos value: Resource?) requires(value.{query}()) {{}}"
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(
                diagnostics.has_errors(),
                unresolved,
                "{query}: {diagnostics:?}"
            );

            assert_eq!(
                diagnostics.iter().any(|diagnostic| diagnostic.kind()
                    == DiagnosticKind::CheckingUnresolvedFinalization),
                unresolved,
                "{query}: {diagnostics:?}"
            );

            let lowered = compilation
                .lowered_unit(source_function_body_key(&compilation, "root"))
                .unwrap();

            assert_eq!(
                lowered.value().is_none(),
                unresolved,
                "{query}: {:?}",
                lowered.diagnostics()
            );
        }
    }

    #[test]
    fn completed_siblings_preserve_proofs_only_across_verified_destruction() {
        for (destructor, accepted) in [
            ("", true),
            ("destruct() executes(pure, total) {}", true),
            (
                "destruct() when(!self.pending) { executes(pure, total) } {}",
                true,
            ),
            ("destruct() {}", false),
        ] {
            for (declarations, parameters, body) in [
                (
                    "",
                    "pos mut first: Resource, pos mut second: Resource",
                    "first.pending = false; second.pending = false;",
                ),
                (
                    "struct Pair { mut first: Resource; mut second: Resource; }",
                    "pos mut value: Pair",
                    "value.first.pending = false; value.second.pending = false;",
                ),
                (
                    "",
                    "pos mut value: (Resource, Resource)",
                    "value.0.pending = false; value.1.pending = false;",
                ),
            ] {
                let source = format!(
                    "module app; struct Resource {{ mut pending: bool; finalize() -> Result<unit, unit> when(!self.pending) {{ executes(pure, total) ensures(result matches Ok(_)) }} {{ if !self.pending {{ return Ok(unit); }} return Error(unit); }} {destructor} }} {declarations} func root({parameters}) {{ {body} }}"
                );

                let compilation = compilation(&source);
                let diagnostics = compilation.check_diagnostics();

                assert_eq!(
                    diagnostics.has_errors(),
                    !accepted,
                    "{source}: {diagnostics:?}"
                );

                let lowered = compilation
                    .lowered_unit(source_function_body_key(&compilation, "root"))
                    .unwrap();

                assert_eq!(
                    lowered.value().is_some(),
                    accepted,
                    "{source}: {:?}",
                    lowered.diagnostics()
                );

                if accepted {
                    let mir = lowered.value().as_ref().unwrap().mir().unwrap();

                    assert!(
                        mir.operations()
                            .iter()
                            .filter(|operation| matches!(
                                operation.kind(),
                                bray_ir::MirOperationKind::Destroy(_)
                            ))
                            .count()
                            >= 2,
                        "{source}: {mir:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn function_cleanup_uses_dependency_checked_execution_guarantees() {
        for (destructor, accepted) in [
            ("destruct() executes(pure, total) {}", true),
            ("destruct() {}", false),
            (
                "destruct() executes(pure, total) { panic(\"invalid promise\"); }",
                false,
            ),
        ] {
            let source = format!(
                concat!(
                    "module app;\n",
                    "struct Resource {{ async finalize() executes(pure, total) {{}} {destructor} }}\n",
                    "func root(pos value: Resource) executes(pure, total) {{}}\n",
                ),
                destructor = destructor
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                accepted,
                "{source}: {diagnostics:?}"
            );

            assert_eq!(
                compilation
                    .lowered_unit(source_function_body_key(&compilation, "root"))
                    .unwrap()
                    .value()
                    .is_some(),
                accepted
            );
        }
    }

    #[test]
    fn unverified_destructor_purity_cannot_preserve_later_completion_proofs() {
        let compilation = compilation(
            "module app; struct Resource { mut pending: bool; finalize() -> Result<unit, unit> when(!self.pending) { executes(pure, total) ensures(result matches Ok(_)) } { if !self.pending { return Ok(unit); } return Error(unit); } destruct() executes(pure) { panic(\"stop\"); } } func root(pos mut first: Resource, pos mut second: Resource) { first.pending = false; second.pending = false; }",
        );

        let key = source_function_body_key(&compilation, "root");

        let proofs = compilation
            .callable_proofs_with_cancellation(key.clone(), &compilation.state.cancellation)
            .unwrap();

        assert!(
            proofs
                .result()
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.kind()
                    == DiagnosticKind::CheckingUnprovenFinalizationCompletion),
            "{:?}",
            proofs.result()
        );

        assert!(compilation.lowered_unit(key).unwrap().value().is_none());
    }

    #[test]
    fn synchronous_ownership_end_requires_async_finalizer_completion() {
        for ty in ["Resource", "(Resource, bool)", "Wrapper", "Resource?"] {
            for body in ["", "panic(\"stop\");"] {
                let source = format!(
                    "module app; struct Resource {{ async finalize() {{}} }} struct Wrapper {{ inner: Resource; }} func root(pos value: {ty}) {{ {body} }}"
                );

                let compilation = compilation(&source);
                let diagnostics = compilation.check_diagnostics();

                assert!(
                    diagnostics.iter().any(|diagnostic| diagnostic.kind()
                        == DiagnosticKind::CheckingAsyncFinalizationInSynchronousContext),
                    "{source}: {diagnostics:?}"
                );

                assert!(
                    compilation
                        .lowered_unit(source_function_body_key(&compilation, "root"))
                        .unwrap()
                        .value()
                        .is_none(),
                    "{source}"
                );
            }
        }
    }

    #[test]
    fn completed_async_finalizers_need_no_frame_on_ordinary_or_panic_exit() {
        for (declarations, ty) in [
            ("", "Resource"),
            ("struct Wrapper { inner: Resource; }", "Wrapper"),
        ] {
            for body in ["", "panic(\"stop\");"] {
                let source = format!(
                    "module app; struct Resource {{ async finalize() executes(pure, total) {{}} destruct() executes(pure, total) {{}} }} {declarations} func root(pos value: {ty}) {{ {body} }}"
                );

                let compilation = compilation(&source);
                let diagnostics = compilation.check_diagnostics();

                assert!(!diagnostics.has_errors(), "{source}: {diagnostics:?}");

                let lowered = compilation
                    .lowered_unit(source_function_body_key(&compilation, "root"))
                    .unwrap();

                assert!(
                    !lowered.diagnostics().has_errors(),
                    "{source}: {:?}",
                    lowered.diagnostics()
                );

                let mir = lowered.value().as_ref().unwrap().mir().unwrap();

                assert!(
                    mir.operations().iter().any(|operation| matches!(
                        operation.kind(),
                        bray_ir::MirOperationKind::Destroy(_)
                    )),
                    "{source}: {mir:?}"
                );

                assert!(
                    !mir.operations().iter().any(|operation| matches!(
                        operation.kind(),
                        bray_ir::MirOperationKind::Async(
                            bray_ir::MirAsyncOperation::CreateFrame { .. }
                        )
                    )),
                    "{source}: {mir:?}"
                );
            }
        }
    }

    #[test]
    fn synchronous_transfer_and_async_ownership_end_preserve_async_obligations() {
        for root in [
            "func root(pos value: Resource) -> Resource { return value; }",
            "async func root(pos value: Resource) {}",
        ] {
            let source = format!("module app; struct Resource {{ async finalize() {{}} }} {root}");
            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert!(!diagnostics.has_errors(), "{source}: {diagnostics:?}");
        }
    }

    #[test]
    fn uniform_array_completion_has_extent_independent_proof_and_mir_size() {
        let mut size = None;

        for length in [2, 100_000] {
            let source = format!(
                "module app; struct Resource {{ async finalize() executes(pure, total) {{}} destruct() {{}} }} func root(pos value: [Resource; {length}]) {{}}"
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert!(!diagnostics.has_errors(), "{source}: {diagnostics:?}");

            let key = source_function_body_key(&compilation, "root");

            let proofs = compilation
                .callable_proofs_with_cancellation(key.clone(), &compilation.state.cancellation)
                .unwrap();

            assert!(
                !proofs.result().diagnostics().has_errors(),
                "{:?}",
                proofs.result()
            );

            let lowered = compilation.lowered_unit(key).unwrap();

            assert!(
                !lowered.diagnostics().has_errors(),
                "{:?}",
                lowered.diagnostics()
            );

            let mir = lowered.value().as_ref().unwrap().mir().unwrap();

            assert!(
                mir.operations().iter().any(|operation| matches!(
                    operation.kind(),
                    bray_ir::MirOperationKind::Destroy(_)
                )),
                "{mir:?}"
            );

            assert!(
                !mir.operations().iter().any(|operation| matches!(
                    operation.kind(),
                    bray_ir::MirOperationKind::Async(
                        bray_ir::MirAsyncOperation::CreateFrame { .. }
                    )
                )),
                "{mir:?}"
            );

            let current = (proofs.result().value().len(), mir.operations().len());

            if let Some(expected) = size {
                assert_eq!(current, expected);
            }

            size = Some(current);
        }
    }

    #[test]
    fn absent_cleanup_parts_receive_vacuous_completion_certificates() {
        for (ty, contract) in [
            ("[Resource; 0]", ""),
            ("Resource?", "requires(value.is_absent())"),
        ] {
            let source = format!(
                "module app; struct Resource {{ async finalize() -> Result<unit, unit> {{ return Error(unit); }} }} func root(pos value: {ty}) {contract} {{}}"
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert!(!diagnostics.has_errors(), "{source}: {diagnostics:?}");

            let key = source_function_body_key(&compilation, "root");

            let proofs = compilation
                .callable_proofs_with_cancellation(key.clone(), &compilation.state.cancellation)
                .unwrap();

            let has_obligation = proofs.result().value().iter().any(|proof| {
                matches!(
                    proof,
                    bray_bound_tree::CallableProofObligation::Finalization { part: Some(_), .. }
                )
            });

            assert_eq!(
                has_obligation,
                ty != "[Resource; 0]",
                "{source}: {:?}",
                proofs.result()
            );

            assert!(compilation.lowered_unit(key).unwrap().value().is_some());
        }
    }

    #[test]
    fn absence_promises_require_implementation_evidence_before_cleanup_is_omitted() {
        for borrow in ["&", "&mut "] {
            let compilation = compilation(&format!(
                "module app; struct Resource {{ finalize() -> Result<unit, unit> {{ return Error(unit); }} }} func clear(pos value: {borrow}Resource?) executes(pure, total) ensures(value.is_absent()) {{}} func root(pos mut value: Resource?) {{ clear({borrow}value); }}"
            ));

            let key = source_function_body_key(&compilation, "root");

            let proofs = compilation
                .callable_proofs_with_cancellation(key.clone(), &compilation.state.cancellation)
                .unwrap();

            assert!(
                proofs
                    .result()
                    .diagnostics()
                    .iter()
                    .any(|diagnostic| diagnostic.kind()
                        == DiagnosticKind::CheckingUnprovenFinalizationCompletion),
                "{:?}",
                proofs.result()
            );

            assert!(compilation.lowered_unit(key).unwrap().value().is_none());
        }
    }

    #[test]
    fn lowering_requires_validated_transitive_contract_evidence() {
        let compilation = compilation(
            "module app; func leaf() -> bool executes(pure, total) ensures(result) { return false; } func root() -> bool when(true) { ensures(result) } { return leaf(); }",
        );

        let key = source_function_body_key(&compilation, "root");
        let lowered = compilation.lowered_unit(key).unwrap();

        assert!(lowered.value().is_none());

        assert!(
            lowered.diagnostics().iter().any(
                |diagnostic| diagnostic.kind() == DiagnosticKind::CheckingUnprovenPostcondition
            )
        );
    }
}
