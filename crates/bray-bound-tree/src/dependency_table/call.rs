use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use bray_base::sorted_unique_shared_slice;

use crate::{BoundDependencyContract, BoundUnitId};

/// One owned report output of immediate or deferred execution.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CallReportOutput {
    /// A report accompanying the immediate value.
    Completed,
    /// A report accompanying immediate panic or cancellation.
    Abnormal,
    /// A report accompanying the lazy body's eventual value.
    DeferredCompleted,
    /// A report accompanying the lazy body's panic or cancellation.
    DeferredAbnormal,
}

impl CallReportOutput {
    /// Whether this output accompanies panic or cancellation.
    pub const fn is_abnormal(self) -> bool {
        match self {
            Self::Completed | Self::DeferredCompleted => false,
            Self::Abnormal | Self::DeferredAbnormal => true,
        }
    }
}

/// The portable equations and checked storage obligations of one produced report.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BoundCallReport {
    requirements: Arc<[bray_symbols::DependencyRequirement]>,
    storage: BoundDependencyContract,
}

impl BoundCallReport {
    /// Retains the producer's equations alongside their instantiated storage obligations.
    pub const fn new(requirements: Arc<[bray_symbols::DependencyRequirement]>, storage: BoundDependencyContract) -> Self {
        Self { requirements, storage }
    }

    /// The producer's portable report lifetime requirements.
    pub fn requirements(&self) -> &[bray_symbols::DependencyRequirement] {
        &self.requirements
    }

    /// The checked obligations in the invocation's storage plan.
    pub const fn storage(&self) -> &BoundDependencyContract {
        &self.storage
    }
}

/// Report dependencies of one execution phase. An empty present contract produces an independent report.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct BoundCallReports {
    completed: Arc<[Option<BoundCallReport>]>,
    abnormal: Option<BoundCallReport>,
}

impl BoundCallReports {
    /// Creates the report contracts in the same order as the normal outcome rows.
    pub const fn new(completed: Arc<[Option<BoundCallReport>]>, abnormal: Option<BoundCallReport>) -> Self {
        Self { completed, abnormal }
    }

    /// Enumerates completed report contracts with their normal outcome index.
    pub fn completed(&self) -> impl Iterator<Item = (usize, &BoundCallReport)> {
        self.completed.iter().enumerate().filter_map(|(index, report)| Some((index, report.as_ref()?)))
    }

    /// Returns dependencies retained by reports accompanying panic or cancellation.
    pub const fn abnormal(&self) -> Option<&BoundCallReport> {
        self.abnormal.as_ref()
    }

    /// Enumerates all possible report obligations at this phase.
    fn contracts(&self) -> impl Iterator<Item = &BoundDependencyContract> {
        self.completed().map(|(_, report)| report.storage()).chain(self.abnormal().map(BoundCallReport::storage))
    }
}

/// A checked input-to-output reference relation of a selected call.
/// Address formation locates storage without equating a pointer with the borrowed value.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BoundCallReference {
    relation: bray_symbols::DependencyBorrowRelation,
    preserves_value: bool,
    addressed_type: Option<bray_symbols::TypeId>,
}

impl BoundCallReference {
    /// Records whether the reference forwards an input value or forms an address.
    pub const fn new(
        relation: bray_symbols::DependencyBorrowRelation,
        preserves_value: bool,
        addressed_type: Option<bray_symbols::TypeId>,
    ) -> Self {
        Self {
            relation,
            preserves_value,
            addressed_type,
        }
    }

    /// Returns the checked formal reference relation.
    pub const fn relation(&self) -> &bray_symbols::DependencyBorrowRelation {
        &self.relation
    }

    /// The reached storage type when forming an address or a bounded reference.
    pub const fn addressed_type(&self) -> Option<bray_symbols::TypeId> {
        self.addressed_type
    }

    /// Whether the output retains the input reference value, rather than only its target.
    pub const fn preserves_value(&self) -> bool {
        self.preserves_value && self.relation.is_exact()
    }
}

/// Prepared input route and completion conditions of one exact owning transfer.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BoundReturnedValue {
    root: bray_symbols::DependencySubjectRoot,
    borrowed: bool,
    input_path: crate::StorageReferencePath,
}

