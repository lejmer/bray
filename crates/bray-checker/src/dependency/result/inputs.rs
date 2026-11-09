use std::collections::BTreeSet;

use bray_bound_tree::BoundExecutionSite;
use bray_symbols::{
    DependencyRequirement, DependencyRequirementKind, DependencySubject, DependencySubjectRoot,
};

use super::core::Transfer;
use crate::{CheckerQueryError, CheckerRequestContext};

impl<C: CheckerRequestContext + ?Sized> Transfer<'_, '_, C> {
    pub(super) fn borrowed_input_receiver(&self, input: BoundExecutionSite) -> Option<bray_bound_tree::BoundExpressionId> {
        let BoundExecutionSite::CallInput { expression, ordinal } = input else { return None; };
        let bray_bound_tree::SelectedInput::Receiver(receiver) = self.selections.expression(expression)
            .expect("input dependency transfer retains its invocation")
            .input(ordinal).expect("input dependency transfer retains its exact input") else { return None; };

        matches!(receiver.mode(), bray_symbols::ReceiverMode::Shared | bray_symbols::ReceiverMode::Mutable)
            .then_some(receiver.expression())
    }

    pub(super) fn input_values(
        &self,
        input: BoundExecutionSite,
    ) -> Result<BTreeSet<DependencyRequirement>, CheckerQueryError<C::UpstreamError>> {
        let site = input;

        if self.inputs.is_independent(input) {
            return Ok(BTreeSet::new());
        }

        let BoundExecutionSite::CallInput {
            expression,
            ordinal,
        } = input
        else {
            panic!("input dependency transfer has an exact input occurrence");
        };

        let selection = self
            .selections
            .expression(expression)
            .expect("input dependency transfer retains its invocation");

        if let Some(receiver) = self.borrowed_input_receiver(site) {
            if self.types.expression(receiver).is_some_and(|ty| {
                matches!(self.request.semantic_values().type_data(ty.ty()).as_ref(),
                    bray_symbols::TypeData::Borrow { .. })
            }) {
                // Implicit receiver borrowing reaches the existing referent. The
                // temporary slot containing its descriptor is not a lifetime anchor.
                return Ok(self.projected_values(receiver, &[]));
            }

            return Ok(self.borrowed_values(receiver, &[]));
        }

        let template =
            crate::dependency::call::selected_input_template(self.request, selection, ordinal)?;

        self.mapped_result_values(template.requirements(), |root| {
            self.input_argument_site(site, root)
        })
    }

    pub(super) fn input_argument_site(
        &self,
        input: BoundExecutionSite,
        root: DependencySubjectRoot,
    ) -> Option<BoundExecutionSite> {
        let BoundExecutionSite::CallInput { expression, .. } = input else {
            panic!("input mapping has an exact input occurrence");
        };

        let selection = self
            .selections
            .expression(expression)
            .expect("input mapping retains its invocation");

        selection.invocation_input_site(input, root)
    }

    pub(super) fn mapped_site_values(
        &self,
        argument: BoundExecutionSite,
        path: &[bray_symbols::DependencyProjection],
        kind: DependencyRequirementKind,
    ) -> Result<Vec<DependencyRequirement>, CheckerQueryError<C::UpstreamError>> {
        let ty = match argument {
            BoundExecutionSite::Node(bray_bound_tree::AnyBoundNodeId::Expression(expression)) => {
                return self.mapped_input_values(expression, path, kind);
            }
            BoundExecutionSite::CallInput {
                expression,
                ordinal,
            } => {
                let input = self.selections.expression(expression)
                    .expect("staged input retains its invocation")
                    .input(ordinal)
                    .expect("staged input retains its checked type");

                if kind == DependencyRequirementKind::ValueDependencies
                    && !path.is_empty()
                    && let Some((source, path)) = self.inputs.projected_input_value(
                        self.request.semantic_values(), self.selections, argument, path,
                    )
                {
                    // The source expression already has its evaluated snapshot. A copied
                    // component carries that value's dependencies, not the input loan's lifetime.
                    return Ok(self.copied_values(source, path)?.into_iter().collect());
                }

                input.ty(self.request.semantic_values())
            }
            BoundExecutionSite::Iterate(expression) | BoundExecutionSite::Next(expression) => {
                let Some(bray_bound_tree::SemanticSelection::Iteration(iteration)) =
                    self.selections.expression(expression)
                else {
                    unreachable!("implicit iteration value retains its selected protocol");
                };

                if matches!(argument, BoundExecutionSite::Iterate(_)) {
                    iteration.cursor_type()
                } else {
                    self.request
                        .semantic_values()
                        .intern_type(bray_symbols::TypeData::Nullable(iteration.element_type()))
                        .expect("checked iteration retains its nullable element result type")
                }
            }
            BoundExecutionSite::ScopedEnter(expression) => {
                let Some(bray_bound_tree::SemanticSelection::ScopedUse(scoped)) =
                    self.selections.expression(expression)
                else {
                    unreachable!("entered capability retains its selected scope");
                };

                scoped.capability_type()
            }
            _ => panic!("input dependencies require an evaluated value site: {argument:?}"),
        };

        if kind == DependencyRequirementKind::ValueDependencies
            || matches!(
                self.request.semantic_values().type_data(ty).as_ref(),
                bray_symbols::TypeData::Borrow { .. }
            )
        {
            // The implicit result is an immutable evaluated value. Keeping all its
            // requirements is conservative when the requested path selects one field.
            return Ok(self.state.value_requirements(argument).cloned().collect());
        }

        Ok(vec![DependencyRequirement::direct(
            DependencySubject::root(DependencySubjectRoot::EvaluationStorage),
            kind,
        )])
    }
}
