use crate::analysis::storage_flow::memory::Effect;
use bray_symbols::DependencyMutationKind;
use std::borrow::Cow;
use std::collections::BTreeSet;

use super::super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::{Origin, Place, Source, Target};
use crate::analysis::storage_flow::model::{EffectPrefix, StorageFlowState};
use bray_bound_tree::{BoundExecutionSite, BoundExpressionId, SemanticSelection, StorageMutationScope};
use bray_symbols::DependencySubjectRoot;

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(in crate::analysis::storage_flow::check::analysis) fn operation_escapes(
        &mut self,
        state: &StorageFlowState,
        execution: &crate::analysis::guarantee::ExecutionState,
        operation: crate::analysis::model::AnalysisOperationKind,
        (assignment, assignment_effects): (Option<&[Target]>, Option<&[Effect]>),
        (inputs, call_effects): (Option<&[Target]>, Option<&[Effect]>),
        scheduled: bool,
    ) -> (BTreeSet<Target>, EffectPrefix) {
        let mut prefix = EffectPrefix::default();

        let mut escaped = assignment.map(|roots| {
            self.address_escapes(state, execution, operation, assignment_effects, roots, &mut prefix)
        }).unwrap_or_default();

        if let Some(roots) = inputs {
            let mut concurrent = EffectPrefix::default();
            let call_prefix = if scheduled { &mut concurrent } else { &mut prefix };

            escaped.extend(self.address_escapes(state, execution, operation, call_effects, roots, call_prefix));
        }

        (escaped, prefix)
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn address_escapes(
        &mut self,
        state: &StorageFlowState,
        execution: &crate::analysis::guarantee::ExecutionState,
        operation: crate::analysis::model::AnalysisOperationKind,
        effects: Option<&[crate::analysis::storage_flow::memory::Effect]>,
        roots: &[Target],
        prefix: &mut EffectPrefix,
    ) -> BTreeSet<Target> {
        let memory = matches!(operation.occurrence(), BoundExecutionSite::Node(_))
            .then(|| {
                operation
                    .occurrence()
                    .expression()
                    .and_then(|expression| self.memory.operation(expression))
            })
            .flatten();

        if memory.is_some_and(|operation| {
            matches!(
                operation.kind(),
                bray_bound_tree::CheckedMemoryOperationKind::RawDeallocate
                    | bray_bound_tree::CheckedMemoryOperationKind::Deallocate
                    | bray_bound_tree::CheckedMemoryOperationKind::RawBufferSetInitializedCount
                    | bray_bound_tree::CheckedMemoryOperationKind::RawBufferPop { .. }
            )
        }) {
            // These compiler-owned operations retire backing, extract an element,
            // or update its live prefix without publishing an input address.
            return BTreeSet::new();
        }

        let exposed_integer = memory.is_some_and(|operation| {
            matches!(
                operation.kind(),
                bray_bound_tree::CheckedMemoryOperationKind::ExposeAddress { .. }
            )
        });

        if memory.is_some_and(|operation| match operation.kind() {
            bray_bound_tree::CheckedMemoryOperationKind::Copy { pointee, .. } =>
                self.input.footprint.complete_reference_types.get(pointee) == Some(&true),
            bray_bound_tree::CheckedMemoryOperationKind::ByteBufferFill => true,
            _ => false,
        }) {
            // Storing reference-free data publishes neither the source nor the
            // destination pointer. Its destination mutation is modeled separately.
            return BTreeSet::new();
        }

        if memory.is_some_and(|operation| matches!(operation.kind(),
            bray_bound_tree::CheckedMemoryOperationKind::RawBufferRelocate { .. }
                | bray_bound_tree::CheckedMemoryOperationKind::RawBufferReplace { .. }
                | bray_bound_tree::CheckedMemoryOperationKind::RawBufferRelease { .. }
        )) {
            // The owning primitive supplies the stored payload's exact escape roots.
            return self.escape_effects(&state.memory, effects, &[], false, Some(prefix));
        }

        let direct_write = memory.is_some()
            || self
                .storage
                .occurrence_plans(operation.occurrence())
                .any(|plan| plan.purpose() == bray_bound_tree::StorageAccessPurpose::Assignment);

        let may_store = direct_write
            && effects.is_none_or(|effects| {
                effects
                    .iter()
                    .any(|effect| state.memory.may_retain_address(effect))
            });

        if let Some(operation) = memory
            && matches!(
                operation.kind(),
                bray_bound_tree::CheckedMemoryOperationKind::Write { .. }
                    | bray_bound_tree::CheckedMemoryOperationKind::VolatileWrite { .. }
                    | bray_bound_tree::CheckedMemoryOperationKind::UninitWrite { .. }
                    | bray_bound_tree::CheckedMemoryOperationKind::RawBufferPush { .. }
            )
        {
            // Dereferencing the destination does not publish its address. Only
            // references contained in the stored value can escape through this write.
            let value = operation.arguments()[1];
            let mut stored = Vec::new();
            let capture = self.captured_memory_operand(execution, operation, value);

            if let Some(capture) = capture {
                stored.extend(capture.carried_regions());
                stored.extend(capture.carried_sources().cloned());
            }

            self.escape_effects(&state.memory, Some(&[]), &stored, may_store, Some(prefix))
        } else {
            self.escape_effects(
                &state.memory,
                effects,
                roots,
                exposed_integer || may_store,
                Some(prefix),
            )
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn escape_effects(
        &mut self,
        memory: &crate::analysis::storage_flow::memory::State,
        effects: Option<&[crate::analysis::storage_flow::memory::Effect]>,
        roots: &[Target],
        retain_inputs: bool,
        mut prefix: Option<&mut EffectPrefix>,
    ) -> BTreeSet<Target> {

        let all_inputs = retain_inputs
            || effects.is_none_or(|effects| {
                effects
                    .iter()
                    .any(|effect| matches!(effect, Effect::Escape(None)))
            });

        let targets = effects
            .into_iter()
            .flatten()
            .flat_map(Effect::escape_regions)
            .chain(roots.iter().filter(|_| all_inputs))
            .cloned()
            .collect::<Vec<_>>();

        // Cleanup can publish references stored in the disposed value. Its own
        // receiver address remains governed by the terminal receiver contract.
        let cleanup_references = effects.into_iter().flatten().flat_map(|effect| {
            match effect {
                Effect::Mutation { footprint: target, kind: kind @ (DependencyMutationKind::Dispose(_) | DependencyMutationKind::Cleanup(_)), .. } => {
                    Some(matches!(kind, DependencyMutationKind::Cleanup(_)).then_some(target).into_iter().chain(memory.contained_regions(target)))
                }
                _ => None,
            }.into_iter().flatten().cloned()
        });

        let roots = targets.into_iter().chain(cleanup_references).collect::<Vec<_>>();

        // A possible alias inside this body is not an unconditional input escape.
        // Retain the operands so callers resolve aliases using their actual storage.
        for target in memory.escape_roots(roots.iter().cloned()) {
            self.record_target_effect(
                memory,
                &target,
                false,
                DependencyMutationKind::Escape,
                None,
                &[],
                prefix.as_deref_mut(),
            );
        }

        memory.escaped_regions(roots)
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn invocation_roots(
        memory: &crate::analysis::storage_flow::memory::State,
        entry: Option<&crate::ExecutionCallEvidence>,
    ) -> Vec<Target> {
        let mut roots = Vec::new();

        let mut capture = |argument: &crate::ExecutionCondition| {
            let Some(value) = argument.inline_captured_storage() else { return; };

            if let Some(regions) = value.address_regions() {
                roots.extend(regions);
                return;
            }

            roots.extend(value.carried_regions());
            roots.extend(value.carried_sources().cloned());

            // A value with an opaque layout can still carry actual loan support.
            for support in value.dependency_support() {
                for subject in &support.subjects {
                    if let bray_bound_tree::BoundDependencySubject::BorrowCapability(loan) = *subject {
                        roots.extend(memory.borrow_regions(loan).into_iter().flatten());
                    }
                }
            }
        };

        for (_, argument) in entry.into_iter().flat_map(crate::ExecutionCallEvidence::positional_arguments) {
            capture(argument);

            for frame in argument.deferred_values() {
                for (_, input) in frame.producers.values().flat_map(|producer| producer.entry.positional_arguments()) {
                    capture(input);
                }
            }
        }

        roots
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn invocation_place(
        &self,
        occurrence: BoundExecutionSite,
        root: DependencySubjectRoot,
    ) -> Option<Place> {
        if root == DependencySubjectRoot::Result {
            return self.invocation_result(occurrence);
        }

        if matches!(
            occurrence,
            BoundExecutionSite::Iterate(_)
                | BoundExecutionSite::Next(_)
                | BoundExecutionSite::ScopedEnter(_)
                | BoundExecutionSite::ScopedExit(_)
        ) {
            let selection = self
                .execution
                .semantics
                .selections()
                .expression(occurrence.expression()?)?;

            selection.invocation_input_site(occurrence, root)?;

            return self
                .storage
                .implicit_input_plan(occurrence)
                .and_then(|plan| self.storage.logical_path(plan.access()))
                .cloned();
        }

        let expression = occurrence.expression()?;

        if let Some(selection) = self.execution.semantics.selections().expression(expression)
            && selection.stages_inputs()
            && let Some(BoundExecutionSite::CallInput {
                expression,
                ordinal,
            }) = selection.invocation_input_site(occurrence, root)
        {
            let access = self
                .storage
                .invocation_input(expression, ordinal)
                .expect("selected input root retains its staged storage");

            return self.storage.logical_path(access).cloned();
        }

        if let Some(argument) = invocation_argument(
            self.request.view(),
            self.execution.semantics.selections(),
            occurrence,
            root,
        ) {
            return self.argument_storage(argument);
        }

        None
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn invocation_wrapping(
        &self,
        occurrence: BoundExecutionSite,
        root: DependencySubjectRoot,
    ) -> Vec<bray_bound_tree::StorageProjection> {
        let Some(selection) = occurrence.expression().and_then(|expression| {
            self.execution.semantics.selections().expression(expression)
        }) else {
            return Vec::new();
        };

        // Staged inputs already have their formal type, including its nullable wrapper.
        if selection.stages_inputs() {
            return Vec::new();
        }

        let Some(input) = selection
            .invocation_input_ordinal(occurrence, root)
            .and_then(|ordinal| selection.input(ordinal))
        else {
            return Vec::new();
        };

        if !input.conversion().is_some_and(|conversion| {
            matches!(conversion.target(), bray_bound_tree::ConversionTarget::NullablePresent)
        }) {
            return Vec::new();
        }

        self.reference_wrapping(
            input.expression().expect("an adapted input retains its source"),
            input.ty(self.request.semantic_values()),
        )
        .expect("a checked nullable adaptation retains its complete wrapping")
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn invocation_reference_path<'p>(
        &self,
        occurrence: BoundExecutionSite,
        root: DependencySubjectRoot,
        path: &'p bray_bound_tree::StorageReferencePath,
    ) -> Option<Cow<'p, bray_bound_tree::StorageReferencePath>> {
        let wrapping = self.invocation_wrapping(occurrence, root);

        if wrapping.is_empty() {
            return Some(Cow::Borrowed(path));
        }

        // These projections belong to the formal value. They do not name cells
        // in the caller's unwrapped argument, including beneath raw pointers.
        Some(Cow::Owned(path.strip_prefix(&wrapping)?))
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn invocation_logical_path(
        &self,
        occurrence: BoundExecutionSite,
        subject: &bray_symbols::DependencySubject,
    ) -> Option<Place> {
        let mut base = self.invocation_place(occurrence, subject.subject_root())?;

        let (_, path) = self.dependencies.calls().get(&occurrence)?.storage().subject(subject)?;

        let path = self.invocation_reference_path(occurrence, subject.subject_root(), path)?;

        base.projections = base.projections.iter().chain(path.projections()).copied().collect();

        Some(base)
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn record_input_targets(
        &mut self,
        memory: &crate::analysis::storage_flow::memory::State,
        operation: &crate::analysis::model::AnalysisOperation,
        captures: &[bray_bound_tree::StorageValueCapture],
    ) {
        let occurrence = operation.kind().occurrence();

        let Some(selection) = occurrence
            .expression()
            .and_then(|expression| self.execution.semantics.selections().expression(expression))
        else {
            return;
        };

        let mut scope = StorageMutationScope::Invocation;

        for (ordinal, _, ty) in
            selection.invocation_reference_inputs(occurrence, self.request.semantic_values())
        {
            let target = captures.iter().find(|capture| capture.input == ordinal)
                .and_then(|capture| memory.captured_value_target(capture.value.binding()?, ty));

            let mutable = matches!(occurrence, BoundExecutionSite::Next(_))
                || matches!(selection, SemanticSelection::Iteration(iteration)
                    if matches!(occurrence, BoundExecutionSite::Iterate(_)) && iteration.mode() == bray_bound_tree::IterationSourceMode::Mutable)
                || selection.input(ordinal).is_some_and(|input| {
                    matches!(
                        self.request
                            .semantic_values()
                            .type_data(input.ty(self.request.semantic_values()))
                            .as_ref(),
                        bray_symbols::TypeData::Borrow {
                            kind: bray_symbols::BorrowKind::Mutable,
                            ..
                        }
                    )
                });

            if mutable {
                let reached = target.as_ref().map_or(StorageMutationScope::Unbounded, |target| {
                    target.mutation_scope(self.storage, self.request.semantic_values())
                });

                scope = scope.max(reached);
            }

            self.record_state_target(
                (operation.id(), None),
                bray_bound_tree::StorageStateTarget::Input {
                    invocation: occurrence,
                    ordinal,
                },
                target,
            );
        }

        if self.publish
            && self
                .execution
                .semantics
                .selections()
                .invoked_callable(occurrence)
                .is_some()
        {
            self.input_observation_scopes.record(self.block(), occurrence, scope);
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn invocation_reference(
        &self,
        state: &StorageFlowState,
        occurrence: BoundExecutionSite,
        root: DependencySubjectRoot,
    ) -> Option<Source> {
        if matches!(
            occurrence,
            BoundExecutionSite::Iterate(_)
                | BoundExecutionSite::Next(_)
                | BoundExecutionSite::ScopedEnter(_)
                | BoundExecutionSite::ScopedExit(_)
        ) {
            if root == DependencySubjectRoot::Result {
                return self
                    .invocation_result(occurrence)
                    .map(|place| Source::Value(Target::storage(place)));
            }

            let selection = self
                .execution
                .semantics
                .selections()
                .expression(occurrence.expression()?)?;

            selection.invocation_input_site(occurrence, root)?;

            let plan = self
                .storage
                .implicit_input_plan(occurrence)
                .expect("implicit invocation retains its actual input");

            if let Some(capability) = self.storage.borrow_for_plan(plan)
                && state.memory.has_borrow(capability)
            {
                return Some(Source::Borrow(capability));
            }

            return self
                .invocation_place(occurrence, root)
                .map(|place| Source::Value(Target::storage(place)));
        }

        if let Some(place) = self.invocation_place(occurrence, root)
            && matches!(
                self.storage.identity(place.root),
                Some(bray_bound_tree::StorageIdentity::InvocationInput { .. })
            )
        {
            // Completion observes the evaluated input, even after its source slot changes.
            return Some(Source::Value(Target::storage(place)));
        }

        if let Some(argument) = invocation_argument(
            self.request.view(),
            self.execution.semantics.selections(),
            occurrence,
            root,
        ) {
            if let Some(SemanticSelection::Operation(operation)) = occurrence
                .expression()
                .and_then(|expression| self.execution.semantics.selections().expression(expression))
                && let Some((_, input @ crate::dependency::OperationInput::Borrow(_, _))) =
                    crate::dependency::operation_inputs(
                        self.request.view(),
                        occurrence.expression()?,
                        operation,
                    )
                    .into_iter()
                    .find(|(actual, _)| *actual == root)
            {
                let capability = self
                    .storage
                    .borrow_for_plan(
                        input
                            .borrow_plan(self.storage)
                            .expect("borrow input has a plan"),
                    )
                    .expect("checked borrow plan retains its capability");

                return state
                    .memory
                    .has_borrow(capability)
                    .then_some(Source::Borrow(capability));
            }

            let owner = if matches!(occurrence, BoundExecutionSite::ScopedEnter(_)) {
                occurrence.expression()?
            } else {
                argument
            };

            if root == DependencySubjectRoot::Receiver
                && let Some((capability, _)) =
                    self.storage.borrow_capability_entries().find(|(id, loan)| {
                        loan.expression() == Some(owner) && state.memory.has_borrow(*id)
                    })
            {
                return Some(Source::Borrow(capability));
            }

            return self.expression_reference(state, argument);
        }

        self.invocation_place(occurrence, root)
            .map(|place| Source::Value(Target::storage(place)))
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn admitted_reference_sources(
        &self,
        state: &StorageFlowState,
        occurrence: BoundExecutionSite,
        entry: Option<&crate::ExecutionCallEvidence>,
        subject: &bray_symbols::DependencySubject,
    ) -> Option<Vec<Source>> {
        let root = subject.subject_root();

        if root.is_static() || root == DependencySubjectRoot::Result {
            let locations = match self.dependencies.calls().get(&occurrence)?.storage().subject(subject) {
                Some((_, path)) => self.invocation_stored_locations(state, occurrence, subject, path, false),
                None => self.static_location(subject).map(|target| vec![target]),
            }?;

            return Some(locations.into_iter().map(Source::Value).collect());
        }

        let captured = self.execution.invocation_capture(entry?, occurrence, root)?;
        let contracts = self.dependencies.calls().get(&occurrence)?;

        let (_, path) = contracts.storage().subject(subject)?;

        let path = self.invocation_reference_path(occurrence, root, path)?;

        state.memory.captured_reference_sources(self.request.semantic_values(), captured, &path)
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn static_location(
        &self,
        subject: &bray_symbols::DependencySubject,
    ) -> Option<Target> {
        self.static_target(subject, false)
    }

    fn static_target(&self, subject: &bray_symbols::DependencySubject, owned_region: bool) -> Option<Target> {
        let root = subject.subject_root();

        if !root.is_static() { return None; }

        let values = self.request.semantic_values();

        let selection = if let DependencySubjectRoot::StaticInstance(id, _) = root {
            Some(values.static_reference(id))
        } else { None };

        let planned = selection.as_deref().and_then(|selection| self.storage.static_slot(selection));

        let target = planned.map_or_else(|| Target {
            previous: false,
            origin: Origin::Static { root, selection, projections: [].into() },
        }, |slot| Target::storage(Place::root(slot)));

        if subject.projections().iter().any(|projection| {
            *projection == bray_symbols::DependencyProjection::AddressTarget
                || (*projection == bray_symbols::DependencyProjection::OwnedTarget && !owned_region)
        }) {
            return None;
        }

        let projections = crate::storage::storage_projections(
            self.request.semantic_values(), self.dependencies, subject.projections(),
        )?;

        Some(target.project(&projections))
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn static_effect_targets(
        &self,
        subject: &bray_symbols::DependencySubject,
        owned_region: bool,
    ) -> Option<Vec<bray_bound_tree::StorageMutationTarget>> {
        // The checked static owner names its backing region even when this
        // body has no evaluated descriptor for the allocation.
        let target = self.static_target(subject, owned_region)?;

        if matches!(subject.subject_root(), DependencySubjectRoot::StaticInstance(..)) {
            let target = if owned_region { bray_bound_tree::StorageMutationTarget::bounded(target) }
                else { bray_bound_tree::StorageMutationTarget::location(target) };

            return Some(vec![target]);
        }

        let declaration = subject.subject_root().static_declaration(self.request.semantic_values())?;
        let projections = target.projections().to_vec();

        let selected = self.storage.static_slots(declaration).map(|slot|
            Target::storage(Place::root(slot)).project(&projections));

        Some(selected.chain([target]).map(bray_bound_tree::StorageMutationTarget::bounded).collect())
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn invocation_result(
        &self,
        occurrence: BoundExecutionSite,
    ) -> Option<Place> {
        match occurrence {
            BoundExecutionSite::CallInput { expression, ordinal } => {
                let access = self.storage.invocation_input(expression, ordinal)
                    .expect("input completion retains its exact slot");

                self.storage.logical_path(access).cloned()
            }
            site @ (BoundExecutionSite::Iterate(_) | BoundExecutionSite::Next(_) | BoundExecutionSite::ScopedEnter(_)) => {
                let access = self.storage.invocation_result(site)
                    .expect("implicit completion retains its planned result slot");

                Some(self.storage.logical_path(access)
                    .expect("implicit result slot has its logical path").clone())
            }
            BoundExecutionSite::ScopedExit(_) => self.storage.invocation_result(occurrence).map(|access| {
                self.storage.logical_path(access).expect("scoped exit result owns its complete path").clone()
            }),
            BoundExecutionSite::Cleanup(_) => panic!("cleanup suspension has no invocation result"),
            BoundExecutionSite::Node(_) => self.storage.invocation_result(occurrence)
                .map(|access| self.storage.logical_path(access)
                    .expect("explicit invocation result retains its complete path").clone())
                .or_else(|| self.expression_result_storage(occurrence.expression()?)),
        }
    }
}

pub(super) fn invocation_argument(
    view: bray_bound_tree::BoundUnitView<'_>,
    selections: &bray_bound_tree::CheckedSemanticSelections,
    occurrence: BoundExecutionSite,
    root: DependencySubjectRoot,
) -> Option<BoundExpressionId> {
    let selection = selections.expression(occurrence.expression()?)?;

    if selection.stages_inputs() {
        return match selection.invocation_input_site(occurrence, root)? {
            BoundExecutionSite::Node(bray_bound_tree::AnyBoundNodeId::Expression(expression)) => {
                Some(expression)
            }
            _ => None,
        };
    }

    match selection {
        SemanticSelection::Call(call) => {
            crate::dependency::result_input(call, root).and_then(bray_bound_tree::SelectedInput::expression)
        }
        SemanticSelection::Operation(operation) => {
            crate::dependency::operation_inputs(view, occurrence.expression()?, operation)
                .into_iter()
                .find(|(actual, _)| *actual == root)
                .and_then(|(_, input)| input.expression())
        }
        SemanticSelection::ScopedUse(scoped)
            if matches!(occurrence, BoundExecutionSite::ScopedEnter(_))
                && root == DependencySubjectRoot::Receiver =>
        {
            Some(scoped.initializer())
        }
        _ => None,
    }
}
