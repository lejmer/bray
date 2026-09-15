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

        // TODO(BRA-500): Bind anonymous contracts to their own inputs before exposing callable evidence.
        if root.kind() == bray_bound_tree::BoundUnitKind::AnonymousCallable
            && declared
                .value()
                .domains()
                .iter()
                .any(|domain| !domain.guards.is_empty())
        {
            let diagnostics = bray_checker::check_execution_guarantees(
                &self.execution_declaration_node(root.source().syntax())?,
                &BTreeSet::new(),
            );

            return Ok(DiagnosticResult::new(
                ExecutionCertification::default(),
                diagnostics,
            ));
        }

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

        Ok(DiagnosticResult::new(certified, diagnostics))
    }

    pub(super) fn certify_execution_obligation(
        &self,
        root: &BoundUnitKey,
        obligation: ExecutionObligation,
        diagnostics: &mut DiagnosticBag,
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
        let mut pending = vec![(root.clone(), obligation)];
        let mut failure = None;

        let limit = usize::try_from(bray_checker::ConstantEvaluationLimits::default().call_depth())
            .unwrap_or(1024);

        while let Some((key, obligation)) = pending.pop() {
            cancellation.check()?;
            let proof_key = (key.source().syntax(), obligation);

            if !visited.insert(proof_key) {
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

            if candidates.result().diagnostics().has_errors() || candidate.failure().is_some() {
                failure.get_or_insert((
                    candidate
                        .failure()
                        .unwrap_or_else(|| source_span(proof_key.0)),
                    false,
                ));

                continue;
            }

            let execution = candidate.dependencies().iter().map(|dependency| {
                (
                    dependency.target,
                    ExecutionObligation::Property(dependency.property, None),
                    dependency.node,
                )
            });

            let completion = candidate
                .completion_dependencies()
                .iter()
                .map(|dependency| {
                    (
                        dependency.target,
                        ExecutionObligation::Postcondition(dependency.source),
                        dependency.node,
                    )
                });

            let mut dependencies = BTreeSet::new();
            let mut valid = true;

            for (target, required, node) in execution.chain(completion) {
                if let BoundCallableTarget::Indirect(ty) = target {
                    let ty = self.semantic_value_store()?.type_data(ty);

                    // An opaque target can carry purity. Its unknown call graph cannot establish
                    // a new termination proof or a completion predicate.
                    valid &= matches!(ty.as_ref(), bray_symbols::TypeData::Callable(callable)
                        if required == ExecutionObligation::Property(bray_checker::ExecutionProperty::Pure, None)
                            && callable.phase_behaviors().invocation().execution_properties()
                                .contains(&bray_checker::ExecutionProperty::Pure));

                    selected_dependencies.insert((proof_key.0, proof_key.1, target, required));

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
                        candidate.call_evidence(node),
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

                        selected_dependencies.insert((proof_key.0, proof_key.1, target, selected));
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
                    let selected = match required {
                        ExecutionObligation::Property(property, _) => {
                            let selected = self.applicable_execution_obligation(
                                &body,
                                declaration.value(),
                                property,
                                candidate.call_evidence(node),
                                cancellation,
                            )?;

                            selected.map(|source| {
                                ExecutionObligation::Property(property, source.map(Into::into))
                            })
                        }
                        postcondition => Some(postcondition),
                    };

                    let selected = if selected.is_none()
                        && self.is_constant_callable(callable, cancellation)?
                        && !declaration.value().has_requirements()
                    {
                        Some(required)
                    } else {
                        selected
                    };

                    if let Some(selected) = selected {
                        selected_dependencies.insert((proof_key.0, proof_key.1, target, selected));
                        dependencies.insert((ExecutionProofOwner::Source(anchor), selected));
                        pending.push((body, selected));
                    } else {
                        valid = false;
                    }
                } else if let (
                    bray_symbols::CallableSymbolId::Function(function),
                    ExecutionObligation::Property(property, None),
                ) = (callable.definition().callable_symbol(), required)
                {
                    let foreign =
                        self.foreign_callable_contract_with_cancellation(function, cancellation)?;

                    diagnostics.add_range(foreign.diagnostics().iter().cloned());

                    if foreign.value().is_some()
                        && !foreign.diagnostics().has_errors()
                        && !declaration.value().has_requirements()
                        && declaration
                            .value()
                            .properties()
                            .iter()
                            .any(|declared| declared.property == property)
                    {
                        selected_dependencies.insert((proof_key.0, proof_key.1, target, required));
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

        let root_key = (
            ExecutionProofOwner::Source(root.source().syntax()),
            obligation,
        );

        if let Some(reason) = check_execution_proof_dependencies(&graph).get(&root_key) {
            failure = Some(match reason {
                ExecutionProofFailure::CircularCompletion(owner) => {
                    (proof_owner_span(*owner, root.source().syntax()), true)
                }
                ExecutionProofFailure::MissingCandidate(owner) => {
                    failure.unwrap_or((proof_owner_span(*owner, root.source().syntax()), false))
                }
            });
        }

        if !graph.contains_key(&root_key) {
            failure.get_or_insert((proof_owner_span(root_key.0, root.source().syntax()), false));
        }

        Ok((failure, assertions, selected_dependencies))
    }
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
