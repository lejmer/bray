use super::dependency::ExecutionProofEntry;
use super::imported::{ExecutionProofGraph, ExecutionProofOwner};
use bray_symbols::{ExecutionProofFailure, check_execution_proof_dependencies};
use std::collections::BTreeSet;

use bray_bound_tree::{BoundCallableTarget, BoundUnitKey};
use bray_checker::{ExecutionCertification, ExecutionObligation};
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticLabel,
    DiagnosticLabelKind, DiagnosticResult, SeverityKind,
};
use bray_source::SourceSpan;

use crate::compilation::Compilation;
use crate::fact::{CancellationToken, FactQueryError};

type ProofAssertions = BTreeSet<(SyntaxAnchor, bray_checker::ExecutionProperty)>;
type ProofDependencies = BTreeSet<(
    SyntaxAnchor,
    ExecutionObligation,
    bray_checker::ExecutionDependencyTarget,
    ExecutionObligation,
)>;
type CertifiedProof<Failure> = (Option<Failure>, ProofAssertions, ProofDependencies);

impl Compilation {
    pub(super) fn compute_certified_execution(
        &self,
        root: &BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<ExecutionCertification>, FactQueryError> {
        if root.is_static_initializer() {
            let diagnostics = self.check_static_initializer_eligibility(root, cancellation)?;

            return Ok(DiagnosticResult::new(
                ExecutionCertification::default(),
                diagnostics,
            ));
        }

        let declared = self.execution_unit_declaration(root)?;

        // The certificate owns source diagnostics independently of the declaration query.
        let mut diagnostics = declared.diagnostics().clone();
        let mut certified = ExecutionCertification::default();
        let constant = self.unit_requires_constant_evaluation(root, cancellation)?;

        if !constant
            && declared
                .value()
                .domains()
                .iter()
                .all(|domain| domain.properties.is_empty() && domain.postconditions.is_empty())
        {
            return Ok(DiagnosticResult::new(certified, diagnostics));
        }

        let candidates = self.execution_candidates_with_cancellation(root.clone(), cancellation)?;

        diagnostics.add_range(candidates.result().diagnostics().iter().cloned());

        if diagnostics.has_errors() {
            return Ok(DiagnosticResult::new(certified, diagnostics));
        }

        for domain in declared.value().domains() {
            let properties = domain.properties.iter().map(|property| {
                (
                    ExecutionObligation::Property(
                        property.property,
                        (!domain.guards.is_empty()).then_some(property.source.into()),
                    ),
                    property.source,
                )
            });

            let postconditions = domain.postconditions.iter().map(|anchor| {
                let source = source_span(*anchor);

                (ExecutionObligation::Postcondition(source.into()), source)
            });

            for (obligation, source) in properties.chain(postconditions) {
                let (failure, assertions, dependencies) = self.certify_execution_obligation(
                    root,
                    obligation,
                    &mut diagnostics,
                    false,
                    cancellation,
                )?;

                if let Some((cause, circular)) = failure {
                    diagnostics.add(guarantee_diagnostic(obligation, source, cause, circular));
                    continue;
                }

                match obligation {
                    ExecutionObligation::Property(property, None) => {
                        certified.properties.insert(property);
                    }
                    ExecutionObligation::Property(
                        property,
                        Some(bray_checker::ExecutionClauseId::Source(source)),
                    ) => {
                        certified.guarded_properties.insert((source, property));
                    }
                    ExecutionObligation::Postcondition(
                        bray_checker::ExecutionClauseId::Source(source),
                    ) => {
                        certified.postconditions.insert(source);
                    }
                    _ => continue,
                }

                certified.foreign_assertions.extend(assertions);
                certified.dependencies.extend(dependencies);
            }
        }

        if constant {
            let anchor = root.source().syntax();
            let source = source_span(anchor);

            for property in [
                bray_checker::ExecutionProperty::Pure,
                bray_checker::ExecutionProperty::Total,
            ] {
                if declared
                    .value()
                    .properties()
                    .iter()
                    .any(|declared| declared.property == property)
                {
                    continue;
                }

                let obligation = ExecutionObligation::Property(property, None);

                let (failure, assertions, dependencies) = self.certify_execution_obligation(
                    root,
                    obligation,
                    &mut diagnostics,
                    true,
                    cancellation,
                )?;

                if let Some((cause, circular)) = failure {
                    diagnostics.add(guarantee_diagnostic(obligation, source, cause, circular));
                } else {
                    certified.properties.insert(property);
                    certified.foreign_assertions.extend(assertions);
                    certified.dependencies.extend(dependencies);
                }
            }
        }

        Ok(DiagnosticResult::new(certified, diagnostics))
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "source proof selection retains declaration, required obligation, call evidence, and diagnostics"
    )]
    fn select_source_execution_obligation(
        &self,
        body: Option<&BoundUnitKey>,
        callable: Option<bray_symbols::CallableInstanceData>,
        declaration: &bray_checker::ExecutionDeclaration,
        required: ExecutionObligation,
        evidence: Option<&bray_checker::ExecutionCallEvidence>,
        diagnostics: &mut DiagnosticBag,
        cancellation: &CancellationToken,
    ) -> Result<Option<ExecutionObligation>, FactQueryError> {
        let Some(body) = body else {
            let callable =
                callable.expect("source declaration without a body retains its selected callable");

            return self.select_foreign_execution_obligation(
                callable,
                declaration,
                required,
                evidence,
                diagnostics,
                cancellation,
            );
        };

        let selected = match required {
            ExecutionObligation::Observation => self
                .applicable_execution_obligation(
                    body,
                    callable,
                    declaration,
                    ExecutionObligation::Property(bray_checker::ExecutionProperty::Pure, None),
                    evidence,
                    cancellation,
                )?
                .map(|source| {
                    ExecutionObligation::Property(
                        bray_checker::ExecutionProperty::Pure,
                        source.map(Into::into),
                    )
                })
                .or(Some(ExecutionObligation::Observation)),
            ExecutionObligation::Property(property, _) => {
                let selected = self.applicable_execution_obligation(
                    body,
                    callable,
                    declaration,
                    required,
                    evidence,
                    cancellation,
                )?;

                selected
                    .map(|source| ExecutionObligation::Property(property, source.map(Into::into)))
            }
            postcondition => Some(postcondition),
        };

        Ok(selected)
    }

