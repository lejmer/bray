use std::collections::BTreeSet;

use bray_bound_tree::BoundExecutionSite;
use bray_symbols::{DependencyRequirement, DependencyRequirementKind, DependencySubject, DependencySubjectRoot};

use super::core::Transfer;
use crate::{CheckerQueryError, CheckerRequestContext};

impl<C: CheckerRequestContext + ?Sized> Transfer<'_, '_, C> {
    pub(super) fn input_values(
        &self, input: BoundExecutionSite,
    ) -> Result<BTreeSet<DependencyRequirement>, CheckerQueryError<C::UpstreamError>> {
        let site = input;
        let BoundExecutionSite::CallInput { expression, ordinal } = input else {
            panic!("input dependency transfer has an exact input occurrence");
        };
        let selection = self.selections.expression(expression).expect("input dependency transfer retains its invocation");
        let input = selection.input(ordinal).expect("input dependency transfer retains its exact input");
        if let bray_bound_tree::SelectedInput::Receiver(receiver) = input
            && matches!(receiver.mode(), bray_symbols::ReceiverMode::Shared | bray_symbols::ReceiverMode::Mutable)
        {
            return Ok(self.borrowed_values(receiver.expression(), &[]));
        }
        let template = crate::dependency::call::selected_input_template(self.request, selection, ordinal)?;
        self.mapped_result_values(template.requirements(), |root| self.input_argument_site(site, root))
    }

    pub(super) fn input_argument_site(
        &self, input: BoundExecutionSite, root: DependencySubjectRoot,
    ) -> Option<BoundExecutionSite> {
        let BoundExecutionSite::CallInput { expression, .. } = input else {
            panic!("input mapping has an exact input occurrence");
        };
        let selection = self.selections.expression(expression).expect("input mapping retains its invocation");
        selection.invocation_input_site(input, root)
    }

    pub(super) fn mapped_site_values(
        &self,
        argument: BoundExecutionSite,
        path: &[bray_symbols::DependencyProjection],
        kind: DependencyRequirementKind,
    ) -> Result<Vec<DependencyRequirement>, bray_symbols::SemanticValueStoreError> {
        let ty = match argument {
            BoundExecutionSite::Node(bray_bound_tree::AnyBoundNodeId::Expression(expression)) =>
                return self.mapped_input_values(expression, path, kind),
            BoundExecutionSite::CallInput { expression, ordinal } => self.selections.expression(expression)
                .expect("staged input retains its invocation").input(ordinal)
                .expect("staged input retains its checked type").ty(self.request.semantic_values()),
            BoundExecutionSite::Iterate(expression) | BoundExecutionSite::Next(expression) => {
                let Some(bray_bound_tree::SemanticSelection::Iteration(iteration)) = self.selections.expression(expression) else {
                    unreachable!("implicit iteration value retains its selected protocol");
                };
                if matches!(argument, BoundExecutionSite::Iterate(_)) { iteration.cursor_type() }
                else { self.request.semantic_values().intern_type(bray_symbols::TypeData::Nullable(iteration.element_type()))? }
            }
            BoundExecutionSite::ScopedEnter(expression) => {
                let Some(bray_bound_tree::SemanticSelection::ScopedUse(scoped)) = self.selections.expression(expression) else {
                    unreachable!("entered capability retains its selected scope");
                };
                scoped.capability_type()
            }
            _ => panic!("input dependencies require an evaluated value site: {argument:?}"),
        };
        if kind == DependencyRequirementKind::ValueDependencies
            || matches!(self.request.semantic_values().type_data(ty).as_ref(), bray_symbols::TypeData::Borrow { .. })
        {
            // The implicit result is an immutable evaluated value. Keeping all its
            // requirements is conservative when the requested path selects one field.
            return Ok(self.state.value_requirements(argument).cloned().collect());
        }
        Ok(vec![DependencyRequirement::direct(
            DependencySubject::root(DependencySubjectRoot::EvaluationStorage), kind)])
    }
}
