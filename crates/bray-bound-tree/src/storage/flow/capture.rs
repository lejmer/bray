use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::{AnalysisOperationId, BoundExecutionSite, StorageCompletion, StorageDeferredProducer, StorageInvalidation, StorageMutation, StorageTarget, StorageTaskOutcome, StorageValueCapture, StorageValueBoundary, StorageValueRetirement, StorageValueTransition};
use super::model::StorageFlow;

/// The admitted source rows covered by one reached operation.
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageOutcomeRows {
    /// The selected frame producer. A widened attempt can cover every producer.
    pub producer: Option<StorageDeferredProducer>,
    /// The source rows covered by the attempt. Missing rows cover any outcome.
    pub rows: Option<Arc<[usize]>>,
    /// The terminal result reached at a running-task observation.
    pub task: Option<StorageTaskOutcome>,
}

static UNRESTRICTED_ROWS: StorageOutcomeRows = StorageOutcomeRows { producer: None, rows: None, task: None };

impl StorageOutcomeRows {
    fn matches_context(&self, selection: &Self) -> bool {
        self.producer.as_ref().zip(selection.producer.as_ref()).is_none_or(|(left, right)| left == right)
            && self.task.zip(selection.task).is_none_or(|(left, right)| left.overlaps(right))
    }

    fn includes_row(&self, row: &usize) -> bool {
        self.rows.as_deref().is_none_or(|rows| rows.binary_search(row).is_ok())
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) enum OperationOutcomes<T> {
    Single {
        selection: Option<Box<StorageOutcomeRows>>,
        frame: T,
    },
    Multiple {
        combined: T,
        guarded: Vec<(StorageOutcomeRows, T)>,
    },
}

impl StorageFlow {
    /// Publishes reached entry and attempt values independently of normal completion.
    pub fn with_call_admissions(
        mut self,
        captures: BTreeMap<(AnalysisOperationId, Option<StorageDeferredProducer>), Arc<[StorageValueCapture]>>,
        attempts: BTreeMap<(AnalysisOperationId, Option<StorageDeferredProducer>), Arc<[StorageValueTransition]>>,
    ) -> Self {
        assert!(captures.keys().chain(attempts.keys()).all(|(operation, _)| operation.unit() == self.unit()));

        self.input_captures = captures;
        self.attempt_values = attempts;

        self
    }

    /// Returns the actual evaluated inputs recorded at the reached input boundary.
    pub fn input_captures(
        &self, operation: AnalysisOperationId, producer: Option<StorageDeferredProducer>,
    ) -> Option<&[StorageValueCapture]> {
        self.input_captures.get(&(operation, producer)).map(Arc::as_ref)
    }

    /// Iterates the captured inputs of reached operations in graph order.
    pub fn input_admissions(&self) -> impl Iterator<Item = (AnalysisOperationId, Option<&StorageDeferredProducer>, &[StorageValueCapture])> + '_ {
        self.input_captures.iter().map(|((operation, producer), captures)| (*operation, producer.as_ref(), captures.as_ref()))
    }

    /// Returns the guarded transitions admitted at this operation for the selected producer.
    pub fn attempt_values(
        &self,
        operation: AnalysisOperationId,
        producer: Option<StorageDeferredProducer>,
    ) -> Option<&[StorageValueTransition]> {
        self.attempt_values.get(&(operation, producer)).map(Arc::as_ref)
    }

    /// Publishes observation frames by exact CFG operation and admitted rows.
    pub fn with_operation_invalidations(
        mut self,
        frames: BTreeMap<(AnalysisOperationId, StorageOutcomeRows), StorageInvalidation>,
    ) -> Self {
        let unit = self.unit();

        assert!(frames.iter().all(|((point, _), frame)| point.unit() == unit && frame.is_valid_for(unit)));

        self.operation_invalidations = OperationOutcomes::group(frames, StorageInvalidation::merge);

        self
    }

    /// Iterates the combined normal observation frames in CFG operation order.
    pub fn operation_invalidations(&self) -> impl Iterator<Item = (AnalysisOperationId, &StorageInvalidation)> {
        self.operation_invalidations.iter().map(|(&operation, reached)| (operation, reached.combined()))
    }

    /// Reads the conservative observation frame of one reached operation.
    pub fn operation_invalidation(&self, operation: AnalysisOperationId) -> Option<&StorageInvalidation> {
        self.operation_invalidations.get(&operation).map(OperationOutcomes::combined)
    }