impl BoundReturnedValue {
    /// Creates the checked description independently of whether its transfer is selected.
    pub fn new(
        root: bray_symbols::DependencySubjectRoot,
        borrowed: bool,
        input_path: crate::StorageReferencePath,
    ) -> Self {
        Self { root, borrowed, input_path }
    }

    /// The receiver or formal argument supplying this value.
    pub const fn input_root(&self) -> bray_symbols::DependencySubjectRoot { self.root }

    /// Whether the instantiated output is a borrow rather than an extracted owner.
    pub const fn is_borrowed(&self) -> bool { self.borrowed }

    /// The current physical source, including borrowed layers crossed by extraction.
    pub fn input_path(&self) -> &crate::StorageReferencePath { &self.input_path }

}

/// Declaration metadata needed to execute a frame outside its forming body.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BoundDeferredCallable {
    /// The selected body's specialized summary in the current semantic store.
    pub summary: bray_symbols::DependencyContractTemplateId,
    /// Selected runtime behavior, including task observation.
    pub hook: Option<bray_compiler_known::ImplementationHook>,
}

/// The distinct invocation, returned-value and deferred requirements of one selected call.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BoundCallDependencyContracts {
    synchronous_cleanup: Arc<[bray_symbols::DependencyRequirement]>,
    input_count: usize,
    captured_values: Arc<[bray_symbols::CapturedValueOutcome]>,
    returned_values: BTreeMap<bray_symbols::ConstantTermId,
        BTreeMap<Arc<[bray_symbols::ConstantProjectionKind]>, BoundReturnedValue>>,
    invocation: BoundDependencyContract,
    result: BoundDependencyContract,
    deferred_result: Option<BoundDependencyContract>,
    deferred: Option<BoundDependencyContract>,
    reports: BoundCallReports,
    deferred_reports: Option<BoundCallReports>,
    reference_types: BTreeMap<bray_symbols::DependencyBorrowRelation, (bool, Option<bray_symbols::TypeId>)>,
    stored_value_paths: BTreeMap<bray_symbols::DependencyStoredValuePath,
        (bray_symbols::TypeId, crate::StorageReferencePath)>,
    effects: bray_symbols::DependencyEffectOutcomes<bray_symbols::DependencyStorageEffect>,
    retained_inputs: Option<BTreeSet<crate::BoundExecutionSite>>,
    reference_subjects: BTreeMap<
        bray_symbols::DependencySubject,
        (bray_symbols::TypeId, crate::StorageReferencePath),
    >,
    normal_completion: bool,
    deferred_effects: Option<bray_symbols::DependencyEffectOutcomes<bray_symbols::DependencyStorageEffect>>,
    deferred_callee: Option<Arc<bray_symbols::CapturedCallable>>,
    deferred_completion: bool,
    deferred_cleanup: Option<(Arc<[bray_symbols::DependencyStorageEffect]>, Arc<[bray_symbols::DependencyStorageEffect]>)>,
}

impl BoundCallDependencyContracts {
    /// Retains the checked portable expression for the actual normal value.
    pub fn with_captured_values(mut self, values: impl IntoIterator<Item = bray_symbols::CapturedValueOutcome>) -> Self {
        self.captured_values = sorted_unique_shared_slice(values);

        self
    }

    /// Returns the actual value expression bound by this invocation.
    pub fn captured_values(&self) -> &[bray_symbols::CapturedValueOutcome] { &self.captured_values }

    /// Derives the checked ownership layout before selecting a normal exit.
    pub fn owned_result(&self) -> bray_symbols::DependencyOwnedResult {
        bray_symbols::CapturedValueOutcome::ownership_layout(&self.captured_values)
    }

    /// Retains the checked ordinary shape and conditions of each possible input transfer.
    pub fn with_returned_values(
        mut self,
        values: impl IntoIterator<Item = (bray_symbols::DependencyOwnedTransfer, BoundReturnedValue)>,
    ) -> Self {
        for (transfer, route) in values {
            self.returned_values.entry(transfer.input()).or_default()
                .insert(transfer.output().into(), route);
        }

        self
    }

    /// Returns the checked input route and conditions for one selected transfer.
    pub fn returned_value(
        &self,
        transfer: &bray_symbols::DependencyOwnedTransfer,
    ) -> &BoundReturnedValue {
        self.returned_values.get(&transfer.input()).and_then(|paths| paths.get(transfer.output()))
            .expect("selected owned transfer retains its checked value description")
    }

