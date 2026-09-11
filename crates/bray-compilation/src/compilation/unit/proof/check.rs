use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_bound_tree::{
    BoundUnitId, BoundUnitKey, CallableProofKey, CallableProofObligation, CallableProofResult,
};
use bray_checker::{CallableProofFailure, check_callable_proof_dependencies};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticResult,
    SeverityKind,
};
use bray_source::SourceSpan;
use bray_symbols::{CallableDefinitionId, CallableInstanceData, CallableSymbolId};

use crate::compilation::Compilation;
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError, PublishedUnitResult};

use super::input::{ProofDependency, ProofInputs};
use super::selection::{EvidenceSelection, ProofContext};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum ProofOwner {
    Source(BoundUnitId),
    Declaration(CallableSymbolId),
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ProofInstance {
    owner: ProofOwner,
    context: ProofContext,
}

type ProofKey = CallableProofKey<ProofInstance>;

#[derive(Default)]
struct ProofGraph {
    inputs: BTreeMap<ProofOwner, ProofInputs>,
    origins: BTreeMap<ProofOwner, BoundUnitKey>,
    graph: BTreeMap<ProofKey, BTreeSet<ProofKey>>,
    unavailable: BTreeMap<ProofKey, CallableProofFailure<ProofInstance>>,
    diagnostics: DiagnosticBag,
}

impl Compilation {
    pub(in crate::compilation) fn instance_callable_proofs(
        &self,
        instance: CallableInstanceData,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Vec<CallableProofObligation>>, FactQueryError> {
        let mut graph = ProofGraph::default();
        let owner = graph.callable_owner(self, instance.definition(), cancellation)?;

        let root = ProofInstance {
            owner,
            context: ProofContext {
                substitution: Some(instance.substitution()),
                contextual_self: None,
                allow_open_requirements: false,
            },
        };

        graph.inputs(self, owner, cancellation)?;

        self.certify_callable_proofs(root, &mut graph, cancellation)
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
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;
                let owner = ProofOwner::Source(bound.result().value().unit());
                let mut graph = ProofGraph::default();

                graph.origins.insert(owner, key.clone());
                graph.inputs(self, owner, cancellation)?;

                let context = self.source_proof_context(&key, cancellation)?;

                let result = self.certify_callable_proofs(
                    ProofInstance { owner, context },
                    &mut graph,
                    cancellation,
                )?;

                Ok((result, Box::new([])))
            },
        )
    }

    fn certify_callable_proofs(
        &self,
        root: ProofInstance,
        graph: &mut ProofGraph,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Vec<CallableProofObligation>>, FactQueryError> {
        let roots = graph
            .inputs(self, root.owner, cancellation)?
            .keys()
            .map(|obligation| ProofKey::new(root, *obligation))
            .collect::<Vec<_>>();

        let mut pending = roots
            .iter()
            .map(|key| (*key, Vec::new()))
            .collect::<Vec<_>>();

        let mut visited = BTreeSet::new();

        while let Some((key, mut path)) = pending.pop() {
            cancellation.check()?;

            if !visited.insert(key) {
                continue;
            }

            let Some(dependencies) = graph
                .inputs(self, key.owner().owner, cancellation)?
                .get(&key.obligation())
            else {
                continue;
            };

            // The work list owns the small semantic identities while newly discovered declarations
            // extend the input cache. Source bodies and their larger analyses remain shared.
            let dependencies = dependencies.clone();
            graph.graph.entry(key).or_default();
            path.push(key);

            for dependency in dependencies {
                let ProofDependency::Selected(target, obligation) = dependency else {
                    let ProofDependency::Unverified(site) = dependency else {
                        continue;
                    };

                    graph
                        .unavailable
                        .insert(key, CallableProofFailure::UnverifiedCall(site));

                    continue;
                };

                let selection = self.select_callable_evidence(
                    target,
                    key.owner().context,
                    cancellation,
                    &mut graph.diagnostics,
                )?;

                let (instance, context, obligations) = match selection {
                    EvidenceSelection::Direct(instance, context) => {
                        (instance, context, vec![obligation])
                    }
                    EvidenceSelection::Fulfillment(instance, context) => {
                        let obligations =
                            self.fulfillment_proof_obligations(instance, obligation, cancellation)?;

                        (instance, context, obligations)
                    }
                    EvidenceSelection::Open => continue,
                    EvidenceSelection::Unavailable => {
                        // Keep a missing exact promise in the graph so every dependent proof fails.
                        let callable = self
                            .semantic_value_store()?
                            .callable_instance_data(target.callable())?;

                        let missing = ProofInstance {
                            owner: ProofOwner::Declaration(callable.definition().callable_symbol()),
                            context: key.owner().context,
                        };

                        graph.unavailable.insert(
                            key,
                            CallableProofFailure::MissingCandidate(ProofKey::new(
                                missing, obligation,
                            )),
                        );

                        continue;
                    }
                };

                let owner = graph.callable_owner(self, instance.definition(), cancellation)?;
                let target = ProofInstance { owner, context };

                if obligations.is_empty() {
                    graph.unavailable.insert(
                        key,
                        CallableProofFailure::MissingCandidate(ProofKey::new(target, obligation)),
                    );

                    continue;
                }

                for obligation in obligations {
                    let dependency = ProofKey::new(target, obligation);
                    graph.graph.entry(key).or_default().insert(dependency);

                    // Repeated identical instances form ordinary graph cycles. Expanding generic
                    // recursion cannot supply a finite proof by generating endlessly new identities.
                    if path.iter().any(|ancestor| {
                        ancestor.owner().owner == owner
                            && ancestor.obligation() == obligation
                            && ancestor.owner().context != context
                    }) {
                        graph.unavailable.insert(
                            key,
                            CallableProofFailure::ExpandingInstantiation(dependency),
                        );

                        continue;
                    }

                    // Each pending branch retains its own short chain of copyable proof identities.
                    pending.push((dependency, path.clone()));
                }
            }
        }

        let failures = check_callable_proof_dependencies(&graph.graph, &graph.unavailable);
        let mut proven = Vec::new();

        for proof in roots {
            if let Some(failure) = failures.get(&proof) {
                self.publish_callable_proof_failure(
                    graph,
                    root.owner,
                    proof.obligation(),
                    *failure,
                    cancellation,
                )?;
            } else {
                proven.push(proof.obligation());
            }
        }

        Ok(DiagnosticResult::new(
            proven,
            std::mem::take(&mut graph.diagnostics),
        ))
    }

    fn publish_callable_proof_failure(
        &self,
        graph: &mut ProofGraph,
        root: ProofOwner,
        obligation: CallableProofObligation,
        failure: CallableProofFailure<ProofInstance>,
        cancellation: &CancellationToken,
    ) -> Result<(), FactQueryError> {
        if let CallableProofFailure::MissingCandidate(missing) = failure
            && let Some(key) = graph.origins.get(&missing.owner().owner)
        {
            let semantics = self.body_semantics_with_cancellation(key.clone(), cancellation)?;

            if let Some(CallableProofResult::Unproven { diagnostic, .. }) = semantics
                .result()
                .value()
                .execution_proofs()
                .iter()
                .find(|checked| checked.obligation() == missing.obligation())
            {
                // Failure publication retains the precise diagnostic independently of its cached body.
                graph.diagnostics.add(diagnostic.clone());
            }
        }

        let Some(root) = graph.origins.get(&root) else {
            return Ok(());
        };

        let anchor = root.source().syntax();
        let id = DiagnosticId::new(anchor.full_range().start().bytes());

        let diagnostic = match obligation {
            CallableProofObligation::Execution(guarantee) => Diagnostic::new(
                id,
                DiagnosticKind::CheckingUnprovenExecutionGuarantee,
                SeverityKind::Error,
            )
            .with_arg(DiagnosticArg::referenced_name(
                guarantee.property().as_str(),
            )),
            CallableProofObligation::Postcondition(_) => Diagnostic::new(
                id,
                DiagnosticKind::CheckingUnprovenPostcondition,
                SeverityKind::Error,
            ),
            CallableProofObligation::Finalization { .. }
            | CallableProofObligation::TypeFinalization { .. } => Diagnostic::new(
                id,
                DiagnosticKind::CheckingUnprovenFinalizationCompletion,
                SeverityKind::Error,
            ),
            CallableProofObligation::SynchronousDestruction { .. } => Diagnostic::new(
                id,
                DiagnosticKind::CheckingUnprovenExecutionGuarantee,
                SeverityKind::Error,
            )
            .with_arg(DiagnosticArg::referenced_name(
                bray_symbols::ExecutionProperty::Pure.as_str(),
            )),
        };

        graph.diagnostics.add(
            diagnostic.with_primary_span(SourceSpan::new(anchor.source_id(), anchor.full_range())),
        );

        Ok(())
    }
}

impl ProofGraph {
    fn callable_owner(
        &mut self,
        compilation: &Compilation,
        definition: CallableDefinitionId,
        cancellation: &CancellationToken,
    ) -> Result<ProofOwner, FactQueryError> {
        let Some(key) = compilation.callable_body_key(definition)? else {
            return Ok(ProofOwner::Declaration(definition.callable_symbol()));
        };

        let bound = compilation.bound_unit_with_cancellation(key.clone(), cancellation)?;
        let owner = ProofOwner::Source(bound.result().value().unit());

        self.origins.insert(owner, key);

        Ok(owner)
    }

    fn inputs(
        &mut self,
        compilation: &Compilation,
        owner: ProofOwner,
        cancellation: &CancellationToken,
    ) -> Result<&ProofInputs, FactQueryError> {
        match self.inputs.entry(owner) {
            std::collections::btree_map::Entry::Occupied(entry) => Ok(entry.into_mut()),
            std::collections::btree_map::Entry::Vacant(entry) => {
                let input = match owner {
                    ProofOwner::Source(_) => {
                        let Some(key) = self.origins.get(&owner) else {
                            return Ok(entry.insert(BTreeMap::new()));
                        };

                        compilation.source_proof_inputs(key, cancellation)?
                    }
                    ProofOwner::Declaration(callable) => {
                        compilation.declared_proof_inputs(callable, cancellation)?
                    }
                };

                self.diagnostics = self.diagnostics.merged(input.diagnostics());

                Ok(entry.insert(input.into_parts().0))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::{compilation, source_function_body_key};
    use bray_diagnostics::DiagnosticKind;

    #[test]
    fn returned_owner_construction_keeps_admission_effects_despite_completion() {
        for guarantee in ["pure", "total"] {
            for (kind, members, constructed, completed) in [
                (
                    "struct",
                    "pending: bool;",
                    "Resource { pending = false }",
                    "!self.pending",
                ),
                (
                    "union",
                    "Ready(pos value: bool); Pending;",
                    "Ready(false)",
                    "self matches Ready(_)",
                ),
                ("union", "Ready; Pending;", "Ready", "self matches Ready"),
            ] {
                for (finalizer, accepted) in [
                    (
                        "finalize() -> Result<unit, unit> when(!self.pending) { executes(pure, total) ensures(result matches Ok(_)) } { if !self.pending { return Ok(unit); } return Error(unit); }",
                        false,
                    ),
                    ("finalize() executes(pure, total) {}", true),
                ] {
                    let finalizer = finalizer.replace("!self.pending", completed);

                    let source = format!(
                        r#"
                    module app;
                    {kind} Resource
                    {{
                        {members}
                        {finalizer}
                        destruct() executes(pure, total) {{}}
                    }}
                    func make() -> Resource executes({guarantee})
                    {{
                        return {constructed};
                    }}
                    "#
                    );

                    let compilation = compilation(&source);
                    let diagnostics = compilation.check_diagnostics();

                    assert_eq!(
                        !diagnostics.has_errors(),
                        accepted,
                        "{source}: {diagnostics:?}"
                    );

                    if !accepted {
                        assert!(
                            diagnostics.iter().any(|diagnostic| diagnostic.kind()
                                == DiagnosticKind::CheckingUnprovenExecutionGuarantee),
                            "{source}: {diagnostics:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn complete_catch_owner_uses_unconditional_finalizer_evidence() {
        let source = r#"
            module app;
            struct Guard { destruct() executes(pure, total) {} }
            struct Owner
            {
                guard: Guard;
                finalize() -> Result<unit, unit>
                    executes(pure, total) ensures(result matches Ok(_))
                { return Ok(unit); }
            }
            func make() -> Result<Owner, PanicReport>
            {
                return catch
                {
                    let guard = Guard {};
                    let owner = Owner { guard = guard };
                    yield owner;
                };
            }
            "#;

        for (returned, accepted) in [("return Ok(unit);", true), ("return Error(unit);", false)] {
            let compilation = compilation(&source.replace("return Ok(unit);", returned));
            let diagnostics = compilation.check_diagnostics();
            assert_eq!(!diagnostics.has_errors(), accepted, "{diagnostics:?}");

            if !accepted {
                assert!(
                    diagnostics.iter().any(|diagnostic| diagnostic.kind()
                        == DiagnosticKind::CheckingUnprovenPostcondition),
                    "{diagnostics:?}"
                );
            }
        }
    }

    #[test]
    fn task_admission_rejection_retains_the_inactive_frame_for_caller_cleanup() {
        let compilation = compilation(
            r#"
            module app;

            async func leaf() {}

            async func main()
            {
                let task = leaf().start();
            }
            "#,
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
                r#"
                module app;
                struct Owner
                {{
                    leaf: Leaf;
                    destruct() {owner_contract}
                    {{
                    }}
                }}
                struct Leaf
                {{
                    async finalize() {leaf_contract}
                    {{
                        {body}
                    }}
                    destruct() executes(pure, total)
                    {{
                    }}
                }}
                func main()
                {{
                    let owner = Owner
                    {{
                        leaf = Leaf
                        {{
                        }}
                    }};
                }}
                "#,
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
            r#"
            module app;

            func leaf() -> bool
                executes(pure, total)
            {
                return 0;
            }

            func root() -> bool
                executes(pure, total)
            {
                return leaf();
            }
            "#,
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
            r#"
            module app;

            func leaf() -> bool
                ensures(result)
            {
                return false;
            }

            func root() -> bool
                when(true)
                {
                    ensures(result)
                }
            {
                return leaf();
            }
            "#,
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
            r#"
            module app;

            struct Resource
            {
                finalize()
                    executes(pure, total) {}

                destruct() {}
            }

            func root(pos value: Resource) {}
            "#,
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
            r#"
            module app;

            struct Resource
            {
                mut pending: bool;

                finalize()
                    executes(pure, total)
                {
                    self.pending = false;
                }
            }

            func root(pos value: Resource) {}
            "#,
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
            r#"
            module app;

            struct Resource
            {
                mut pending: bool;

                mut func close()
                    ensures(!self.pending)
                {
                    self.pending = false;
                }

                finalize() -> Result<unit, unit>
                    when(!self.pending)
                    {
                        executes(pure, total)
                        ensures(result matches Ok(_))
                    }
                {
                    if !self.pending
                    {
                        return Ok(unit);
                    }

                    return Error(unit);
                }
            }

            func root(pos mut value: Resource)
            {
                value.close();
            }
            "#,
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
                r#"
                module app;
                struct Resource
                {{
                    mut pending: bool;
                    mut func close() ensures(!self.pending)
                    {{
                        self.pending = false;
                    }}
                    finalize() -> Result<unit, unit> when(!self.pending)
                    {{
                        executes(pure, total) ensures(result matches Ok(_))
                    }}
                    {{
                        if !self.pending
                        {{
                            return Ok(unit);
                        }}
                        return Error(unit);
                    }}
                }}
                func root(pos mut value: Resource) {result}
                {{
                    {body}
                }}
                "#
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
                r#"
                module app;
                struct Resource
                {{
                    finalize() -> Result<unit, unit>
                    {{
                        return Error(unit);
                    }}
                }}
                func root(pos value: Resource) {result}
                {{
                    {body}
                }}
                "#
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
                r#"
                module app;
                struct Resource
                {{
                    mut pending: bool;
                    finalize() -> Result<unit, unit> when(!self.pending)
                    {{
                        executes(pure, total) ensures(result matches Ok(_))
                    }}
                    {{
                        if !self.pending
                        {{
                            return Ok(unit);
                        }}
                        return Error(unit);
                    }}
                }}
                func root(pos mut value: Resource)
                {{
                    {body}
                }}
                "#
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
                r#"
                module app;
                struct Resource
                {{
                    finalize() -> Result<unit, unit>
                    {{
                        return Error(unit);
                    }}
                }}
                struct Wrapper<T>
                {{
                    inner: T;
                }}
                union Payload
                {{
                    Present(pos value: Resource);
                    Absent;
                }}
                func root(pos value: {ty})
                {{
                }}
                "#
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
                r#"
                module app;
                struct Resource
                {{
                    finalize() -> Result<unit, unit>
                    {{
                        return Error(unit);
                    }}
                }}
                struct Wrapper
                {{
                    inner: Resource;
                }}
                func root(pos value: {ty}) {result}
                {{
                    {body}
                }}
                "#
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
            r#"
            module app;

            struct Resource
            {
                finalize() -> Result<unit, unit>
                {
                    return Error(unit);
                }
            }

            func root(pos value: Resource)
            {
                panic("stop");
            }
            "#,
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
                r#"
                module app;
                struct Resource
                {{
                    mut pending: bool;
                    finalize() -> Result<unit, unit> when(!self.pending)
                    {{
                        executes(pure, total) ensures(result matches Ok(_))
                    }}
                    {{
                        if !self.pending
                        {{
                            return Ok(unit);
                        }}
                        return Error(unit);
                    }}
                }}
                func root()
                {{
                    let value = Resource
                    {{
                        pending = {pending}
                    }};
                }}
                "#
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
                r#"
                struct Wrapper
                {
                    mut inner: Inner;
                }

                struct Inner
                {
                    mut resource: Resource;
                }
                "#,
                "Wrapper",
                "value.inner.resource.pending = false;",
            ),
            ("", "(Resource, bool)", "value.0.pending = false;"),
        ] {
            let source = format!(
                r#"
                module app;
                struct Resource
                {{
                    mut pending: bool;
                    finalize() -> Result<unit, unit> when(!self.pending)
                    {{
                        executes(pure, total) ensures(result matches Ok(_))
                    }}
                    {{
                        if !self.pending
                        {{
                            return Ok(unit);
                        }}
                        return Error(unit);
                    }}
                    destruct()
                    {{
                    }}
                }}
                {declarations} func root(pos mut value: {ty})
                {{
                    {body}
                }}
                "#
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
                r#"
                module app;
                struct Resource
                {{
                    mut pending: bool;
                    finalize() -> Result<unit, unit> when(!self.pending)
                    {{
                        executes(pure, total) ensures(result matches Ok(_))
                    }}
                    {{
                        if !self.pending
                        {{
                            return Ok(unit);
                        }}
                        return Error(unit);
                    }}
                }}
                func root(pos value: box Resource) requires( value matches box(Resource
                {{
                    pending = {pending}
                }} ) )
                {{
                }}
                "#
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
                r#"
                module app;
                struct Resource
                {{
                    mut pending: bool;
                    finalize() -> Result<unit, unit> when(!self.pending)
                    {{
                        executes(pure, total) ensures(result matches Ok(_))
                    }}
                    {{
                        if !self.pending
                        {{
                            return Ok(unit);
                        }}
                        return Error(unit);
                    }}
                }}
                func root(pos mut value: box Resource) requires(value matches box(Resource
                {{
                    pending = false
                }}))
                {{
                    match value
                    {{
                        case box(resource)
                        {{
                            {body}
                        }}
                    }};
                }}
                "#
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
                    r#"
                    module app;
                    struct Resource
                    {{
                        mut pending: bool;
                        finalize() -> Result<unit, unit> when(!self.pending)
                        {{
                            executes(pure, total) ensures(result matches Ok(_))
                        }}
                        {{
                            if !self.pending
                            {{
                                return Ok(unit);
                            }}
                            return Error(unit);
                        }}
                    }}
                    {declarations} func root(pos mut value: {ty})
                    {{
                        match value
                        {{
                            case {pattern}
                            {{
                                resource.pending = {pending};
                            }}
                        }};
                    }}
                    "#
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
            r#"
            module app;

            struct Resource
            {
                mut pending: bool;

                finalize() -> Result<unit, unit>
                    when(!self.pending)
                    {
                        executes(pure, total)
                        ensures(result matches Ok(_))
                    }
                {
                    if !self.pending
                    {
                        return Ok(unit);
                    }

                    return Error(unit);
                }
            }

            func root(pos mut value: [Resource; 2]
            )
                {
                    match value
                    {
                        case [resource, _] { resource.pending = false; }
                    };
                }
            "#,
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
                r#"
                module app;
                struct Resource
                {{
                    finalize() -> Result<unit, unit>
                    {{
                        return Error(unit);
                    }}
                }}
                func root(pos value: Resource?) requires(value.{query}())
                {{
                }}
                "#
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
    fn completed_siblings_preserve_proofs_across_disjoint_destruction() {
        for (destructor, accepted) in [
            ("", true),
            ("destruct() executes(pure, total) {}", true),
            (
                "destruct() when(!self.pending) { executes(pure, total) } {}",
                true,
            ),
            ("destruct() {}", true),
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
                    r#"
                    module app;
                    struct Resource
                    {{
                        mut pending: bool;
                        finalize() -> Result<unit, unit> when(!self.pending)
                        {{
                            executes(pure, total) ensures(result matches Ok(_))
                        }}
                        {{
                            if !self.pending
                            {{
                                return Ok(unit);
                            }}
                            return Error(unit);
                        }}
                        {destructor}
                    }}
                    {declarations} func root({parameters})
                    {{
                        {body}
                    }}
                    "#
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
                r#"
                module app;
                struct Resource
                {{
                    async finalize() executes(pure, total)
                    {{
                    }}
                    {destructor}
                }}
                func root(pos value: Resource) executes(pure, total)
                {{
                }}
                "#,
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
            r#"
            module app;

            struct Resource
            {
                mut pending: bool;

                finalize() -> Result<unit, unit>
                    when(!self.pending)
                    {
                        executes(pure, total)
                        ensures(result matches Ok(_))
                    }
                {
                    if !self.pending
                    {
                        return Ok(unit);
                    }

                    return Error(unit);
                }

                destruct()
                    executes(pure)
                {
                    panic("stop");
                }
            }

            func root(pos mut first: Resource, pos mut second: Resource)
            {
                first.pending = false;
                second.pending = false;
            }
            "#,
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
                    r#"
                    module app;
                    struct Resource
                    {{
                        async finalize()
                        {{
                        }}
                    }}
                    struct Wrapper
                    {{
                        inner: Resource;
                    }}
                    func root(pos value: {ty})
                    {{
                        {body}
                    }}
                    "#
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
                    r#"
                    module app;
                    struct Resource
                    {{
                        async finalize() executes(pure, total)
                        {{
                        }}
                        destruct() executes(pure, total)
                        {{
                        }}
                    }}
                    {declarations} func root(pos value: {ty})
                    {{
                        {body}
                    }}
                    "#
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
            r#"
            func root(pos value: Resource) -> Resource
            {
                return value;
            }
            "#,
            "async func root(pos value: Resource) {}",
        ] {
            let source = format!(
                r#"
                module app;
                struct Resource
                {{
                    async finalize()
                    {{
                    }}
                }}
                {root}
                "#
            );

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
                r#"
                module app;
                struct Resource
                {{
                    async finalize() executes(pure, total)
                    {{
                    }}
                    destruct()
                    {{
                    }}
                }}
                func root(pos value: [Resource;
                {length}] )
                {{
                }}
                "#
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
                r#"
                module app;
                struct Resource
                {{
                    async finalize() -> Result<unit, unit>
                    {{
                        return Error(unit);
                    }}
                }}
                func root(pos value: {ty}) {contract}
                {{
                }}
                "#
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
                r#"
                module app;
                struct Resource
                {{
                    finalize() -> Result<unit, unit>
                    {{
                        return Error(unit);
                    }}
                }}
                func clear(pos value: {borrow}Resource?) executes(pure, total) ensures(value.is_absent())
                {{
                }}
                func root(pos mut value: Resource?)
                {{
                    clear({borrow}value);
                }}
                "#
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
            r#"
            module app;

            func leaf() -> bool
                executes(pure, total)
                ensures(result)
            {
                return false;
            }

            func root() -> bool
                when(true)
                {
                    ensures(result)
                }
            {
                return leaf();
            }
            "#,
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