    fn select_foreign_execution_obligation(
        &self,
        callable: bray_symbols::CallableInstanceData,
        declaration: &bray_checker::ExecutionDeclaration,
        required: ExecutionObligation,
        evidence: Option<&bray_checker::ExecutionCallEvidence>,
        diagnostics: &mut DiagnosticBag,
        cancellation: &CancellationToken,
    ) -> Result<Option<ExecutionObligation>, FactQueryError> {
        let bray_symbols::CallableSymbolId::Function(function) =
            callable.definition().callable_symbol()
        else {
            return Ok(None);
        };

        let foreign = self.foreign_callable_contract_with_cancellation(function, cancellation)?;

        diagnostics.add_range(foreign.diagnostics().iter().cloned());

        if foreign.value().is_none() || foreign.diagnostics().has_errors() {
            return Ok(None);
        }

        let Some(owner) = self.execution_contract_owner(function.into())? else {
            return Ok(None);
        };

        match required {
            ExecutionObligation::Property(property, _) => Ok(self
                .applicable_execution_obligation(
                    &owner,
                    Some(callable),
                    declaration,
                    required,
                    evidence,
                    cancellation,
                )?
                .map(|guard| ExecutionObligation::Property(property, guard.map(Into::into)))),
            ExecutionObligation::Postcondition(bray_checker::ExecutionClauseId::Source(source)) => {
                let Some(domain) = declaration.domains().iter().find(|domain| {
                    domain
                        .postconditions
                        .iter()
                        .any(|anchor| source_span(*anchor) == source)
                }) else {
                    return Ok(None);
                };

                let anchors = declaration
                    .requirements()
                    .iter()
                    .chain(&domain.guards)
                    .copied()
                    .collect::<Vec<_>>();

                let entry = self.execution_condition_inputs(&owner, &anchors, cancellation)?;

                diagnostics.add_range(entry.diagnostics().iter().cloned());

                if entry.diagnostics().has_errors() {
                    return Ok(None);
                }

                let conditions = entry
                    .into_parts()
                    .0
                    .into_iter()
                    .map(|(condition, _)| condition)
                    .collect::<Vec<_>>();

                let inputs = self.execution_unit_inputs(&owner, cancellation)?;

                Ok(evidence
                    .unwrap_or(&bray_checker::ExecutionCallEvidence::default())
                    .proves_trusted_with_inputs(&conditions, &inputs)
                    .then_some(required))
            }
            _ => Ok(None),
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "declared dependencies retain selected callable, obligation, proof entry, graph, and assertions"
    )]
    pub(super) fn declared_execution_dependency(
        &self,
        callable: bray_symbols::CallableInstanceData,
        required: ExecutionObligation,
        entry: ExecutionProofEntry<'_>,
        graph: &mut ExecutionProofGraph,
        assertions: &mut BTreeSet<(SyntaxAnchor, bray_checker::ExecutionProperty)>,
        diagnostics: &mut DiagnosticBag,
        cancellation: &CancellationToken,
    ) -> Result<
        Option<(
            ExecutionProofOwner,
            ExecutionObligation,
            Option<BoundUnitKey>,
        )>,
        FactQueryError,
    > {
        let Some(anchor) = self
            .symbol_graph()?
            .declaration_syntax_anchor(callable.definition().symbol())
        else {
            let selected = match entry {
                ExecutionProofEntry::Invocation(evidence) => self
                    .select_imported_execution_obligation(
                        callable,
                        required,
                        evidence,
                        diagnostics,
                        cancellation,
                    )?,
                ExecutionProofEntry::Dependency => Some(required),
            };

            let Some(selected) = selected else {
                return Ok(None);
            };

            return Ok(Some((
                ExecutionProofOwner::Imported(callable),
                selected,
                None,
            )));
        };

        let declaration = self.execution_declaration(anchor)?;

        diagnostics.add_range(declaration.diagnostics().iter().cloned());

        if declaration.diagnostics().has_errors() {
            return Ok(None);
        }

        let body = self.callable_body_key(callable.definition())?;

        let selected = match entry {
            ExecutionProofEntry::Invocation(evidence) => self.select_source_execution_obligation(
                body.as_ref(),
                Some(callable),
                declaration.value(),
                required,
                evidence,
                diagnostics,
                cancellation,
            )?,
            ExecutionProofEntry::Dependency => Some(required),
        };

        let Some(selected) = selected else {
            return Ok(None);
        };

        if body.is_none() {
            if let ExecutionObligation::Property(property, _) = selected {
                assertions.insert((anchor, property));
            }

            graph.insert(
                (ExecutionProofOwner::Source(anchor, None), selected),
                BTreeSet::new(),
            );
        }

        let owner = ExecutionProofOwner::Source(anchor, body.as_ref().map(|_| callable));

        Ok(Some((owner, selected, body)))
    }