    /// Creates the instantiated phase contracts for a selected call.
    pub fn new(
        invocation: BoundDependencyContract,
        result: BoundDependencyContract,
        deferred: Option<BoundDependencyContract>,
    ) -> Self {
        Self {
            invocation,
            result,
            deferred_result: None,
            captured_values: Arc::new([]),
            returned_values: BTreeMap::new(),
            input_count: 0,
            synchronous_cleanup: Arc::new([]),
            deferred,
            reports: BoundCallReports::default(),
            deferred_reports: None,
            reference_types: BTreeMap::new(),
            stored_value_paths: BTreeMap::new(),
            effects: bray_symbols::DependencyEffectOutcomes::new(bray_symbols::DependencyStorageEffect::opaque_call()),
            retained_inputs: None,
            reference_subjects: BTreeMap::new(),
            normal_completion: true,
            deferred_effects: None,
            deferred_callee: None,
            deferred_completion: true,
            deferred_cleanup: None,
        }
    }

    /// Retains receiver-first formal arity for portable completion conditions.
    pub const fn with_input_count(mut self, count: usize) -> Self {
        self.input_count = count;

        self
    }

    /// Returns the ordinal reserved for the completion result.
    pub const fn input_count(&self) -> usize {
        self.input_count
    }

    /// Retains checked endpoint routes for the stored values in the normal outcomes.
    pub fn with_stored_value_paths(
        mut self,
        paths: BTreeMap<bray_symbols::DependencyStoredValuePath,
            (bray_symbols::TypeId, crate::StorageReferencePath)>,
    ) -> Self {
        self.stored_value_paths = paths;

        self
    }

    /// Returns the actual type and route of a stored-value endpoint, including terminal loads.
    pub fn stored_value_path(
        &self,
        endpoint: &bray_symbols::DependencyStoredValuePath,
    ) -> Option<(bray_symbols::TypeId, &crate::StorageReferencePath)> {
        self.stored_value_paths.get(endpoint).map(|(ty, path)| (*ty, path))
    }

    /// Records whether this invocation phase can produce a normal result.
    pub fn with_normal_completion(mut self, possible: bool) -> Self {
        self.normal_completion = possible;

        self
    }

    /// Whether normal completion is possible for this selected invocation phase.
    pub const fn normal_completion(&self) -> bool {
        self.normal_completion
    }

    /// Retains the normal transitions of the selected invocation.
    pub fn with_normal_effects(mut self, effects: impl IntoIterator<Item = bray_symbols::DependencyStorageEffect>) -> Self {
        self.effects = self.effects.with_normal(effects);

        self
    }

    /// Whether normal invocation preserves storage, apart from input ownership transfers.
    pub fn preserves_storage_on_normal_return(&self) -> bool {
        self.effects.preserves_storage_on_normal_return()
    }

    /// Whether normal deferred execution preserves storage.
    pub fn deferred_preserves_storage_on_normal_return(&self) -> bool {
        self.deferred_effects.as_ref().is_some_and(bray_symbols::DependencyEffectOutcomes::preserves_storage_on_normal_return)
    }

    /// Records the body effects and outcome separately from lazy frame creation.
    pub fn with_deferred_execution(
        mut self,
        effects: bray_symbols::DependencyEffectOutcomes<bray_symbols::DependencyStorageEffect>,
        normal_completion: bool,
        callee: Option<bray_symbols::CapturedCallable>,
    ) -> Self {
        self.deferred_callee = callee.map(Arc::new);
        self.deferred_effects = Some(effects);
        self.deferred_completion = normal_completion;

        self
    }

    /// Selected dispatch and witnesses retained by the formed value.
    pub fn deferred_callee(&self) -> Option<&Arc<bray_symbols::CapturedCallable>> { self.deferred_callee.as_ref() }

    /// Body effects applied when this lazy invocation is driven.
    pub fn deferred_effects(&self) -> Option<&[bray_symbols::DependencyStorageEffect]> {
        self.deferred_effects.as_ref().map(bray_symbols::DependencyEffectOutcomes::possible)
    }

