use bray_bound_tree::{
    BoundBlockId, BoundExpressionId, BoundOperator, BoundPatternId, BoundStructuredExpressionKind,
};
use bray_declarations::SyntaxAnchor;

use crate::CheckerRequestContext;

use super::build::{CatchContext, ControlFlowGraphBuilder, LoopContext};
use super::id::AnalysisBlockId;
use super::model::{AnalysisCallPhase, AnalysisEdgeKind, AnalysisExitKind, AnalysisRefinement};

impl<C> ControlFlowGraphBuilder<'_, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) fn build_structured(
        &mut self,
        id: BoundExpressionId,
        expression: &bray_bound_tree::BoundStructuredExpression,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        match expression.kind() {
            BoundStructuredExpressionKind::With => self.build_scoped_use(id, expression, current),
            BoundStructuredExpressionKind::PatternTest => {
                let [scope] = expression.blocks() else {
                    return None;
                };

                self.push_bound(current, (*scope).into());
                self.scopes.push((*scope).into());

                let completion = self.build_operands(expression.operands(), current)?;

                self.push_bound(completion, id.into());
                self.scopes.pop();

                Some(Some(self.push_scope_exit(
                    completion,
                    (*scope).into(),
                    id.into(),
                    None,
                )))
            }
            BoundStructuredExpressionKind::Conditional => {
                self.build_branches(id, expression.blocks(), expression.operands(), current)
            }
            BoundStructuredExpressionKind::While => {
                let Some(body) = expression.blocks().first().copied() else {
                    self.push_recovery(current, id.into());

                    return Some(Some(current));
                };

                self.build_while(
                    id,
                    expression.operands(),
                    body,
                    expression.blocks().get(1).copied(),
                    expression.origin().source_anchor().syntax(),
                    current,
                )
            }
            BoundStructuredExpressionKind::Loop => {
                let Some(body) = expression.blocks().first().copied() else {
                    self.push_recovery(current, id.into());

                    return Some(Some(current));
                };

                self.build_unconditional_loop(
                    id,
                    body,
                    expression.origin().source_anchor().syntax(),
                    current,
                )
            }
            BoundStructuredExpressionKind::Assertion => {
                self.build_assertion(id, expression.operands(), current)
            }
            BoundStructuredExpressionKind::TrustBoundary => {
                self.build_trust_boundary(id, expression.operands(), expression.blocks(), current)
            }
            BoundStructuredExpressionKind::BooleanAllFold
            | BoundStructuredExpressionKind::BooleanAnyFold => {
                self.build_boolean_fold(id, expression.operands(), current)
            }
            BoundStructuredExpressionKind::ArrayGenerator
            | BoundStructuredExpressionKind::GeneralGenerator => {
                self.build_generator_region(id, expression, current)
            }
            BoundStructuredExpressionKind::Catch => {
                self.build_catch(id, expression.operands(), expression.blocks(), current)
            }
            BoundStructuredExpressionKind::ResultPropagation => {
                let current = self.build_operands(expression.operands(), current)?;

                self.build_result_or_run_result_propagation(id, expression, current)
            }
            BoundStructuredExpressionKind::NullablePropagation => {
                let current = self.build_operands(expression.operands(), current)?;

                self.build_nullable_propagation(id, current)
            }
            BoundStructuredExpressionKind::Panic => {
                let current = self.build_operands(expression.operands(), current)?;

                self.push_bound(current, id.into());
                self.push_exit(current, AnalysisExitKind::Panic, id.into());

                Some(None)
            }
            _ => {
                let mut current = self.build_operands(expression.runtime_operands(), current)?;

                current = self.push_source_operation(id, current);

                for block in expression.blocks() {
                    current = self
                        .build_block(*block, current)?
                        .unwrap_or_else(|| self.push_block());
                }

                Some(Some(current))
            }
        }
    }

    fn build_scoped_use(
        &mut self,
        id: BoundExpressionId,
        expression: &bray_bound_tree::BoundStructuredExpression,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let ([initializer], [pattern], [body]) = (
            expression.operands(),
            expression.patterns(),
            expression.blocks(),
        ) else {
            panic!("with occurrence {id:?} retains its initializer, pattern and body");
        };

        let Some(mut current) = self.build_expression(*initializer, current)? else {
            return Some(None);
        };

        let completion = self.push_block();

        if self.scoped_uses.contains_key(&(*body).into()) {
            let completed = self.push_block();
            let invocation = bray_bound_tree::BoundExecutionSite::ScopedEnter(id);

            let Some(bray_bound_tree::SemanticSelection::ScopedUse(scoped)) = self
                .selections()
                .and_then(|selections| selections.expression(id)) else {
                panic!("checked scope entry retains its lifecycle selection");
            };

            let entry_type = scoped.enter_result().completion_type();
            let suspends = matches!(scoped.enter_result(), bray_bound_tree::BoundCallResult::LazyFuture(_));
            let exit = scoped.exit_result();

            let reserves_backing = suspends
                || matches!(exit, bray_bound_tree::BoundCallResult::LazyFuture(_))
                || self.request().available_compiler_known_symbols()
                    .result_payloads(self.request().semantic_values(), exit.completion_type()).is_some();

            let entry_callable = self.request().semantic_values().type_data(scoped.enter().1.callable_type());

            let bray_symbols::TypeData::Callable(entry_callable) = entry_callable.as_ref() else {
                unreachable!("the selected scope entry retains its callable signature");
            };

            let checks_requirements = entry_callable.phase_behaviors().invocation().has_runtime_requirements();

            current = self.begin_invocation(invocation, current, checks_requirements);

            if reserves_backing {
                // Admission follows the requirements and leaves the receiver with the caller on failure.
                current = self.push_storage_admission(
                    current, id.into(),
                    Some(super::model::AnalysisRefinement::InvocationBackingFailure(invocation)),
                );
            }

            self.push_call(
                current,
                invocation,
                super::model::AnalysisCallPhase::Attempt,
            );

            if suspends {
                self.push_suspension(
                    current,
                    invocation,
                    super::model::AnalysisSuspensionKind::ScopedCall,
                );

                let suspended = self.push_block();

                self.push_edge(current, suspended, AnalysisEdgeKind::Suspension, None);
                self.connect_suspended_invocation(suspended, completed, invocation);
            } else {
                self.push_edge(
                    current,
                    completed,
                    AnalysisEdgeKind::Sequential,
                    Some(super::model::AnalysisRefinement::InvocationCompletion(
                        invocation,
                    )),
                );

                self.push_dependency_failures(current, invocation);
            }

            self.push_call(completed, invocation, AnalysisCallPhase::Completion);

            let entered = if entry_type != scoped.capability_type() {
                use super::model::{AnalysisOperationKind, ScopedResultDestination, ScopedResultTransfer};
                use bray_bound_tree::StorageProjection;

                let (_, representation) = self.request().available_compiler_known_symbols()
                    .result_payloads(self.request().semantic_values(), entry_type)
                    .expect("fallible entry retains its checked Result representation");

                let storage = self.checked_storage().expect("fallible entry retains its storage plan");

                let source = storage.invocation_result(invocation).and_then(|access| storage.root_identity(access))
                    .expect("fallible entry retains its raw result slot");

                let capability = storage.implicit_input_plan(bray_bound_tree::BoundExecutionSite::ScopedExit(id))
                    .and_then(|input| storage.root_identity(input.access()))
                    .expect("selected exit consumes the exact entered capability");

                let result = storage.value_slot(id).expect("fallible scope has a private result slot");
                let success = StorageProjection::ActiveUnionPayloadField { variant: representation.success_variant(), field: representation.success_field() };
                let error = StorageProjection::ActiveUnionPayloadField { variant: representation.error_variant(), field: representation.error_field() };

                let entered = self.push_block();
                let failed = self.push_block();

                self.push_edge(completed, entered, AnalysisEdgeKind::ResultSuccess,
                    Some(AnalysisRefinement::InvocationVariant(invocation, representation.success_variant())));

                self.push_edge(
                    completed,
                    failed,
                    AnalysisEdgeKind::ResultErrorPropagation,
                    Some(AnalysisRefinement::InvocationVariant(invocation, representation.error_variant())),
                );

                self.storage.push_operation(entered, AnalysisOperationKind::ScopedResult(ScopedResultTransfer {
                    invocation, source: Some(source), projection: success, destination: ScopedResultDestination::Capability(capability),
                }));

                self.storage.push_operation(failed, AnalysisOperationKind::ScopedResult(ScopedResultTransfer {
                    invocation, source: Some(source), projection: error, destination: ScopedResultDestination::Region { slot: result, displaced: None },
                }));

                let failed = if suspends { self.continue_committed_invocation(failed, id) } else { failed };

                self.push_edge(failed, completion, AnalysisEdgeKind::Sequential, None);

                entered
            } else {
                completed
            };

            current = if suspends {
                // The acquired capability owns its checked body scope before the pattern runs.
                self.scopes.push((*body).into());

                let continued = self.continue_committed_invocation(entered, id);

                self.scopes.pop();

                continued
            } else {
                entered
            };
        }

        let current = self.build_pattern(*pattern, current)?;
        let current = current.unwrap_or_else(|| self.push_block());

        self.result_yields.push(super::build::ResultYieldContext {
            target: self
                .request()
                .view()
                .block(*body)?
                .origin()
                .source_anchor()
                .syntax(),
            completion,
            scope_depth: self.scope_depth(),
            destination: self.checked_storage().and_then(|storage| storage.value_slot(id)),
        });

        let body_completion = self.build_block(*body, current);

        self.result_yields.pop();

        if let Some(body_completion) = body_completion? {
            self.push_edge(
                body_completion,
                completion,
                AnalysisEdgeKind::Sequential,
                None,
            );
        }

        Some(Some(completion))
    }

    pub(super) fn push_scoped_fallthrough(&mut self, current: AnalysisBlockId, scope: bray_bound_tree::StorageScope) {
        use super::model::{AnalysisOperationKind, ScopedResultDestination, ScopedResultTransfer};

        let Some(&expression) = self.scoped_uses.get(&scope) else { return; };

        let Some(bray_bound_tree::SemanticSelection::ScopedUse(scoped)) = self.selections().and_then(|selections| selections.expression(expression)) else {
            unreachable!("scoped fallthrough retains its checked selection");
        };

        if scoped.failure_type().is_none() { return; }

        let storage = self.checked_storage().expect("scoped fallthrough retains its storage plan");

        let Some(slot) = storage.value_slot(expression) else { return; };

        let ty = storage.storage_type(slot).expect("scoped fallthrough has its checked region type");

        let Some(([body, _], representation)) = self.request().available_compiler_known_symbols()
            .result_payloads(self.request().semantic_values(), ty) else { return; };

        if crate::representation::type_representation(self.request(), body) != Some(bray_compiler_known::RepresentationRole::Unit) { return; }

        self.storage.push_operation(current, AnalysisOperationKind::ScopedResult(ScopedResultTransfer {
            invocation: expression.into(), source: None,
            projection: bray_bound_tree::StorageProjection::ActiveUnionPayloadField {
                variant: representation.success_variant(), field: representation.success_field(),
            },
            destination: ScopedResultDestination::Region { slot, displaced: None },
        }));
    }

    fn build_generator_region(
        &mut self,
        id: BoundExpressionId,
        expression: &bray_bound_tree::BoundStructuredExpression,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let current = if expression.kind() == BoundStructuredExpressionKind::GeneralGenerator {
            self.push_storage_admission(current, id.into(), Some(super::model::AnalysisRefinement::GeneratorStorageFailure(id)))
        } else {
            current
        };

        let owner = self.checked_storage().and_then(|storage| storage.value_slot(id));

        let abandoned = owner.map(|value| bray_bound_tree::StorageScope::AbandonedTransfer {
            transfer: id.into(), value,
        });

        let depth = self.scopes.len();

        self.push_result(current, id, super::model::AnalysisResultPhase::Begin);
        self.scopes.extend(abandoned);

        self.yield_regions
            .push((expression.origin().source_anchor().syntax(), id));

        let completion = (|| {
            let mut current = self.build_operands(expression.operands(), current)?;

            for block in expression.blocks() {
                current = self.build_block(*block, current)?.unwrap_or_else(|| self.push_block());
            }

            Some(current)
        })();

        self.yield_regions.pop();
        self.scopes.truncate(depth);

        let current = completion?;

        self.push_result(current, id, super::model::AnalysisResultPhase::Completion);

        Some(Some(current))
    }

    pub(super) fn build_short_circuit(
        &mut self,
        id: BoundExpressionId,
        operands: &[BoundExpressionId],
        operator: BoundOperator,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        if operands.len() != 2 {
            self.push_recovery(current, id.into());

            return Some(Some(current));
        }

        if !matches!(operator, BoundOperator::LogicalAnd | BoundOperator::LogicalOr) {
            return Some(Some(current));
        }

        use super::model::AnalysisResultPhase;

        self.push_result(current, id, AnalysisResultPhase::Begin);

        let shortcut = operator == BoundOperator::LogicalOr;
        let right = self.push_block();
        let skipped = self.push_block();
        let join = self.push_block();

        self.build_value_branch(operands[0], current,
            (if shortcut { skipped } else { right }, AnalysisEdgeKind::ConditionalTrue),
            (if shortcut { right } else { skipped }, AnalysisEdgeKind::ConditionalFalse))?;

        self.push_edge(skipped, join, AnalysisEdgeKind::Sequential,
            Some(AnalysisRefinement::Condition { expression: id, value: shortcut }));

        if let Some(completed) = self.build_expression(operands[1], right)? {
            self.push_result(completed, id, AnalysisResultPhase::Production);
            self.push_edge(completed, join, AnalysisEdgeKind::Sequential, None);
        }

        self.push_result(join, id, AnalysisResultPhase::Completion);

        Some(Some(join))
    }

    fn build_assertion(
        &mut self,
        id: BoundExpressionId,
        operands: &[BoundExpressionId],
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let Some(condition) = operands.first().copied() else {
            self.push_recovery(current, id.into());

            return Some(Some(current));
        };

        self.push_bound(current, id.into());

        let success = self.push_block();
        let failure = self.push_block();

        self.build_condition(
            condition,
            current,
            (success, AnalysisEdgeKind::ConditionalTrue),
            (failure, AnalysisEdgeKind::ConditionalFalse),
        )?;

        let failure = self.build_operands(operands.get(1..).unwrap_or_default(), failure)?;

        self.push_exit(failure, AnalysisExitKind::Panic, id.into());

        Some(Some(success))
    }

    fn build_trust_boundary(
        &mut self,
        id: BoundExpressionId,
        operands: &[BoundExpressionId],
        blocks: &[BoundBlockId],
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let entry = self.push_block();

        self.push_edge(
            current,
            entry,
            AnalysisEdgeKind::Sequential,
            Some(AnalysisRefinement::TrustBoundary(id)),
        );

        let mut current = self.build_operands(operands, entry)?;

        for block in blocks {
            let Some(completion) = self.build_block(*block, current)? else {
                return Some(None);
            };

            current = completion;
        }

        self.push_bound(current, id.into());

        Some(Some(current))
    }

    fn build_boolean_fold(
        &mut self,
        id: BoundExpressionId,
        operands: &[BoundExpressionId],
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let current = self.build_operands(operands, current)?;

        let (header, item, exhausted) = self.build_iteration_entry(id, current);

        let completion = self.push_block();

        let all = matches!(self.view().expression(id),
            Some(bray_bound_tree::BoundExpression::Structured(operation))
                if operation.kind() == BoundStructuredExpressionKind::BooleanAllFold);

        let selected = self.has_complete_iteration(id);

        let next = bray_bound_tree::BoundExecutionSite::Next(id);

        self.push_edge(
            item,
            header,
            AnalysisEdgeKind::LoopBack,
            selected.then_some(AnalysisRefinement::InvocationBoolean(next, all)),
        );

        self.push_edge(
            item,
            completion,
            AnalysisEdgeKind::ConditionalFalse,
            selected.then_some(AnalysisRefinement::InvocationBoolean(next, !all)),
        );

        self.push_edge(exhausted, completion, AnalysisEdgeKind::Sequential, None);

        Some(Some(completion))
    }

    fn has_complete_iteration(&self, id: BoundExpressionId) -> bool {
        self.view().node_is_recovered(id.into()) == Some(false)
            && self.selections().is_some_and(|selections| {
                matches!(
                    selections.expression(id),
                    Some(bray_bound_tree::SemanticSelection::Iteration(_))
                )
            })
    }

    fn build_iteration_entry(
        &mut self,
        id: BoundExpressionId,
        current: AnalysisBlockId,
    ) -> (AnalysisBlockId, AnalysisBlockId, AnalysisBlockId) {
        let selected = self.has_complete_iteration(id);

        let current = if selected {
            self.push_protocol_invocation(bray_bound_tree::BoundExecutionSite::Iterate(id), current)
        } else {
            // Untyped control flow owns loop topology without selected protocol calls.
            if self.selections().is_some() {
                self.push_recovery(current, id.into());
            }

            current
        };

        let header = self.storage.push_loop_header(id);
        let item = self.push_block();
        let exhausted = self.push_block();

        self.push_edge(current, header, AnalysisEdgeKind::LoopEntry, None);

        let next = bray_bound_tree::BoundExecutionSite::Next(id);

        let advanced = if selected {
            self.push_protocol_invocation(next, header)
        } else {
            header
        };

        self.push_edge(
            advanced,
            item,
            AnalysisEdgeKind::ConditionalTrue,
            selected.then_some(AnalysisRefinement::InvocationPresence(next, true)),
        );

        self.push_edge(
            advanced,
            exhausted,
            AnalysisEdgeKind::ConditionalFalse,
            selected.then_some(AnalysisRefinement::InvocationPresence(next, false)),
        );

        (header, item, exhausted)
    }

    fn build_catch(
        &mut self,
        id: BoundExpressionId,
        operands: &[BoundExpressionId],
        blocks: &[BoundBlockId],
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let join = self.push_block();

        self.catches.push(CatchContext {
            expression: id,
            target: join,
            scope_depth: self.scope_depth(),
        });

        let parent = self.storage.replace_report_boundary(Some(id));
        let entry = self.push_block();

        self.push_edge(current, entry, AnalysisEdgeKind::Sequential, None);

        let mut completion = self.build_operands(operands, entry)?;

        for block in blocks {
            completion = self
                .build_block(*block, completion)?
                .unwrap_or_else(|| self.push_block());
        }

        self.catches.pop();
        self.storage.replace_report_boundary(parent);

        self.push_edge(
            completion,
            join,
            AnalysisEdgeKind::Sequential,
            Some(AnalysisRefinement::CaughtResult {
                expression: id,
                outcome: super::model::AnalysisCaughtResult::Completed(operands.last().copied()),
            }),
        );

        Some(Some(join))
    }

    pub(super) fn build_for(
        &mut self,
        id: BoundExpressionId,
        expression: &bray_bound_tree::BoundForExpression,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let current = self.build_expression(expression.source(), current)?;
        let current = current.unwrap_or_else(|| self.push_block());

        self.build_iteration(
            id,
            expression.pattern(),
            expression.body(),
            expression.else_body(),
            expression.origin().source_anchor().syntax(),
            current,
        )
    }

    pub(super) fn build_match(
        &mut self,
        id: BoundExpressionId,
        expression: &bray_bound_tree::BoundMatchExpression,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let current = self.build_expression(expression.subject(), current)?;
        let current = current.unwrap_or_else(|| self.push_block());

        let join = self.push_block();
        let mut candidate = current;

        for arm in expression.arms() {
            let arm_entry = self.push_block();
            let next_candidate = self.push_block();

            let refinement = AnalysisRefinement::PatternOutcome {
                subject: expression.subject(),
                pattern: arm.pattern(),
                value: true,
            };

            self.push_edge(
                candidate,
                arm_entry,
                AnalysisEdgeKind::MatchArm,
                Some(refinement),
            );

            self.push_edge(
                candidate,
                next_candidate,
                AnalysisEdgeKind::MatchNoMatch,
                Some(AnalysisRefinement::PatternOutcome {
                    subject: expression.subject(),
                    pattern: arm.pattern(),
                    value: false,
                }),
            );

            let body_entry = match arm.guard() {
                Some(guard) => {
                    self.build_pattern_observations(arm.pattern(), arm_entry)?;

                    let body_entry = self.push_block();

                    self.build_condition(
                        guard,
                        arm_entry,
                        (body_entry, AnalysisEdgeKind::ConditionalTrue),
                        (next_candidate, AnalysisEdgeKind::ConditionalFalse),
                    )?;

                    body_entry
                }
                None => arm_entry,
            };

            let body_entry = self
                .build_pattern(arm.pattern(), body_entry)?
                .unwrap_or_else(|| self.push_block());

            if let Some(completion) =
                self.build_result_branch(id, arm.body(), body_entry, join, self.scope_depth())?
            {
                self.push_edge(completion, join, AnalysisEdgeKind::Sequential, None);
            }

            candidate = next_candidate;
        }

        let exhaustive = self
            .patterns
            .and_then(|patterns| patterns.match_coverage(id))
            .is_some_and(|coverage| coverage.is_exhaustive() && !coverage.is_recovered());

        if !exhaustive {
            self.push_edge(candidate, join, AnalysisEdgeKind::MatchNoMatch, None);
        }

        Some(Some(join))
    }

    fn build_while(
        &mut self,
        id: BoundExpressionId,
        condition: &[BoundExpressionId],
        body: BoundBlockId,
        else_body: Option<BoundBlockId>,
        target: SyntaxAnchor,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let header = self.storage.push_loop_header(id);
        let body_entry = self.push_block();
        let exhausted = self.push_block();
        let join = self.push_block();

        self.push_edge(current, header, AnalysisEdgeKind::Sequential, None);

        let condition = condition.first().copied()?;

        self.push_result(header, id, super::model::AnalysisResultPhase::Begin);

        let depth = self.scope_depth();
        let scope = self.begin_condition_scope(condition, header);

        self.build_condition(
            condition,
            header,
            (body_entry, AnalysisEdgeKind::LoopEntry),
            (exhausted, AnalysisEdgeKind::ConditionalFalse),
        )?;

        self.loops.push(LoopContext {
            target,
            continue_target: Some(header),
            completion: join,
            scope_depth: depth,
        });

        if let Some(body_exit) = self.build_block(body, body_entry)? {
            let body_exit = scope.map_or(body_exit, |scope| {
                self.push_scope_exit(body_exit, scope.into(), body.into(), None)
            });

            self.push_edge(body_exit, header, AnalysisEdgeKind::LoopBack, None);
        }

        let exhausted = if let Some(scope) = scope {
            self.scopes.pop();

            self.push_scope_exit(exhausted, scope.into(), condition.into(), None)
        } else {
            exhausted
        };

        if let Some(context) = self.loops.last_mut() {
            context.continue_target = None;
        }

        match else_body {
            Some(else_body) => {
                if let Some(else_exit) = self.build_block(else_body, exhausted)? {
                    self.push_edge(else_exit, join, AnalysisEdgeKind::Sequential, None);
                }
            }
            None => self.push_edge(exhausted, join, AnalysisEdgeKind::Sequential, None),
        }

        self.loops.pop();

        Some(Some(join))
    }

    fn build_unconditional_loop(
        &mut self,
        id: BoundExpressionId,
        body: BoundBlockId,
        target: SyntaxAnchor,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let header = self.storage.push_loop_header(id);
        let body_entry = self.push_block();
        let join = self.push_block();

        self.push_edge(current, header, AnalysisEdgeKind::Sequential, None);
        self.push_result(header, id, super::model::AnalysisResultPhase::Begin);
        self.push_edge(header, body_entry, AnalysisEdgeKind::LoopEntry, None);
        self.push_exit(header, AnalysisExitKind::Divergence, id.into());

        self.loops.push(LoopContext {
            target,
            continue_target: Some(header),
            completion: join,
            scope_depth: self.scope_depth(),
        });

        if let Some(body_exit) = self.build_block(body, body_entry)? {
            self.push_edge(body_exit, header, AnalysisEdgeKind::LoopBack, None);
        }

        self.loops.pop();

        Some(Some(join))
    }

    pub(super) fn build_iteration(
        &mut self,
        id: BoundExpressionId,
        pattern: BoundPatternId,
        body: BoundBlockId,
        else_body: Option<BoundBlockId>,
        target: SyntaxAnchor,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let (header, item, exhausted) = self.build_iteration_entry(id, current);

        let join = self.push_block();

        self.loops.push(LoopContext {
            target,
            continue_target: Some(header),
            completion: join,
            scope_depth: self.scope_depth(),
        });

        let item = self
            .build_pattern(pattern, item)?
            .unwrap_or_else(|| self.push_block());

        if let Some(body_exit) = self.build_block(body, item)? {
            self.push_edge(body_exit, header, AnalysisEdgeKind::LoopBack, None);
        }

        if let Some(context) = self.loops.last_mut() {
            context.continue_target = None;
        }

        match else_body {
            Some(else_body) => {
                if let Some(else_exit) = self.build_block(else_body, exhausted)? {
                    self.push_edge(else_exit, join, AnalysisEdgeKind::Sequential, None);
                }
            }
            None => self.push_edge(exhausted, join, AnalysisEdgeKind::Sequential, None),
        }

        self.loops.pop();

        Some(Some(join))
    }
}