    pub(in crate::compilation) fn certify_execution_obligation(
        &self,
        root: &BoundUnitKey,
        obligation: ExecutionObligation,
        diagnostics: &mut DiagnosticBag,
        predicate: bool,
        cancellation: &CancellationToken,
    ) -> Result<CertifiedProof<(SourceSpan, bool)>, FactQueryError> {
        let source = root.source().syntax();

        let (failure, assertions, dependencies) = self.certify_execution_proof(
            ExecutionProofOwner::Source(source, None),
            Some(root.clone()),
            obligation,
            diagnostics,
            predicate,
            cancellation,
        )?;

        Ok((
            failure.map(|(span, circular)| (span.unwrap_or_else(|| source_span(source)), circular)),
            assertions,
            dependencies,
        ))
    }

    // rust-style: allow(function-too-large, reason = "one bounded proof worklist shares visited owners, selected dependencies, and failure state across source and imported edges")
    pub(super) fn certify_execution_proof(
        &self,
        root_owner: ExecutionProofOwner,
        body: Option<BoundUnitKey>,
        obligation: ExecutionObligation,
        diagnostics: &mut DiagnosticBag,
        predicate: bool,
        cancellation: &CancellationToken,
    ) -> Result<CertifiedProof<(Option<SourceSpan>, bool)>, FactQueryError> {
        let source = match root_owner {
            ExecutionProofOwner::Source(source, _) => Some(source),
            _ => None,
        };

        let mut graph = ExecutionProofGraph::new();
        let mut assertions = BTreeSet::new();
        let mut selected_dependencies = BTreeSet::new();
        let mut visited = BTreeSet::new();

        let mut pending = vec![(
            root_owner,
            body,
            obligation,
            source.map(|source| (source, obligation)),
            predicate,
            1usize,
        )];

        let mut failure = None;
        let permit_premises = obligation.is_internal();

        let limit = usize::try_from(bray_checker::ConstantEvaluationLimits::default().call_depth())
            .expect("execution proof budget fits host");

        while let Some((owner, body, required, exported_owner, predicate, depth)) = pending.pop() {
            cancellation.check()?;

            if !visited.insert((owner, required, predicate)) {
                continue;
            }

            if depth > limit {
                failure = Some((proof_owner_span(owner, source), false));
                break;
            }

            let mut dependencies = BTreeSet::new();
            let mut valid = true;

            if let ExecutionProofOwner::Imported(callable) = owner {
                if self.intrinsic_projection_obligation(callable, required, cancellation)? {
                    graph.insert((owner, required), dependencies);
                    continue;
                }

                let contract =
                    self.imported_execution_contract(callable, diagnostics, cancellation)?;

                let Some(portable) = super::imported::portable_imported_obligation(required) else {
                    failure.get_or_insert((source.map(source_span), false));
                    continue;
                };

                let Some(proof) = contract
                    .evidence
                    .iter()
                    .find(|proof| proof.obligation == portable)
                else {
                    failure.get_or_insert((source.map(source_span), false));
                    continue;
                };

                if matches!(
                    proof.origin,
                    bray_symbols::CallableExecutionOrigin::Requirement
                        | bray_symbols::CallableExecutionOrigin::CompilerIntrinsic
                ) {
                    failure.get_or_insert((source.map(source_span), false));
                    continue;
                }

                for (target, obligation) in proof.dependencies.iter() {
                    let obligation = super::imported::imported_obligation(*obligation);

                    if let Some((target, selected, body)) = self.selected_execution_target(
                        *target,
                        obligation,
                        ExecutionProofEntry::Dependency,
                        permit_premises,
                        &mut graph,
                        &mut assertions,
                        diagnostics,
                        cancellation,
                    )? {
                        dependencies.insert((target, selected));

                        if body.is_some() || matches!(target, ExecutionProofOwner::Imported(_)) {
                            pending.push((
                                target,
                                body,
                                selected,
                                exported_owner,
                                false,
                                depth + 1,
                            ));
                        }
                    } else {
                        valid = false;
                    }
                }
            } else {
                let ExecutionProofOwner::Source(anchor, instance) = owner else {
                    unreachable!("leaf execution premises are established before queueing");
                };

                let key = body.expect("source proof retains its cached checked body");

                let predicate = !required.is_internal()
                    && (predicate || self.unit_requires_constant_evaluation(&key, cancellation)?);

                let candidates = self.execution_candidates_with_cancellation(key, cancellation)?;

                let goal = match required {
                    ExecutionObligation::Observation => {
                        ExecutionObligation::Property(bray_checker::ExecutionProperty::Pure, None)
                    }
                    ExecutionObligation::FiniteIteration => ExecutionObligation::Termination,
                    _ => required,
                };

                diagnostics.add_range(candidates.result().diagnostics().iter().cloned());

                if candidates.result().diagnostics().has_errors() {
                    failure.get_or_insert((Some(source_span(anchor)), false));
                    continue;
                }

                let candidate = candidates.result().value().get(&goal).unwrap_or_else(|| {
                    panic!("checked source execution candidates omit {goal:?} for {owner:?}")
                });

                let local_failure = if required == ExecutionObligation::Observation {
                    candidate.observation_failure()
                } else {
                    candidate
                        .failure()
                        .or_else(|| predicate.then(|| candidate.constant_failure()).flatten())
                };

                if local_failure.is_some()
                    || (required == ExecutionObligation::FiniteIteration
                        && !candidate.finite_iteration())
                {
                    failure.get_or_insert((
                        Some(local_failure.unwrap_or_else(|| source_span(anchor))),
                        false,
                    ));

                    continue;
                }

                for (original, requested, site) in candidate_dependencies(candidate) {
                    let requested = if required == ExecutionObligation::Observation
                        && requested
                            == ExecutionObligation::Property(
                                bray_checker::ExecutionProperty::Pure,
                                None,
                            ) {
                        ExecutionObligation::Observation
                    } else if matches!(
                        required,
                        ExecutionObligation::Termination | ExecutionObligation::FiniteIteration
                    ) && requested
                        == ExecutionObligation::Property(
                            bray_checker::ExecutionProperty::Total,
                            None,
                        )
                    {
                        ExecutionObligation::Termination
                    } else {
                        requested
                    };

                    if let bray_checker::ExecutionDependencyTarget::Default(provider) = original {
                        if let Some(body) = self.declared_unit_key(
                            provider.symbol(),
                            bray_bound_tree::BoundUnitKind::RuntimeDefault,
                        )? {
                            let target =
                                ExecutionProofOwner::Source(body.source().syntax(), instance);

                            dependencies.insert((target, requested));

                            pending.push((
                                target,
                                Some(body),
                                requested,
                                exported_owner,
                                predicate,
                                depth + 1,
                            ));
                        } else {
                            valid &= self.imported_default_proves_execution(
                                provider,
                                requested,
                                predicate,
                                diagnostics,
                                cancellation,
                            )?;
                        }

                        continue;
                    }

                    let original_callable = original
                        .callable()
                        .expect("non-default proof dependency retains its selected callable");

                    if matches!(original_callable, BoundCallableTarget::Predicate(_)) {
                        continue;
                    }

                    let evidence = candidate.dependency_evidence(site, original_callable);

                    if matches!(
                        original_callable,
                        BoundCallableTarget::Anonymous(_) | BoundCallableTarget::Indirect(_)
                    ) && let Some(body) =
                        evidence.and_then(bray_checker::ExecutionCallEvidence::anonymous_target)
                    {
                        let declared = self.execution_unit_declaration(body)?;

                        diagnostics.add_range(declared.diagnostics().iter().cloned());

                        let selected = self.select_source_execution_obligation(
                            Some(body),
                            None,
                            declared.value(),
                            requested,
                            evidence,
                            diagnostics,
                            cancellation,
                        )?;

                        if let Some(selected) = selected {
                            let target =
                                ExecutionProofOwner::Source(body.source().syntax(), instance);

                            dependencies.insert((target, selected));

                            pending.push((
                                target,
                                Some(body.clone()),
                                selected,
                                exported_owner,
                                false,
                                depth + 1,
                            ));
                        } else {
                            valid = false;
                        }

                        continue;
                    }

                    let target = self
                        .execution_dependency_target(
                            original,
                            instance.map(|instance| instance.substitution()),
                            cancellation,
                        )?
                        .expect(
                            "checked named or opaque dependency normalizes to its semantic target",
                        );

                    if let Some((target, selected, body)) = self.selected_execution_target(
                        target,
                        requested,
                        ExecutionProofEntry::Invocation(evidence),
                        permit_premises,
                        &mut graph,
                        &mut assertions,
                        diagnostics,
                        cancellation,
                    )? {
                        if let Some((anchor, obligation)) = exported_owner {
                            selected_dependencies.insert((anchor, obligation, original, selected));
                        }

                        dependencies.insert((target, selected));

                        if body.is_some() || matches!(target, ExecutionProofOwner::Imported(_)) {
                            let exported = match target {
                                ExecutionProofOwner::Source(anchor, _) => Some((anchor, selected)),
                                _ => exported_owner,
                            };

                            pending.push((target, body, selected, exported, false, depth + 1));
                        }
                    } else {
                        valid = false;
                    }
                }
            }

            if valid {
                graph.insert((owner, required), dependencies);
            } else {
                failure.get_or_insert((proof_owner_span(owner, source), false));
            }
        }

        let failure = execution_proof_failure(&graph, root_owner, source, obligation, failure);

        Ok((failure, assertions, selected_dependencies))
    }
}

