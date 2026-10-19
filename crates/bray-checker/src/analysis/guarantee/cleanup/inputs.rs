use std::collections::{BTreeMap, BTreeSet};

use bray_diagnostics::DiagnosticBag;
use bray_symbols::TypeId;

use super::plan::{Plan, has_source_lifecycle};
use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};

impl Plan {
    pub(super) fn select_scoped_quiescence<C: CheckerRequestContext + ?Sized>(
        &mut self,
        cleanup_shapes: &mut crate::asynchronous::CleanupShapeResolver<'_, C>,
        request: CheckerUnitView<'_, C>,
        selections: &bray_bound_tree::CheckedSemanticSelections,
    ) -> Result<(), CheckerQueryError<C::UpstreamError>> {
        for entry in selections.entries() {
            let bray_bound_tree::SemanticSelection::ScopedUse(scoped) = entry.selection() else {
                continue;
            };

            // Async exit already publishes the checked continuation reused by error quiescence.
            if matches!(
                scoped.exit_result(),
                bray_bound_tree::BoundCallResult::LazyFuture(_)
            ) {
                continue;
            }

            if crate::representation::type_representation(request, scoped.exit().1.result())
                != Some(bray_compiler_known::RepresentationRole::Result)
            {
                continue;
            }

            let Some(error) = scoped.failure_type() else {
                continue;
            };

            if !cleanup_shapes.quiescence_requires_suspension(error)? {
                continue;
            }

            let Some(bray_bound_tree::BoundExpression::Structured(expression)) =
                request.view().expression(scoped.expression())
            else {
                panic!("checked scoped use retains its structured expression");
            };

            let [body] = expression.blocks() else {
                panic!("checked scoped use retains its sole body");
            };

            self.scoped_error_quiescence
                .insert(*body, scoped.expression());
        }

        Ok(())
    }

    pub(super) fn select_inputs(
        &mut self,
        unit: &bray_bound_tree::BoundUnit,
        types: &bray_bound_tree::CheckedExpressionTypes,
        graph: &crate::analysis::ControlFlowGraph,
        cleanup: &BTreeMap<TypeId, BTreeSet<bray_bound_tree::BodyBehaviorCall>>,
    ) {
        if cleanup.is_empty() {
            return;
        }

        let exits = graph
            .operations()
            .iter()
            .filter_map(|operation| match operation.kind() {
                crate::analysis::AnalysisOperationKind::ScopeExit {
                    exit: bray_bound_tree::AnyBoundNodeId::Expression(expression),
                    ..
                } => Some(expression),
                _ => None,
            })
            .collect::<BTreeSet<_>>();

        let mut selected = BTreeMap::<_, BTreeSet<_>>::new();

        for (expression, node) in unit.tree().expressions() {
            if !matches!(
                node,
                bray_bound_tree::BoundExpression::Call(_)
                    | bray_bound_tree::BoundExpression::StructConstruction(_)
                    | bray_bound_tree::BoundExpression::BoxConstruction(_)
            ) {
                continue;
            }

            let mut pending = BTreeSet::new();
            let mut suspends = false;

            for input in node.child_expressions() {
                // Earlier inputs remain owned while later inputs are evaluated.
                if suspends {
                    let walked = bray_bound_tree::walk_bound_tree(unit.tree(), input, |event| {
                        if let bray_bound_tree::BoundWalkEvent::Enter(
                            bray_bound_tree::AnyBoundNodeId::Expression(input),
                        ) = event
                            && exits.contains(&input)
                        {
                            selected
                                .entry(input)
                                .or_default()
                                .extend(pending.iter().cloned());
                        }

                        bray_bound_tree::BoundWalkControl::Continue
                    });

                    assert_eq!(
                        walked,
                        bray_bound_tree::BoundWalkOutcome::Completed,
                        "pending input cleanup retains its complete bound subtree"
                    );
                }

                if let Some(ty) = types.expression(input)
                    && let Some(callees) = cleanup.get(&ty.ty())
                {
                    suspends = true;
                    pending.extend(callees.iter().cloned());
                }
            }

            if suspends && exits.contains(&expression) {
                selected.entry(expression).or_default().extend(pending);
            }
        }

        self.input_cleanup_calls = selected;
    }
}

pub(super) fn parts<C: CheckerRequestContext + ?Sized>(
    cleanup_shapes: &mut crate::asynchronous::CleanupShapeResolver<'_, C>,
    request: CheckerUnitView<'_, C>,
    types: &bray_bound_tree::CheckedExpressionTypes,
    lifecycles: &mut BTreeSet<TypeId>,
    diagnostics: &mut DiagnosticBag,
) -> Result<
    BTreeMap<TypeId, Vec<(TypeId, Option<bray_bound_tree::StorageCleanupPart>)>>,
    CheckerQueryError<C::UpstreamError>,
> {
    let mut inputs = BTreeMap::new();

    for (_, expression) in request.unit().tree().expressions() {
        if !matches!(
            expression,
            bray_bound_tree::BoundExpression::Call(_)
                | bray_bound_tree::BoundExpression::StructConstruction(_)
                | bray_bound_tree::BoundExpression::BoxConstruction(_)
        ) {
            continue;
        }

        for input in expression.child_expressions() {
            let Some(ty) = types.expression(input) else {
                continue;
            };

            if inputs.contains_key(&ty.ty()) {
                continue;
            }

            let source = request
                .view()
                .expression(input)
                .expect("construction input retains its bound node")
                .origin()
                .source_anchor();

            let (parts, additional) =
                crate::asynchronous::full_cleanup_parts(cleanup_shapes, ty.ty(), source, false)?
                    .into_parts();

            diagnostics.add_range(additional);

            for (part, projection) in &parts {
                if has_source_lifecycle(cleanup_shapes, request, *part, projection.as_ref())? {
                    lifecycles.insert(*part);
                }
            }

            inputs.insert(ty.ty(), parts);
        }
    }

    Ok(inputs)
}
