use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

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

/// Dependencies owned by the normal and abnormal report outputs of one execution phase.
/// An absent output produces no report. An empty present contract produces an independent report.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct BoundCallReports {
    completed: Option<BoundDependencyContract>,
    abnormal: Option<BoundDependencyContract>,
}

impl BoundCallReports {
    /// Creates the checked report outputs for one selected execution phase.
    pub const fn new(
        completed: Option<BoundDependencyContract>,
        abnormal: Option<BoundDependencyContract>,
    ) -> Self {
        Self {
            completed,
            abnormal,
        }
    }

    /// Returns dependencies retained by reports accompanying normal completion.
    pub const fn completed(&self) -> Option<&BoundDependencyContract> {
        self.completed.as_ref()
    }

    /// Returns dependencies retained by reports accompanying panic or cancellation.
    pub const fn abnormal(&self) -> Option<&BoundDependencyContract> {
        self.abnormal.as_ref()
    }

    fn contracts(&self) -> impl Iterator<Item = &BoundDependencyContract> {
        self.completed.iter().chain(self.abnormal.iter())
    }
}

/// An exact input-to-output reference relation of a selected call.
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

    /// The source storage type when the result forms an address instead of forwarding a reference.
    pub const fn addressed_type(&self) -> Option<bray_symbols::TypeId> {
        self.addressed_type
    }

    /// Whether the output retains the input reference value, rather than only its target.
    pub const fn preserves_value(&self) -> bool {
        self.preserves_value
    }
}

/// The distinct invocation, returned-value and deferred requirements of one selected call.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BoundCallDependencyContracts {
    input_count: usize,
    returned_input: Option<bray_symbols::ConstantTermId>,
    returned_input_conditions: Arc<[bray_symbols::ConstantTermId]>,
    carriers: Arc<[bray_symbols::DependencyWitnessCarrier]>,
    invocation: BoundDependencyContract,
    result: BoundDependencyContract,
    deferred: Option<BoundDependencyContract>,
    reports: BoundCallReports,
    deferred_reports: Option<BoundCallReports>,
    references: Vec<BoundCallReference>,
    effects: Arc<[bray_symbols::DependencyStorageEffect]>,
    retained_inputs: Option<BTreeSet<crate::BoundExecutionSite>>,
    reference_paths: BTreeMap<bray_symbols::DependencySubject, crate::StorageReferencePath>,
    normal_completion: bool,
    normal_storage_preserved: bool,
    deferred_effects: Option<Arc<[bray_symbols::DependencyStorageEffect]>>,
    deferred_completion: bool,
    deferred_storage_preserved: bool,
}

impl BoundCallDependencyContracts {
    /// Records an unchanged owned input returned as the complete result on every normal exit.
    /// Normal completion must have no observable storage mutation or cleanup effects.
    /// The term contains only a formal input and owned projections, never an equality or dereference.
    pub fn with_returned_input(mut self, input: Option<bray_symbols::ConstantTermId>) -> Self {
        self.returned_input = input;

        self
    }

    /// Returns the checked whole-value transport, without introducing trusted guarantees.
    pub const fn returned_input(&self) -> Option<bray_symbols::ConstantTermId> {
        self.returned_input
    }

    /// Retains presence facts implied by successfully returning the checked input projection.
    pub fn with_returned_input_conditions(
        mut self,
        conditions: impl IntoIterator<Item = bray_symbols::ConstantTermId>,
    ) -> Self {
        self.returned_input_conditions = conditions.into_iter().collect();

        self
    }