fn candidate_dependencies(
    candidate: &bray_checker::ExecutionCandidate,
) -> impl Iterator<
    Item = (
        bray_checker::ExecutionDependencyTarget,
        ExecutionObligation,
        bray_bound_tree::BoundExecutionSite,
    ),
> + '_ {
    let execution = candidate.dependencies().iter().map(|dependency| {
        (
            dependency.target,
            dependency.obligation,
            dependency.occurrence,
        )
    });

    let completion = candidate
        .completion_dependencies()
        .iter()
        .map(|dependency| {
            (
                dependency.target.into(),
                ExecutionObligation::Postcondition(dependency.source),
                dependency.node.into(),
            )
        });

    execution.chain(completion)
}

fn execution_proof_failure(
    graph: &ExecutionProofGraph,
    owner: ExecutionProofOwner,
    source: Option<SyntaxAnchor>,
    obligation: ExecutionObligation,
    mut failure: Option<(Option<SourceSpan>, bool)>,
) -> Option<(Option<SourceSpan>, bool)> {
    let root_key = (owner, obligation);

    if let Some(reason) = check_execution_proof_dependencies(graph).get(&root_key) {
        failure = Some(match reason {
            ExecutionProofFailure::CircularCompletion(owner) => {
                (proof_owner_span(*owner, source), true)
            }
            ExecutionProofFailure::MissingCandidate(owner) => {
                failure.unwrap_or((proof_owner_span(*owner, source), false))
            }
        });
    }

    if !graph.contains_key(&root_key) {
        failure.get_or_insert((proof_owner_span(root_key.0, source), false));
    }

    failure
}

