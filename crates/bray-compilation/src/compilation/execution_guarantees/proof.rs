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

impl Compilation {
    pub(super) fn compute_certified_execution(
        &self,
        root: &BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<ExecutionCertification>, FactQueryError> {
        let declared = self.execution_declaration(root.source().syntax())?;

        // The certificate owns source diagnostics independently of the declaration query.
        let mut diagnostics = declared.diagnostics().clone();
        let mut certified = ExecutionCertification::default();
        let candidates = self.execution_candidates_with_cancellation(root.clone(), cancellation)?;

        diagnostics.add_range(candidates.result().diagnostics().iter().cloned());

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

        if self.unit_requires_constant_evaluation(root, cancellation)? {
            let anchor = root.source().syntax();
            let source = source_span(anchor);

            for property in [bray_checker::ExecutionProperty::Pure, bray_checker::ExecutionProperty::Total] {
                if declared.value().properties().iter().any(|declared| declared.property == property) { continue; }
                let obligation = ExecutionObligation::Property(property, None);
                let (failure, assertions, dependencies) = self.certify_execution_obligation(
                    root, obligation, &mut diagnostics, true, cancellation,
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

    fn select_source_execution_obligation(
        &self,
        body: &BoundUnitKey,
        callable: Option<bray_symbols::CallableInstanceData>,
        declaration: &bray_checker::ExecutionDeclaration,
        required: ExecutionObligation,
        evidence: Option<&bray_checker::ExecutionCallEvidence>,
        cancellation: &CancellationToken,
    ) -> Result<Option<ExecutionObligation>, FactQueryError> {
        let selected = match required {
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

    fn foreign_execution_applies(
        &self,
        callable: bray_symbols::CallableInstanceData,
        function: bray_symbols::FunctionSymbolId,
        declaration: &bray_checker::ExecutionDeclaration,
        property: bray_checker::ExecutionProperty,
        evidence: Option<&bray_checker::ExecutionCallEvidence>,
        diagnostics: &mut DiagnosticBag,
        cancellation: &CancellationToken,
    ) -> Result<bool, FactQueryError> {
        let foreign = self.foreign_callable_contract_with_cancellation(function, cancellation)?;

        diagnostics.add_range(foreign.diagnostics().iter().cloned());

        let applicable = if let Some(owner) = self.execution_contract_owner(function.into())? {
            self.applicable_execution_obligation(
                &owner,
                Some(callable),
                declaration,
                ExecutionObligation::Property(property, None),
                evidence,
                cancellation,
            )? == Some(None)
        } else {
            false
        };

        Ok(foreign.value().is_some() && !foreign.diagnostics().has_errors() && applicable)
    }

    pub(in crate::compilation) fn certify_execution_obligation(
        &self,
        root: &BoundUnitKey,
        obligation: ExecutionObligation,
        diagnostics: &mut DiagnosticBag,
        predicate: bool,
        cancellation: &CancellationToken,
    ) -> Result<
        (
            Option<(SourceSpan, bool)>,
            BTreeSet<(SyntaxAnchor, bray_checker::ExecutionProperty)>,
            BTreeSet<(
                SyntaxAnchor,
                ExecutionObligation,
                BoundCallableTarget,
                ExecutionObligation,
            )>,
        ),
        FactQueryError,
    > {
        let mut graph = ExecutionProofGraph::new();
        let mut assertions = BTreeSet::new();
        let mut selected_dependencies = BTreeSet::new();
        let mut visited = BTreeSet::new();

        // The traversal retains each immutable unit identity until its dependencies are checked.
        let mut pending = vec![(
            root.clone(),
            obligation,
            (root.source().syntax(), obligation),
            predicate,
        )];

        let mut failure = None;

        let limit = usize::try_from(bray_checker::ConstantEvaluationLimits::default().call_depth())
            .unwrap_or(1024);

        while let Some((key, obligation, exported_owner, predicate)) = pending.pop() {
            cancellation.check()?;

            let predicate = predicate || self.unit_requires_constant_evaluation(&key, cancellation)?;
            let proof_key = (key.source().syntax(), obligation);

            if !visited.insert((proof_key, exported_owner, predicate)) {
                continue;
            }

            if visited.len() > limit {
                failure = Some((source_span(proof_key.0), false));
                break;
            }

            let candidates = self.execution_candidates_with_cancellation(key, cancellation)?;

            let Some(candidate) = candidates.result().value().get(&obligation) else {
                failure = Some((source_span(proof_key.0), false));
                continue;
            };

            diagnostics.add_range(candidates.result().diagnostics().iter().cloned());

            let local_failure = candidate.failure().or_else(|| predicate.then(|| candidate.constant_failure()).flatten());

            if candidates.result().diagnostics().has_errors() || local_failure.is_some() {
                failure.get_or_insert((
                    local_failure.unwrap_or_else(|| source_span(proof_key.0)),
                    false,
                ));

                continue;
            }

            let mut dependencies = BTreeSet::new();
            let mut valid = true;

            for (target, required, node) in candidate_dependencies(candidate) {
                let target = match target {
                    bray_checker::ExecutionDependencyTarget::Callable(target) => target,
                    bray_checker::ExecutionDependencyTarget::Default(provider) => {
                        if let Some(body) = self.declared_unit_key(provider.symbol(), bray_bound_tree::BoundUnitKind::RuntimeDefault)? {
                            dependencies.insert((ExecutionProofOwner::Source(body.source().syntax()), required));
                            pending.push((body, required, exported_owner, predicate));
                        } else {
                            let imported = self.checked_default_template(provider, cancellation)?;
                            diagnostics.add_range(imported.diagnostics().iter().cloned());
                            valid &= !imported.diagnostics().has_errors() && imported.value().as_ref().is_some_and(|template| {
                                (!predicate || template.behavior().execution().constant_eligible())
                                    && required.property().is_some_and(|property| template.behavior().execution().properties().contains(&property))
                            });
                        }
                        continue;
                    }
                };

                if let BoundCallableTarget::Predicate(_) = target {
                    // Predicate definitions are admitted by their own source or imported owner.
                    // Their calls observe contract meaning and never invoke a runtime body here.
                    continue;
                }

                if matches!(
                    target,
                    BoundCallableTarget::Anonymous(_) | BoundCallableTarget::Indirect(_)
                ) && let Some(body) = candidate
                    .dependency_evidence(node, target)
                    .and_then(bray_checker::ExecutionCallEvidence::anonymous_target)
                {
                    let declared = self.execution_declaration(body.source().syntax())?;
                    diagnostics.add_range(declared.diagnostics().iter().cloned());
                    let selected = self.select_source_execution_obligation(body, None, declared.value(), required,
                        candidate.dependency_evidence(node, target), cancellation)?;
                    if (!predicate || self.unit_requires_constant_evaluation(body, cancellation)?) && let Some(selected) = selected {
                        dependencies.insert((ExecutionProofOwner::Source(body.source().syntax()), selected));
                        // The local body contributes to the enclosing portable proof without local symbol IDs.
                        pending.push((body.clone(), selected, exported_owner, predicate));
                    } else { valid = false; }

                    continue;
                }

                if predicate {
                    let BoundCallableTarget::Declaration(callable) = target else {
                        valid = false;
                        continue;
                    };

                    if !self.is_constant_callable(callable, cancellation)? {
                        valid = false;
                        continue;
                    }
                }

                if let BoundCallableTarget::Indirect(ty) = target {
                    let ty = self.semantic_value_store()?.type_data(ty);

                    // An opaque target can carry purity. Its unknown call graph cannot establish
                    // a new termination proof or a completion predicate.
                    valid &= matches!(ty.as_ref(), bray_symbols::TypeData::Callable(callable)
                        if required == ExecutionObligation::Property(bray_checker::ExecutionProperty::Pure, None)
                            && callable.phase_behaviors().invocation().execution_properties()
                                .contains(&bray_checker::ExecutionProperty::Pure));

                    selected_dependencies.insert((
                        exported_owner.0,
                        exported_owner.1,
                        target,
                        required,
                    ));

                    continue;
                }

                let BoundCallableTarget::Declaration(callable) = target else {
                    valid = false;
                    continue;
                };

                let Some(anchor) = self
                    .symbol_graph()?
                    .declaration_syntax_anchor(callable.definition().symbol())
                else {
                    let selected = self.select_imported_execution_obligation(
                        callable,
                        required,
                        candidate.dependency_evidence(node, target),
                        diagnostics,
                        cancellation,
                    )?;

                    if let Some(selected) = selected {
                        valid &= self.append_imported_execution_proof(
                            callable,
                            selected,
                            &mut graph,
                            diagnostics,
                            cancellation,
                        )?;

                        selected_dependencies.insert((
                            exported_owner.0,
                            exported_owner.1,
                            target,
                            selected,
                        ));

                        dependencies.insert((ExecutionProofOwner::Imported(callable), selected));
                    } else {
                        valid = false;
                    }

                    continue;
                };

                let declaration = self.execution_declaration(anchor)?;

                diagnostics.add_range(declaration.diagnostics().iter().cloned());

                if declaration.diagnostics().has_errors() {
                    valid = false;
                    continue;
                }

                if let Some(body) = self.callable_body_key(callable.definition())? {
                    let selected = self.select_source_execution_obligation(
                        &body,
                        Some(callable),
                        declaration.value(),
                        required,
                        candidate.dependency_evidence(node, target),
                        cancellation,
                    )?;

                    if let Some(selected) = selected {
                        selected_dependencies.insert((
                            exported_owner.0,
                            exported_owner.1,
                            target,
                            selected,
                        ));

                        dependencies.insert((ExecutionProofOwner::Source(anchor), selected));
                        pending.push((body, selected, (anchor, selected), predicate));
                    } else {
                        valid = false;
                    }
                } else if let (
                    bray_symbols::CallableSymbolId::Function(function),
                    ExecutionObligation::Property(property, None),
                ) = (callable.definition().callable_symbol(), required)
                {
                    if self.foreign_execution_applies(
                        callable,
                        function,
                        declaration.value(),
                        property,
                        candidate.dependency_evidence(node, target),
                        diagnostics,
                        cancellation,
                    )? {
                        selected_dependencies.insert((
                            exported_owner.0,
                            exported_owner.1,
                            target,
                            required,
                        ));

                        dependencies.insert((ExecutionProofOwner::Source(anchor), required));

                        graph.insert(
                            (ExecutionProofOwner::Source(anchor), required),
                            BTreeSet::new(),
                        );

                        assertions.insert((anchor, property));
                    } else {
                        valid = false;
                    }
                } else {
                    valid = false;
                }
            }

            if valid {
                graph.insert(
                    (ExecutionProofOwner::Source(proof_key.0), proof_key.1),
                    dependencies,
                );
            } else {
                failure.get_or_insert((source_span(proof_key.0), false));
            }
        }

        let failure = execution_proof_failure(&graph, root.source().syntax(), obligation, failure);

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
            ExecutionObligation::Property(dependency.property, None),
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
    source: SyntaxAnchor,
    obligation: ExecutionObligation,
    mut failure: Option<(SourceSpan, bool)>,
) -> Option<(SourceSpan, bool)> {
    let root_key = (ExecutionProofOwner::Source(source), obligation);

    if let Some(reason) = check_execution_proof_dependencies(&graph).get(&root_key) {
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

fn proof_owner_span(owner: ExecutionProofOwner, source: SyntaxAnchor) -> SourceSpan {
    match owner {
        ExecutionProofOwner::Source(anchor) => source_span(anchor),
        ExecutionProofOwner::Imported(_) => source_span(source),
    }
}
