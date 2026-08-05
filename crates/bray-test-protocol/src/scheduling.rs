use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Arc;

use bray_base::shared_slice;
use bray_symbols::TestExecutionConstraint;

use crate::{
    TestCapturePolicy, TestEntryMetadata, TestIdentity, TestInfrastructureFailure,
    TestInvocationPlan, TestInvocationResult, TestSelection,
};

/// Command-wide test execution mode.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TestExecutionMode {
    /// Admit at most one invocation at a time.
    Sequential,
    /// Admit independent invocations up to an explicit limit.
    Parallel {
        /// Maximum number of simultaneously active invocations.
        maximum_concurrency: NonZeroUsize,
    },
}

impl TestExecutionMode {
    /// Returns the maximum number of simultaneously active invocations.
    pub const fn maximum_concurrency(self) -> NonZeroUsize {
        match self {
            Self::Sequential => NonZeroUsize::MIN,
            Self::Parallel {
                maximum_concurrency,
            } => maximum_concurrency,
        }
    }
}

/// Failure to assemble one deterministic execution plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TestExecutionPlanBuildError {
    /// The invocation count does not match the selected entry count.
    InvocationCountMismatch,
    /// An invocation does not identify the entry at the same canonical position.
    InvocationIdentityMismatch(TestIdentity),
    /// More than one selected entry claims the same product-qualified identity.
    DuplicateIdentity(TestIdentity),
    /// One invocation cannot fit inside the command-wide capture reservation.
    CaptureBudgetExceeded(TestIdentity),
}

/// Immutable selected test work and command-wide admission limits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TestExecutionPlan {
    entries: Arc<[TestEntryMetadata]>,
    invocations: Arc<[TestInvocationPlan]>,
    mode: TestExecutionMode,
    capture_byte_budget: u64,
}

impl TestExecutionPlan {
    /// Validates selected entries, invocation policies, and the capture budget.
    pub fn try_new<'a>(
        selections: impl IntoIterator<Item = &'a TestSelection>,
        invocations: impl IntoIterator<Item = TestInvocationPlan>,
        mode: TestExecutionMode,
        capture_byte_budget: u64,
    ) -> Result<Self, TestExecutionPlanBuildError> {
        let mut entries = selections
            .into_iter()
            .flat_map(TestSelection::entries)
            .cloned()
            .collect::<Vec<_>>();

        entries.sort_by(|left, right| left.identity().cmp(right.identity()));

        if let Some(duplicate) = entries
            .windows(2)
            .find(|pair| pair[0].identity() == pair[1].identity())
        {
            return Err(TestExecutionPlanBuildError::DuplicateIdentity(
                duplicate[1].identity().clone(),
            ));
        }

        let mut invocations = invocations.into_iter().collect::<Vec<_>>();

        invocations.sort_by(|left, right| left.identity().cmp(right.identity()));

        if entries.len() != invocations.len() {
            return Err(TestExecutionPlanBuildError::InvocationCountMismatch);
        }

        for (entry, invocation) in entries.iter().zip(&invocations) {
            if entry.identity() != invocation.identity() {
                return Err(TestExecutionPlanBuildError::InvocationIdentityMismatch(
                    invocation.identity().clone(),
                ));
            }

            if capture_reservation(invocation.capture()) > capture_byte_budget {
                return Err(TestExecutionPlanBuildError::CaptureBudgetExceeded(
                    invocation.identity().clone(),
                ));
            }
        }

        Ok(Self {
            entries: shared_slice(entries),
            invocations: shared_slice(invocations),
            mode,
            capture_byte_budget,
        })
    }

    /// Returns selected entries in canonical catalog order.
    pub fn entries(&self) -> &[TestEntryMetadata] {
        &self.entries
    }

    /// Returns invocation policies in canonical catalog order.
    pub fn invocations(&self) -> &[TestInvocationPlan] {
        &self.invocations
    }

    /// Returns the selected command-wide execution mode.
    pub const fn mode(&self) -> TestExecutionMode {
        self.mode
    }

    /// Returns the maximum capture bytes reserved by active invocations.
    pub const fn capture_byte_budget(&self) -> u64 {
        self.capture_byte_budget
    }
}