pub(super) fn guarantee_diagnostic(
    obligation: ExecutionObligation,
    source: SourceSpan,
    cause: SourceSpan,
    circular: bool,
) -> Diagnostic {
    let kind = if circular {
        DiagnosticKind::CheckingCircularExecutionGuarantee
    } else {
        DiagnosticKind::CheckingExecutionGuaranteeNotProven
    };

    let diagnostic = Diagnostic::new(
        DiagnosticId::new(source.start().bytes()),
        kind,
        SeverityKind::Error,
    )
    .with_primary_span(source)
    .with_label(DiagnosticLabel::primary(
        DiagnosticLabelKind::InvalidDeclaration,
        source,
    ))
    .with_label(DiagnosticLabel::secondary(
        DiagnosticLabelKind::ExecutionGuaranteeFailure,
        cause,
    ));

    if circular {
        diagnostic
    } else {
        diagnostic.with_arg(DiagnosticArg::referenced_name(
            obligation
                .property()
                .map_or("ensures", |property| property.as_str()),
        ))
    }
}

fn source_span(anchor: SyntaxAnchor) -> SourceSpan {
    SourceSpan::new(anchor.source_id(), anchor.full_range())
}

fn proof_owner_span(
    owner: ExecutionProofOwner,
    source: Option<SyntaxAnchor>,
) -> Option<SourceSpan> {
    match owner {
        ExecutionProofOwner::Source(anchor, _) => Some(source_span(anchor)),
        ExecutionProofOwner::Imported(_)
        | ExecutionProofOwner::Premise(..)
        | ExecutionProofOwner::Indirect(_) => source.map(source_span),
    }
}
