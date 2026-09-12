use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{BoundCallableTarget, BoundUnitKey};
use bray_checker::{
    ExecutionCertification, ExecutionProofFailure, ExecutionProperty,
    check_execution_proof_dependencies,
};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticId, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
    DiagnosticResult, SeverityKind,
};

use bray_declarations::SyntaxAnchor;
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

        // This publication owns declaration diagnostics independently of the syntax query.
        let mut diagnostics = declared.diagnostics().clone();
        let mut certified = ExecutionCertification::default();

        for property in declared.value().properties() {
            let mut graph = BTreeMap::new();
            let mut foreign_assertions = BTreeSet::new();
            let mut visited = BTreeSet::new();

            // The traversal retains each immutable unit key until its dependencies are checked.
            let mut pending = vec![(root.clone(), property.property)];
            let mut failure = None;

            let limit =
                usize::try_from(bray_checker::ConstantEvaluationLimits::default().call_depth())
                    .unwrap_or(1024);

            while let Some((key, property)) = pending.pop() {
                cancellation.check()?;

                let proof_key = (key.source().syntax(), property);

                if !visited.insert(proof_key) {
                    continue;
                }

                if visited.len() > limit {
                    failure = Some((source_span(proof_key.0), false));
                    break;
                }

                // Candidate publications own the same Arc-backed key as this traversal entry.
                let candidates =
                    self.execution_candidates_with_cancellation(key.clone(), cancellation)?;

                let candidate = &candidates.result().value()[match property {
                    ExecutionProperty::Pure => 0,
                    ExecutionProperty::Total => 1,
                }];

                if candidates.result().diagnostics().has_errors() || candidate.failure().is_some() {
                    // Preserve dependency failures in the public certificate result.
                    diagnostics.add_range(candidates.result().diagnostics().iter().cloned());

                    failure.get_or_insert((
                        candidate
                            .failure()
                            .unwrap_or_else(|| source_span(proof_key.0)),
                        false,
                    ));

                    continue;
                }

                let mut dependencies = BTreeSet::new();
                let mut valid = true;

                for dependency in candidate.dependencies() {
                    let property = dependency.property;

                    match dependency.target {
                        BoundCallableTarget::Declaration(callable) => {
                            let Some(anchor) = self
                                .symbol_graph()?
                                .declaration_syntax_anchor(callable.definition().symbol())
                            else {
                                valid = false;
                                continue;
                            };

                            let declaration = self.execution_declaration(anchor)?;
                            let constant = self.is_constant_callable(callable, cancellation)?;

                            if declaration.diagnostics().has_errors()
                                || declaration.value().has_requirements()
                                || (!constant
                                    && !declaration
                                        .value()
                                        .properties()
                                        .iter()
                                        .any(|declared| declared.property == property))
                            {
                                // Retain the declaration's exact source diagnostics across this proof boundary.
                                diagnostics.add_range(declaration.diagnostics().iter().cloned());
                                valid = false;
                                failure.get_or_insert((source_span(anchor), false));
                                continue;
                            }

                            let dependency_key = (anchor, property);
                            dependencies.insert(dependency_key);

                            if let Some(body) = self.callable_body_key(callable.definition())? {
                                pending.push((body, property));
                            } else if let bray_symbols::CallableSymbolId::Function(function) =
                                callable.definition().callable_symbol()
                            {
                                let foreign = self.foreign_callable_contract_with_cancellation(
                                    function,
                                    cancellation,
                                )?;

                                // Foreign validation diagnostics remain available to direct query consumers.
                                diagnostics.add_range(foreign.diagnostics().iter().cloned());

                                if foreign.value().is_some() && !foreign.diagnostics().has_errors()
                                {
                                    graph.insert(dependency_key, BTreeSet::new());
                                    foreign_assertions.insert(dependency_key);
                                }
                            }
                        }
                        _ => valid = false,
                    }
                }

                if valid {
                    graph.insert(proof_key, dependencies);
                } else {
                    failure.get_or_insert((source_span(proof_key.0), false));
                }
            }

            let failures = check_execution_proof_dependencies(&graph);
            let root_key = (root.source().syntax(), property.property);

            if let Some(reason) = failures.get(&root_key) {
                failure = Some(match reason {
                    ExecutionProofFailure::CircularTotal(anchor) => (source_span(*anchor), true),
                    ExecutionProofFailure::MissingCandidate(anchor) => {
                        failure.unwrap_or((source_span(*anchor), false))
                    }
                });
            }

            if let Some((cause, circular)) = failure {
                let kind = if circular {
                    DiagnosticKind::CheckingCircularExecutionGuarantee
                } else {
                    DiagnosticKind::CheckingExecutionGuaranteeNotProven
                };

                let mut diagnostic = Diagnostic::new(
                    DiagnosticId::new(property.source.start().bytes()),
                    kind,
                    SeverityKind::Error,
                )
                .with_primary_span(property.source)
                .with_label(DiagnosticLabel::primary(
                    DiagnosticLabelKind::InvalidDeclaration,
                    property.source,
                ))
                .with_label(DiagnosticLabel::secondary(
                    DiagnosticLabelKind::ExecutionGuaranteeFailure,
                    cause,
                ));

                if !circular {
                    diagnostic = diagnostic
                        .with_arg(DiagnosticArg::referenced_name(property.property.as_str()));
                }

                diagnostics.add(diagnostic);
            } else if graph.contains_key(&root_key) {
                certified.properties.insert(property.property);
                certified.foreign_assertions.extend(foreign_assertions);
            }
        }

        Ok(DiagnosticResult::new(certified, diagnostics))
    }
}

fn source_span(anchor: SyntaxAnchor) -> SourceSpan {
    SourceSpan::new(anchor.source_id(), anchor.full_range())
}