    /// Uses guarded frames when they cover the admitted rows of this operation.
    pub fn selected_operation_invalidation(
        &self,
        operation: AnalysisOperationId,
        selection: Option<&StorageOutcomeRows>,
    ) -> Option<Cow<'_, StorageInvalidation>> {
        let reached = self.operation_invalidations.get(&operation)?;

        let selected = reached.select(selection, true, StorageInvalidation::merge);

        if selection.is_some_and(|selection| selection.task.is_some()) {
            selected
        } else {
            Some(selected.unwrap_or_else(|| Cow::Borrowed(reached.combined())))
        }
    }

    /// Iterates reached running-task observations at this source site's normal completions.
    pub fn task_outcome_mutations(
        &self,
        occurrence: BoundExecutionSite,
    ) -> impl Iterator<Item = (StorageTaskOutcome, &StorageInvalidation)> {
        self.normal_completion_sites.get(&occurrence).into_iter()
            .flat_map(|operations| operations.iter())
            .filter_map(|operation| self.operation_invalidations.get(operation))
            .flat_map(OperationOutcomes::guarded)
            .filter_map(|(guard, invalidation)| guard.task.map(|task| (task, invalidation)))
    }

    /// Publishes reached normal results and their surviving storage by operation and outcome.
    pub fn with_operation_completions(
        mut self,
        frames: BTreeMap<(AnalysisOperationId, StorageOutcomeRows), StorageCompletion>,
        occurrence: impl Fn(AnalysisOperationId) -> BoundExecutionSite,
    ) -> Self {
        let completions = OperationOutcomes::group(frames, StorageCompletion::merge);
        let mut sites = BTreeMap::<_, Vec<_>>::new();

        for &operation in completions.keys() {
            let site = occurrence(operation);

            assert!(operation.unit() == self.unit() && site.node().unit() == self.unit());

            sites.entry(site).or_default().push(operation);
        }

        self.normal_completions = completions;
        self.normal_completion_sites = sites.into_iter().map(|(site, operations)| (site, operations.into())).collect();

        self
    }

    /// Reads the reached normal completion covered by the operation's admission.
    pub fn operation_completion(
        &self,
        operation: AnalysisOperationId,
        selection: Option<&StorageOutcomeRows>,
    ) -> Option<Cow<'_, StorageCompletion>> {
        let reached = self.normal_completions.get(&operation)?;

        reached.select(selection, true, StorageCompletion::merge)
    }

    /// Returns whether any CFG operation completes normally at this source site.
    pub fn invocation_completes(&self, site: BoundExecutionSite) -> bool {
        self.normal_completion_sites.contains_key(&site)
    }

    /// Reads facts agreed by every normal completion of this source site.
    pub fn normal_completion(&self, site: BoundExecutionSite) -> Option<Cow<'_, StorageCompletion>> {
        let operations = self.normal_completion_sites.get(&site)?;

        let (first, remaining) = operations.split_first().expect("a completing site owns at least one reached operation");

        let mut completion = Cow::Borrowed(self.normal_completions[first].combined());

        for operation in remaining {
            completion.to_mut().merge(self.normal_completions[operation].combined());
        }

        Some(completion)
    }

    /// Retains capture retirements of cleanup actions and abnormal edges.
    pub fn with_capture_retirements(mut self, retirements: BTreeMap<(StorageMutation, StorageValueBoundary), BTreeSet<(StorageTarget, StorageValueRetirement)>>) -> Self {
        assert!(retirements.keys().all(|(mutation, _)| !matches!(mutation, StorageMutation::Operation(_) | StorageMutation::TaskObservation { .. })));
        self.capture_retirements = retirements.into_iter().map(|(point, values)| (point, values.into_iter().collect())).collect();

        self
    }

    /// Returns capture retirements at the outcome's checked boundary.
    pub fn capture_retirements(&self, mutation: StorageMutation, boundary: StorageValueBoundary) -> &[(StorageTarget, StorageValueRetirement)] {
        self.capture_retirements.get(&(mutation, boundary)).map_or(&[], Arc::as_ref)
    }

    /// Publishes the ordinary retirements reached at each CFG operation and boundary.
    pub fn with_operation_retirements(mut self, retirements: BTreeMap<((AnalysisOperationId, StorageValueBoundary), StorageOutcomeRows), BTreeSet<(StorageTarget, StorageValueRetirement)>>) -> Self {
        assert!(retirements.keys().all(|((operation, _), _)| operation.unit() == self.unit()));

        let retirements = retirements.into_iter().map(|(point, values)| (point, values.into_iter().collect())).collect();

        self.operation_retirements = OperationOutcomes::group(retirements, |left, right| merge_retirements(left, right));

        self
    }

    /// Reads the possible capture retirements covered by the operation's admission.
    pub fn operation_retirements(
        &self,
        operation: AnalysisOperationId,
        boundary: StorageValueBoundary,
        selection: Option<&StorageOutcomeRows>,
    ) -> Cow<'_, [(StorageTarget, StorageValueRetirement)]> {
        let Some(retirements) = self.operation_retirements.get(&(operation, boundary)) else {
            return Cow::Borrowed(&[]);
        };

        match retirements.select(selection, false, |left, right| merge_retirements(left, right)) {
            Some(Cow::Borrowed(values)) => Cow::Borrowed(values),
            Some(Cow::Owned(values)) => Cow::Owned(values),
            None => Cow::Borrowed(&[]),
        }
    }
}