    /// Effects of normal deferred completion, captured with the latent body.
    pub fn deferred_effect_outcomes(&self) -> Option<&bray_symbols::DependencyEffectOutcomes<bray_symbols::DependencyStorageEffect>> { self.deferred_effects.as_ref() }

    /// Retains cleanup contracts for captured values and the discarded completion result.
    pub fn with_deferred_cleanup(
        mut self,
        captures: impl IntoIterator<Item = bray_symbols::DependencyStorageEffect>,
        result: impl IntoIterator<Item = bray_symbols::DependencyStorageEffect>,
    ) -> Self {
        self.deferred_cleanup = Some((bray_symbols::DependencyStorageEffect::normalize(captures), bray_symbols::DependencyStorageEffect::normalize(result)));

        self
    }

    /// Cleanup that can execute for values captured by this lazy invocation.
    pub fn deferred_capture_cleanup(&self) -> Option<&[bray_symbols::DependencyStorageEffect]> {
        self.deferred_cleanup.as_ref().map(|(captures, _)| captures.as_ref())
    }

    /// Additional cleanup when this lazy invocation's completion result is discarded.
    pub fn deferred_result_cleanup(&self) -> Option<&[bray_symbols::DependencyStorageEffect]> {
        self.deferred_cleanup.as_ref().map(|(_, result)| result.as_ref())
    }

    /// Whether driving this invocation can return normally.
    pub const fn deferred_completion(&self) -> bool {
        self.deferred_completion
    }

    /// Shares checked reference types and paths between transfers and mutation effects.
    pub fn with_reference_subjects(
        mut self,
        inputs: BTreeMap<
            bray_symbols::DependencySubject,
            (bray_symbols::TypeId, crate::StorageReferencePath),
        >,
    ) -> Self {
        self.reference_subjects = inputs;

        self
    }

    /// Returns the input and result subjects prepared by call analysis.
    pub fn reference_subjects(&self) -> impl Iterator<Item = &bray_symbols::DependencySubject> {
        self.reference_subjects.keys()
    }

    /// Returns the selected input path prepared by type analysis.
    pub fn reference_path(
        &self,
        subject: &bray_symbols::DependencySubject,
    ) -> Option<&crate::StorageReferencePath> {
        self.reference_subjects.get(subject).map(|(_, path)| path)
    }

    /// Returns the checked value type at one selected reference subject.
    pub fn reference_type(
        &self,
        subject: &bray_symbols::DependencySubject,
    ) -> Option<bray_symbols::TypeId> {
        self.reference_subjects.get(subject).map(|(ty, _)| *ty)
    }

    /// Records the dependencies carried by the returned value.
    pub fn with_result(mut self, result: BoundDependencyContract) -> Self {
        self.result = result;

        self
    }

    /// Records dependencies of the value produced by normal completion of a lazy frame.
    pub fn with_deferred_result(mut self, result: BoundDependencyContract) -> Self {
        self.deferred_result = Some(result);

        self
    }

    /// Returns the completed value's dependencies independently of the frame captures.
    pub const fn deferred_result(&self) -> Option<&BoundDependencyContract> {
        self.deferred_result.as_ref()
    }

    /// Attaches report ownership for argument evaluation and immediate execution.
    pub fn with_reports(mut self, reports: BoundCallReports) -> Self {
        self.reports = reports;

        self
    }

    /// Attaches report ownership for execution of a lazy frame.
    pub fn with_deferred_reports(mut self, reports: BoundCallReports) -> Self {
        self.deferred_reports = Some(reports);

        self
    }

    /// Enumerates the checked obligations of all possible reports.
    pub fn report_contracts(&self) -> impl Iterator<Item = &BoundDependencyContract> {
        self.reports.contracts().chain(self.deferred_reports.iter().flat_map(BoundCallReports::contracts))
    }

    /// Returns report outputs from evaluating this invocation.
    pub const fn reports(&self) -> &BoundCallReports {
        &self.reports
    }

    /// Returns report outputs from driving the returned lazy frame.
    pub const fn deferred_reports(&self) -> Option<&BoundCallReports> {
        self.deferred_reports.as_ref()
    }