/// One invocation admitted in deterministic catalog order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TestAdmission {
    catalog_index: usize,
    entry: TestEntryMetadata,
    invocation: TestInvocationPlan,
}

impl TestAdmission {
    /// Returns the invocation's canonical position in the selected catalog.
    pub const fn catalog_index(&self) -> usize {
        self.catalog_index
    }

    /// Returns the selected declaration metadata.
    pub const fn entry(&self) -> &TestEntryMetadata {
        &self.entry
    }

    /// Returns the invocation-owned timeout and capture policy.
    pub const fn invocation(&self) -> &TestInvocationPlan {
        &self.invocation
    }
}

/// Reason the runner stopped admitting selected work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TestStopReason {
    /// The complete command was cancelled.
    Cancelled,
    /// Runner or host infrastructure can no longer preserve the execution contract.
    InfrastructureFailure(TestInfrastructureFailure),
}

/// Invalid mutation of one admission schedule.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TestSchedulingError {
    /// A result does not belong to an active invocation.
    InvocationNotActive(TestIdentity),
    /// A completed result identifies a different invocation.
    ResultIdentityMismatch(TestIdentity),
}

/// Mutable admission state for one immutable execution plan.
///
/// The schedule chooses which roots may start. Admitted roots remain owned and
/// executed by the Bray runtime rather than by a second host-language scheduler.
#[derive(Debug)]
pub struct TestAdmissionSchedule {
    plan: TestExecutionPlan,
    next_index: usize,
    active: BTreeMap<TestIdentity, ActiveAdmission>,
    completed: Vec<Option<TestInvocationResult>>,
    active_capture_bytes: u64,
    active_serial: bool,
    stop_reason: Option<TestStopReason>,
}

#[derive(Clone, Copy, Debug)]
struct ActiveAdmission {
    catalog_index: usize,
    capture_bytes: u64,
    serial: bool,
}

impl TestAdmissionSchedule {
    /// Creates an empty admission schedule over one validated plan.
    pub fn new(plan: TestExecutionPlan) -> Self {
        let completed = std::iter::repeat_with(|| None)
            .take(plan.entries().len())
            .collect();

        Self {
            plan,
            next_index: 0,
            active: BTreeMap::new(),
            completed,
            active_capture_bytes: 0,
            active_serial: false,
            stop_reason: None,
        }
    }

    /// Admits the next canonical invocation when every active limit permits it.
    pub fn admit_next(&mut self) -> Option<TestAdmission> {
        if self.stop_reason.is_some()
            || self.active_serial
            || self.active.len() >= self.plan.mode().maximum_concurrency().get()
        {
            return None;
        }

        let entry = self.plan.entries().get(self.next_index)?;
        let invocation = &self.plan.invocations()[self.next_index];
        let serial = entry.constraint() == TestExecutionConstraint::Serial;

        if serial && !self.active.is_empty() {
            return None;
        }

        let capture_bytes = capture_reservation(invocation.capture());

        let available_capture = self
            .plan
            .capture_byte_budget()
            .saturating_sub(self.active_capture_bytes);

        if capture_bytes > available_capture {
            return None;
        }

        let catalog_index = self.next_index;

        self.next_index += 1;
        self.active_capture_bytes += capture_bytes;
        self.active_serial = serial;

        self.active.insert(
            invocation.identity().clone(),
            ActiveAdmission {
                catalog_index,
                capture_bytes,
                serial,
            },
        );

        Some(TestAdmission {
            catalog_index,
            entry: entry.clone(),
            invocation: invocation.clone(),
        })
    }