fn merge_retirements(left: &mut Vec<(StorageTarget, StorageValueRetirement)>, right: &[(StorageTarget, StorageValueRetirement)]) {
    left.extend(right.iter().cloned());
    left.sort_unstable();
    left.dedup();
}

impl<T: Clone> OperationOutcomes<T> {
    pub(super) fn combined(&self) -> &T {
        match self {
            Self::Single { frame, .. } | Self::Multiple { combined: frame, .. } => frame,
        }
    }

    fn guarded(&self) -> impl Clone + Iterator<Item = (&StorageOutcomeRows, &T)> {
        let (single, guarded) = match self {
            Self::Single { selection, frame } =>
                (Some((selection.as_deref().unwrap_or(&UNRESTRICTED_ROWS), frame)), &[][..]),
            Self::Multiple { guarded, .. } => (None, guarded.as_slice()),
        };

        single.into_iter().chain(guarded.iter().map(|(selection, frame)| (selection, frame)))
    }

    pub(super) fn group<K: Copy + Ord>(
        frames: BTreeMap<(K, StorageOutcomeRows), T>,
        merge: impl Fn(&mut T, &T),
    ) -> BTreeMap<K, Self> {
        let mut frames = frames.into_iter().peekable();
        let mut operations = BTreeMap::new();

        while let Some(((operation, rows), frame)) = frames.next() {
            if frames.peek().is_none_or(|((next, _), _)| *next != operation) {
                let selection = (rows != UNRESTRICTED_ROWS).then(|| Box::new(rows));

                operations.insert(operation, Self::Single { selection, frame });

                continue;
            }

            let mut combined = frame.clone();
            let mut guarded = vec![(rows, frame)];

            while let Some(((_, rows), incoming)) = frames.next_if(|((next, _), _)| *next == operation) {
                merge(&mut combined, &incoming);
                guarded.push((rows, incoming));
            }

            operations.insert(operation, Self::Multiple { combined, guarded });
        }

        operations
    }

    fn select(
        &self,
        selection: Option<&StorageOutcomeRows>,
        require_coverage: bool,
        merge: impl Fn(&mut T, &T),
    ) -> Option<Cow<'_, T>> {
        let Some(selection) = selection else {
            return Some(Cow::Borrowed(self.combined()));
        };

        let mut frames = self.matching(selection, require_coverage)?;
        let mut frame = Cow::Borrowed(frames.next()?);

        for incoming in frames {
            merge(frame.to_mut(), incoming);
        }

        Some(frame)
    }

    pub(super) fn common_for(&self, selection: Option<&StorageOutcomeRows>) -> Option<&T>
    where
        T: PartialEq,
    {
        let Some(selection) = selection else { return Some(self.combined()); };

        let mut frames = self.matching(selection, true)?;
        let first = frames.next()?;

        frames.all(|value| value == first).then_some(first)
    }

    fn matching<'reached, 'selection>(
        &'reached self,
        selection: &'selection StorageOutcomeRows,
        require_coverage: bool,
    ) -> Option<impl Iterator<Item = &'reached T> + use<'reached, 'selection, T>> {
        let admitted = selection.rows.as_deref();
        let frames = self.guarded().filter(move |(rows, _)| rows.matches_context(selection));

        if require_coverage
            && admitted.is_some_and(|all| all.iter().any(|row| !frames.clone().any(|(rows, _)| rows.includes_row(row))))
        {
            return None;
        }

        Some(frames.filter(move |(rows, _)| admitted.is_none_or(|all| all.iter().any(|row| rows.includes_row(row))))
            .map(|(_, frame)| frame))
    }
}