    /// Records reference transfers and the inputs retained by a returned witness.
    pub fn with_references(
        mut self,
        references: Vec<BoundCallReference>,
        retained_inputs: Option<BTreeSet<crate::BoundExecutionSite>>,
    ) -> Self {
        self.reference_types = references.into_iter().map(|reference| (
            reference.relation.with_exactness(false),
            (reference.preserves_value, reference.addressed_type),
        )).collect();

        self.retained_inputs = retained_inputs;

        self
    }

    /// Retains synchronous cleanup obligations independently of the caller's execution mode.
    pub fn with_synchronous_cleanup(
        mut self,
        requirements: &[bray_symbols::DependencyRequirement],
    ) -> Self {
        self.synchronous_cleanup = Arc::from(requirements);

        self
    }

    /// Returns instantiated cleanup obligations of the selected body and its callees.
    pub fn synchronous_cleanup(&self) -> &[bray_symbols::DependencyRequirement] {
        &self.synchronous_cleanup
    }

    /// Retains the selected callable's checked formal mutation effects.
    pub fn with_effects(
        mut self,
        effects: impl IntoIterator<Item = bray_symbols::DependencyStorageEffect>,
    ) -> Self {
        self.effects = bray_symbols::DependencyEffectOutcomes::new(bray_symbols::DependencyStorageEffect::normalize(effects).iter().cloned());

        self
    }

    /// Returns the complete may-mutation summary for this invocation.
    pub fn effects(&self) -> &[bray_symbols::DependencyStorageEffect] {
        self.effects.possible()
    }

    /// Returns transitions that may occur on normal invocation completion.
    pub fn normal_effects(&self) -> &[bray_symbols::DependencyStorageEffect] { self.effects.normal() }

    /// Returns the connected effect contract for this invocation.
    pub const fn effect_outcomes(&self) -> &bray_symbols::DependencyEffectOutcomes<bray_symbols::DependencyStorageEffect> { &self.effects }

    /// Joins the portable reference origins before admission.
    pub fn references(&self) -> Vec<BoundCallReference> {
        self.reference_outputs(self.captured_values.iter())
    }

    /// Binds the typed reference writes established by the admitted outcome rows.
    pub fn reference_outputs<'a>(&self, outcomes: impl IntoIterator<Item = &'a bray_symbols::CapturedValueOutcome>) -> Vec<BoundCallReference> {
        let relations = bray_symbols::CapturedValueOutcome::reference_origins(outcomes, bray_symbols::DependencySubject::contains);
        let mut unavailable = BTreeSet::new();
        let mut references = Vec::with_capacity(relations.len());

        for relation in relations.iter().cloned() {
            if let Some(&(forwards_value, addressed_type)) = self.reference_types.get(&relation.clone().with_exactness(false)) {
                references.push(BoundCallReference::new(relation, forwards_value, addressed_type));
            } else {
                unavailable.insert(relation.exit().clone());
            }
        }

        references.retain(|reference| !unavailable.contains(reference.relation().exit()));

        references
    }

    /// Returns retained witness inputs when the returned value has a checked dependency source.
    pub const fn retained_inputs(&self) -> Option<&BTreeSet<crate::BoundExecutionSite>> {
        self.retained_inputs.as_ref()
    }

    /// Returns the requirements for beginning the invocation.
    pub const fn invocation(&self) -> &BoundDependencyContract {
        &self.invocation
    }

    /// Returns the dependencies retained by the result after completion.
    pub const fn result(&self) -> &BoundDependencyContract {
        &self.result
    }

    /// Returns the requirements for driving deferred execution, when present.
    pub const fn deferred(&self) -> Option<&BoundDependencyContract> {
        self.deferred.as_ref()
    }

    /// Returns every checked value, invocation, deferred and report contract.
    pub fn phase_contracts(&self) -> impl Iterator<Item = &BoundDependencyContract> {
        [&self.invocation, &self.result]
            .into_iter()
            .chain(self.deferred.as_ref())
            .chain(self.deferred_result.as_ref())
            .chain(self.report_contracts())
    }

    pub(super) fn is_valid_for(&self, unit: BoundUnitId) -> bool {
        self.phase_contracts()
            .all(|contract| contract.is_valid_for(unit))
            && self
                .retained_inputs
                .iter()
                .flatten()
                .all(|input| input.node().unit() == unit)
    }
}