    /// Records a cleaned-up invocation result and releases its admission budgets.
    pub fn complete(
        &mut self,
        admission: &TestAdmission,
        result: TestInvocationResult,
    ) -> Result<(), TestSchedulingError> {
        if result.identity() != admission.invocation().identity() {
            return Err(TestSchedulingError::ResultIdentityMismatch(
                result.identity().clone(),
            ));
        }

        let Some(active) = self.active.remove(result.identity()) else {
            return Err(TestSchedulingError::InvocationNotActive(
                result.identity().clone(),
            ));
        };

        if active.catalog_index != admission.catalog_index() {
            self.active.insert(result.identity().clone(), active);

            return Err(TestSchedulingError::InvocationNotActive(
                result.identity().clone(),
            ));
        }

        self.active_capture_bytes = self
            .active_capture_bytes
            .saturating_sub(active.capture_bytes);

        if active.serial {
            self.active_serial = false;
        }

        self.completed[active.catalog_index] = Some(result);

        Ok(())
    }

    /// Stops future admission and returns active identities in canonical order.
    pub fn stop(&mut self, reason: TestStopReason) -> Arc<[TestIdentity]> {
        self.stop_reason.get_or_insert(reason);

        shared_slice(
            self.active
                .iter()
                .map(|(identity, active)| (active.catalog_index, identity.clone()))
                .collect::<BTreeMap<_, _>>()
                .into_values(),
        )
    }

    /// Returns whether no admitted invocation remains active.
    pub fn settled(&self) -> bool {
        self.active.is_empty()
    }

    /// Returns whether every selected invocation has completed.
    pub fn completed_all(&self) -> bool {
        self.next_index == self.plan.entries().len()
            && self.active.is_empty()
            && self.completed.iter().all(Option::is_some)
    }

    /// Returns the reason future admission stopped, when applicable.
    pub const fn stop_reason(&self) -> Option<TestStopReason> {
        self.stop_reason
    }

    /// Returns completed results in canonical catalog order.
    pub fn completed_results(&self) -> impl Iterator<Item = &TestInvocationResult> {
        self.completed.iter().filter_map(Option::as_ref)
    }
}