    /// Returns ordinary presence facts prepared from the selected whole-value transport.
    pub fn returned_input_conditions(&self) -> &[bray_symbols::ConstantTermId] {
        &self.returned_input_conditions
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
            returned_input: None,
            returned_input_conditions: Arc::new([]),
            carriers: Arc::new([]),
            input_count: 0,
            deferred,
            reports: BoundCallReports::default(),
            deferred_reports: None,
            references: Vec::new(),
            effects: Arc::new(bray_symbols::DependencyStorageEffect::opaque_call()),
            retained_inputs: None,
            reference_paths: BTreeMap::new(),
            normal_completion: true,
            normal_storage_preserved: false,
            deferred_effects: None,
            deferred_completion: true,
            deferred_storage_preserved: false,
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

    /// Retains the returned carrier paths established by the selected callable.
    pub fn with_carriers(
        mut self,
        carriers: impl IntoIterator<Item = bray_symbols::DependencyWitnessCarrier>,
    ) -> Self {
        self.carriers = carriers.into_iter().collect();

        self
    }

    /// Returns complete owners supporting the callable's trusted completion clauses.
    pub fn carriers(&self) -> &[bray_symbols::DependencyWitnessCarrier] {
        &self.carriers
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

    /// Retains the checked normal-return storage-preservation proof for invocation.
    pub fn with_normal_storage_preservation(mut self, preserved: bool) -> Self {
        self.normal_storage_preserved = preserved;

        self
    }

    /// Whether normal invocation preserves storage, apart from input ownership transfers.
    pub fn preserves_storage_on_normal_return(&self) -> bool {
        self.normal_storage_preserved || self.effects.is_empty()
    }

    /// Retains the same proof for the deferred body of a lazy invocation.
    pub fn with_deferred_storage_preservation(mut self, preserved: bool) -> Self {
        self.deferred_storage_preserved = preserved;

        self
    }

    /// Whether normal deferred execution preserves storage.
    pub fn deferred_preserves_storage_on_normal_return(&self) -> bool {
        self.deferred_storage_preserved
            || self
                .deferred_effects
                .as_ref()
                .is_some_and(|effects| effects.is_empty())
    }

    /// Records the body effects and outcome separately from lazy frame creation.
    pub fn with_deferred_execution(
        mut self,
        effects: impl IntoIterator<Item = bray_symbols::DependencyStorageEffect>,
        normal_completion: bool,
    ) -> Self {
        self.deferred_effects = Some(bray_symbols::DependencyStorageEffect::normalize(effects));
        self.deferred_completion = normal_completion;

        self
    }

    /// Body effects applied when this lazy invocation is driven.
    pub fn deferred_effects(&self) -> Option<&[bray_symbols::DependencyStorageEffect]> {
        self.deferred_effects.as_deref()
    }

    /// Whether driving this invocation can return normally.
    pub const fn deferred_completion(&self) -> bool {
        self.deferred_completion
    }

    /// Shares checked access paths between reference transfers and mutation effects.
    pub fn with_reference_paths(
        mut self,
        paths: BTreeMap<bray_symbols::DependencySubject, crate::StorageReferencePath>,
    ) -> Self {
        self.reference_paths = paths;

        self
    }

    /// Returns the input subjects whose reference paths were prepared by call analysis.
    pub fn reference_subjects(&self) -> impl Iterator<Item = &bray_symbols::DependencySubject> {
        self.reference_paths.keys()
    }

    /// Returns the selected input path prepared by type analysis.
    pub fn reference_path(
        &self,
        subject: &bray_symbols::DependencySubject,
    ) -> Option<&crate::StorageReferencePath> {
        self.reference_paths.get(subject)
    }

    /// Records the dependencies carried by the returned value.
    pub fn with_result(mut self, result: BoundDependencyContract) -> Self {
        self.result = result;

        self
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

    /// Enumerates the owned reports this selected call can produce.
    pub fn report_outputs(
        &self,
    ) -> impl Iterator<Item = (CallReportOutput, &BoundDependencyContract)> {
        [
            (CallReportOutput::Completed, self.reports.completed()),
            (CallReportOutput::Abnormal, self.reports.abnormal()),
            (
                CallReportOutput::DeferredCompleted,
                self.deferred_reports
                    .as_ref()
                    .and_then(BoundCallReports::completed),
            ),
            (
                CallReportOutput::DeferredAbnormal,
                self.deferred_reports
                    .as_ref()
                    .and_then(BoundCallReports::abnormal),
            ),
        ]
        .into_iter()
        .filter_map(|(output, contract)| Some((output, contract?)))
    }

    /// Returns the checked dependency contract for one report output.
    pub fn report(&self, output: CallReportOutput) -> Option<&BoundDependencyContract> {
        match output {
            CallReportOutput::Completed => self.reports.completed(),
            CallReportOutput::Abnormal => self.reports.abnormal(),
            CallReportOutput::DeferredCompleted => self.deferred_reports.as_ref()?.completed(),
            CallReportOutput::DeferredAbnormal => self.deferred_reports.as_ref()?.abnormal(),
        }
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
        self.references = references;
        self.retained_inputs = retained_inputs;

        self
    }

    /// Retains the selected callable's checked formal mutation effects.
    pub fn with_effects(
        mut self,
        effects: impl IntoIterator<Item = bray_symbols::DependencyStorageEffect>,
    ) -> Self {
        self.effects = bray_symbols::DependencyStorageEffect::normalize(effects);

        self
    }

    /// Returns the complete may-mutation summary for this invocation.
    pub fn effects(&self) -> &[bray_symbols::DependencyStorageEffect] {
        &self.effects
    }

    /// Returns exact reference transfers on normal completion.
    pub fn references(&self) -> &[BoundCallReference] {
        &self.references
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

    pub(super) fn phase_contracts(&self) -> impl Iterator<Item = &BoundDependencyContract> {
        [&self.invocation, &self.result]
            .into_iter()
            .chain(self.deferred.as_ref())
            .chain(self.reports.contracts())
            .chain(
                self.deferred_reports
                    .iter()
                    .flat_map(BoundCallReports::contracts),
            )
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