const fn capture_reservation(policy: TestCapturePolicy) -> u64 {
    match policy {
        TestCapturePolicy::Captured(limits) => limits.invocation_byte_limit(),
        TestCapturePolicy::Inherited | TestCapturePolicy::Discarded => 0,
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use bray_source::{SourceId, SourceSpan, SourceVersion, TextRange, TextSize};
    use bray_symbols::{
        CallableExecution, ModulePathKey, ProductIdentity, SymbolName, TestExecutionConstraint,
        TestResultShape,
    };

    use super::{
        TestAdmission, TestAdmissionSchedule, TestExecutionMode, TestExecutionPlan, TestStopReason,
    };
    use crate::test_support::{product, product_named};
    use crate::{
        CapturedStream, TestCaptureLimits, TestCapturePolicy, TestCatalog, TestDeclarationPath,
        TestDuration, TestEntryMetadata, TestIdentity, TestInvocationPlan, TestInvocationResult,
        TestOutcome, TestSelection, TestSelectionQuery, TestSourceAnchor, TestTimeoutPolicy,
    };

    #[test]
    fn sequential_mode_admits_exactly_one_invocation() {
        let selection = selection([
            entry("first", TestExecutionConstraint::Parallel),
            entry("second", TestExecutionConstraint::Parallel),
        ]);

        let plan = plan(&selection, TestExecutionMode::Sequential, 128);
        let mut schedule = TestAdmissionSchedule::new(plan);
        let first = admission(&mut schedule);

        assert!(schedule.admit_next().is_none());

        complete(&mut schedule, &first);

        assert_eq!(
            admission(&mut schedule)
                .entry()
                .identity()
                .declaration()
                .name()
                .as_str(),
            "second"
        );
    }

    #[test]
    fn parallel_completion_is_reported_in_catalog_order() {
        let selection = selection([
            entry("first", TestExecutionConstraint::Parallel),
            entry("second", TestExecutionConstraint::Parallel),
            entry("third", TestExecutionConstraint::Parallel),
        ]);

        let plan = plan(
            &selection,
            TestExecutionMode::Parallel {
                maximum_concurrency: nonzero(2),
            },
            128,
        );

        let mut schedule = TestAdmissionSchedule::new(plan);
        let first = admission(&mut schedule);
        let second = admission(&mut schedule);

        assert!(schedule.admit_next().is_none());

        complete(&mut schedule, &second);

        let third = admission(&mut schedule);

        complete(&mut schedule, &third);
        complete(&mut schedule, &first);

        let names = schedule
            .completed_results()
            .map(|result| result.identity().declaration().name().as_str())
            .collect::<Vec<_>>();

        assert_eq!(names, ["first", "second", "third"]);
        assert!(schedule.completed_all());
    }

    #[test]
    fn serial_entries_are_command_wide_barriers() {
        let first_product = product_named("a-tests");
        let second_product = product_named("b-tests");

        let first_selection = selection_for_product(
            first_product.clone(),
            [entry_for_product(
                first_product,
                "first",
                TestExecutionConstraint::Parallel,
            )],
        );

        let second_selection = selection_for_product(
            second_product.clone(),
            [
                entry_for_product(
                    second_product.clone(),
                    "a_serial",
                    TestExecutionConstraint::Serial,
                ),
                entry_for_product(second_product, "z_last", TestExecutionConstraint::Parallel),
            ],
        );

        let plan = plan_for_selections(
            [&first_selection, &second_selection],
            TestExecutionMode::Parallel {
                maximum_concurrency: nonzero(3),
            },
            128,
        );

        let mut schedule = TestAdmissionSchedule::new(plan);
        let first = admission(&mut schedule);

        assert!(schedule.admit_next().is_none());

        complete(&mut schedule, &first);

        let serial = admission(&mut schedule);

        assert!(schedule.admit_next().is_none());

        complete(&mut schedule, &serial);

        let last = admission(&mut schedule);

        assert_eq!(
            last.entry().identity().declaration().name().as_str(),
            "z_last"
        );
    }

    #[test]
    fn capture_reservations_bound_active_invocations() {
        let first_product = product_named("a-tests");
        let second_product = product_named("b-tests");

        let first_selection = selection_for_product(
            first_product.clone(),
            [entry_for_product(
                first_product,
                "first",
                TestExecutionConstraint::Parallel,
            )],
        );

        let second_selection = selection_for_product(
            second_product.clone(),
            [entry_for_product(
                second_product,
                "second",
                TestExecutionConstraint::Parallel,
            )],
        );

        let plan = plan_for_selections(
            [&first_selection, &second_selection],
            TestExecutionMode::Parallel {
                maximum_concurrency: nonzero(2),
            },
            64,
        );

        let mut schedule = TestAdmissionSchedule::new(plan);
        let first = admission(&mut schedule);

        assert!(schedule.admit_next().is_none());

        complete(&mut schedule, &first);

        assert_eq!(
            admission(&mut schedule)
                .entry()
                .identity()
                .declaration()
                .name()
                .as_str(),
            "second"
        );
    }

    #[test]
    fn stopping_admission_returns_active_work_in_catalog_order() {
        let selection = selection([
            entry("first", TestExecutionConstraint::Parallel),
            entry("second", TestExecutionConstraint::Parallel),
            entry("third", TestExecutionConstraint::Parallel),
        ]);

        let plan = plan(
            &selection,
            TestExecutionMode::Parallel {
                maximum_concurrency: nonzero(2),
            },
            128,
        );

        let mut schedule = TestAdmissionSchedule::new(plan);

        let _first = admission(&mut schedule);
        let _second = admission(&mut schedule);

        let active = schedule.stop(TestStopReason::Cancelled);

        let names = active
            .iter()
            .map(|identity| identity.declaration().name().as_str())
            .collect::<Vec<_>>();

        assert_eq!(names, ["first", "second"]);
        assert!(schedule.admit_next().is_none());
        assert_eq!(schedule.stop_reason(), Some(TestStopReason::Cancelled));
    }

    fn plan(
        selection: &TestSelection,
        mode: TestExecutionMode,
        capture_byte_budget: u64,
    ) -> TestExecutionPlan {
        plan_for_selections([selection], mode, capture_byte_budget)
    }

    fn plan_for_selections<'a>(
        selections: impl IntoIterator<Item = &'a TestSelection>,
        mode: TestExecutionMode,
        capture_byte_budget: u64,
    ) -> TestExecutionPlan {
        let selections = selections.into_iter().collect::<Vec<_>>();

        let invocations = selections.iter().flat_map(|selection| {
            selection.entries().iter().map(|entry| {
                TestInvocationPlan::new(
                    entry.identity().clone(),
                    TestTimeoutPolicy::Limit(TestDuration::from_nanoseconds(1_000_000)),
                    TestCapturePolicy::Captured(TestCaptureLimits::new(64, 64)),
                )
            })
        });

        TestExecutionPlan::try_new(
            selections.iter().copied(),
            invocations,
            mode,
            capture_byte_budget,
        )
        .unwrap_or_else(|error| panic!("test execution plan must be valid: {error:?}"))
    }

    fn admission(schedule: &mut TestAdmissionSchedule) -> TestAdmission {
        schedule
            .admit_next()
            .unwrap_or_else(|| panic!("next test must be admitted"))
    }

    fn complete(schedule: &mut TestAdmissionSchedule, admission: &TestAdmission) {
        let result = TestInvocationResult::after_cleanup(
            admission.invocation().identity().clone(),
            TestOutcome::Passed,
            CapturedStream::captured([], 0, None),
            CapturedStream::captured([], 0, None),
        );

        schedule
            .complete(admission, result)
            .unwrap_or_else(|error| panic!("test invocation must complete: {error:?}"));
    }

    fn selection(entries: impl IntoIterator<Item = TestEntryMetadata>) -> TestSelection {
        let product = product();

        selection_for_product(product, entries)
    }

    fn selection_for_product(
        product: ProductIdentity,
        entries: impl IntoIterator<Item = TestEntryMetadata>,
    ) -> TestSelection {
        let catalog = TestCatalog::try_new(product, entries)
            .unwrap_or_else(|error| panic!("test catalog must be valid: {error:?}"));

        TestSelection::from_catalog(&catalog, &TestSelectionQuery::default())
    }

    fn entry(name: &str, constraint: TestExecutionConstraint) -> TestEntryMetadata {
        entry_for_product(product(), name, constraint)
    }

    fn entry_for_product(
        product: ProductIdentity,
        name: &str,
        constraint: TestExecutionConstraint,
    ) -> TestEntryMetadata {
        let module = ModulePathKey::try_new(["example", "tests"])
            .unwrap_or_else(|| panic!("test module path must be valid"));

        let name =
            SymbolName::try_new(name).unwrap_or_else(|| panic!("test function name must be valid"));

        let identity = TestIdentity::new(product, TestDeclarationPath::new(module, name));

        let source = TestSourceAnchor::new(
            SourceSpan::new(
                SourceId::new(0),
                TextRange::new(TextSize::ZERO, TextSize::ZERO),
            ),
            SourceVersion::new(0),
        );

        TestEntryMetadata::new(
            identity,
            source,
            CallableExecution::Synchronous,
            constraint,
            TestResultShape::Unit,
            None,
        )
    }

    fn nonzero(value: usize) -> NonZeroUsize {
        NonZeroUsize::new(value).unwrap_or_else(|| panic!("test limit must be nonzero"))
    }
}
