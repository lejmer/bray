use std::collections::BTreeSet;

const MAXIMUM_BATCH_PLAN_COUNT: usize = 64;
/// Maximum encoded bytes accepted for one external batch request document.
pub const MAXIMUM_TEST_BATCH_REQUEST_BYTES: usize = 256 * 1024;
/// Maximum UTF-8 bytes in one stable plan identity.
pub const MAXIMUM_TEST_BATCH_PLAN_IDENTITY_BYTES: usize = 64;
/// Maximum selection filters carried by one plan.
pub const MAXIMUM_TEST_BATCH_PLAN_FILTERS: usize = 64;
/// Maximum UTF-8 bytes in one selection filter.
pub const MAXIMUM_TEST_BATCH_FILTER_BYTES: usize = 256;
const MAXIMUM_TEST_BATCH_TEXT_BYTES: usize = 64 * 1024;

/// One ordered native test execution over a shared freshly built host.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(
    feature = "serialization",
    derive(serde::Deserialize, serde::Serialize)
)]
#[cfg_attr(feature = "serialization", serde(deny_unknown_fields))]
pub struct TestBatchPlan {
    identity: String,
    filters: Vec<String>,
    maximum_concurrency: usize,
    timeout_milliseconds: Option<u64>,
}

impl TestBatchPlan {
    /// Creates one plan when its identity, filters, and worker count satisfy protocol limits.
    pub fn try_new(
        identity: impl Into<String>,
        filters: impl IntoIterator<Item = String>,
        maximum_concurrency: usize,
        timeout_milliseconds: Option<u64>,
    ) -> Result<Self, TestBatchPlanError> {
        let identity = identity.into();

        if !valid_plan_identity(&identity) {
            return Err(TestBatchPlanError::Identity);
        }

        if maximum_concurrency == 0 {
            return Err(TestBatchPlanError::Concurrency);
        }

        let mut bounded_filters = Vec::new();

        for filter in filters {
            if bounded_filters.len() == MAXIMUM_TEST_BATCH_PLAN_FILTERS {
                return Err(TestBatchPlanError::FilterCount);
            }

            if filter.len() > MAXIMUM_TEST_BATCH_FILTER_BYTES {
                return Err(TestBatchPlanError::Filter);
            }

            bounded_filters.push(filter);
        }

        Ok(Self {
            identity,
            filters: bounded_filters,
            maximum_concurrency,
            timeout_milliseconds,
        })
    }

    /// Returns the stable plan identity.
    pub fn identity(&self) -> &str {
        &self.identity
    }

    /// Returns ordered selection filters.
    pub fn filters(&self) -> &[String] {
        &self.filters
    }

    /// Returns the maximum runner concurrency.
    pub const fn maximum_concurrency(&self) -> usize {
        self.maximum_concurrency
    }

    /// Returns the per-test timeout in milliseconds when bounded.
    pub const fn timeout_milliseconds(&self) -> Option<u64> {
        self.timeout_milliseconds
    }

    fn validate(&self) -> Result<(), TestBatchPlanError> {
        if !valid_plan_identity(&self.identity) {
            return Err(TestBatchPlanError::Identity);
        }

        if self.maximum_concurrency == 0 {
            return Err(TestBatchPlanError::Concurrency);
        }

        if self.filters.len() > MAXIMUM_TEST_BATCH_PLAN_FILTERS {
            return Err(TestBatchPlanError::FilterCount);
        }

        if self
            .filters
            .iter()
            .any(|filter| filter.len() > MAXIMUM_TEST_BATCH_FILTER_BYTES)
        {
            return Err(TestBatchPlanError::Filter);
        }

        Ok(())
    }
}

fn valid_plan_identity(identity: &str) -> bool {
    !identity.is_empty()
        && identity.len() <= MAXIMUM_TEST_BATCH_PLAN_IDENTITY_BYTES
        && identity
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// Stable rejection category for one test batch plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TestBatchPlanError {
    /// The plan identity is empty, too long, or contains unsupported bytes.
    Identity,
    /// The plan contains too many selection filters.
    FilterCount,
    /// One selection filter exceeds the finite text limit.
    Filter,
    /// The plan requests no execution workers.
    Concurrency,
}

/// Ordered execution plans sharing one freshly built set of test hosts.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(
    feature = "serialization",
    derive(serde::Deserialize, serde::Serialize)
)]
#[cfg_attr(feature = "serialization", serde(deny_unknown_fields))]
pub struct TestBatchRequest {
    plans: Vec<TestBatchPlan>,
}

impl TestBatchRequest {
    /// Creates a finite nonempty batch with unique identities and bounded aggregate text.
    pub fn try_new(
        plans: impl IntoIterator<Item = TestBatchPlan>,
    ) -> Result<Self, TestBatchRequestError> {
        let mut bounded_plans = Vec::new();

        for plan in plans {
            if bounded_plans.len() == MAXIMUM_BATCH_PLAN_COUNT {
                return Err(TestBatchRequestError::PlanCount);
            }

            bounded_plans.push(plan);
        }

        validate_plans(&bounded_plans)?;

        Ok(Self {
            plans: bounded_plans,
        })
    }

    /// Validates data decoded at an external serialization boundary.
    pub fn validate(&self) -> Result<(), TestBatchRequestError> {
        validate_plans(&self.plans)
    }

    /// Returns plans in execution order.
    pub fn plans(&self) -> &[TestBatchPlan] {
        &self.plans
    }
}

fn validate_plans(plans: &[TestBatchPlan]) -> Result<(), TestBatchRequestError> {
    if plans.is_empty() || plans.len() > MAXIMUM_BATCH_PLAN_COUNT {
        return Err(TestBatchRequestError::PlanCount);
    }

    let mut identities = BTreeSet::new();
    let mut text_bytes = 0_usize;

    for plan in plans {
        plan.validate().map_err(TestBatchRequestError::Plan)?;

        if !identities.insert(plan.identity()) {
            return Err(TestBatchRequestError::DuplicateIdentity);
        }

        text_bytes = text_bytes
            .checked_add(plan.identity.len())
            .and_then(|bytes| {
                plan.filters
                    .iter()
                    .try_fold(bytes, |bytes, filter| bytes.checked_add(filter.len()))
            })
            .ok_or(TestBatchRequestError::AggregateBytes)?;
    }

    if text_bytes > MAXIMUM_TEST_BATCH_TEXT_BYTES {
        return Err(TestBatchRequestError::AggregateBytes);
    }

    Ok(())
}

/// Stable rejection category for a test batch request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TestBatchRequestError {
    /// The batch is empty or exceeds the finite plan limit.
    PlanCount,
    /// One plan violates its finite identity, filter, or worker contract.
    Plan(TestBatchPlanError),
    /// Two plans use the same identity.
    DuplicateIdentity,
    /// The aggregate request text exceeds the finite protocol limit.
    AggregateBytes,
}

#[cfg(test)]
mod tests {
    use super::{
        MAXIMUM_TEST_BATCH_FILTER_BYTES, MAXIMUM_TEST_BATCH_PLAN_FILTERS,
        MAXIMUM_TEST_BATCH_PLAN_IDENTITY_BYTES, TestBatchPlan, TestBatchPlanError,
        TestBatchRequest, TestBatchRequestError,
    };

    #[test]
    fn batch_requests_require_valid_unique_ordered_plans() {
        let first = TestBatchPlan::try_new("sequential", [], 1, Some(1000))
            .unwrap_or_else(|error| panic!("first plan must be valid: {error:?}"));

        let second = TestBatchPlan::try_new("parallel", ["memory".to_owned()], 2, None)
            .unwrap_or_else(|error| panic!("second plan must be valid: {error:?}"));

        let batch = TestBatchRequest::try_new([first.clone(), second.clone()])
            .unwrap_or_else(|error| panic!("batch must be valid: {error:?}"));

        assert_eq!(batch.plans(), [first.clone(), second]);

        assert_eq!(
            TestBatchRequest::try_new([first.clone(), first]),
            Err(TestBatchRequestError::DuplicateIdentity)
        );

        assert_eq!(
            TestBatchRequest::try_new([]),
            Err(TestBatchRequestError::PlanCount)
        );
    }

    #[test]
    fn plan_construction_rejects_oversized_identity_filters_and_filter_text() {
        assert_eq!(
            TestBatchPlan::try_new(
                "a".repeat(MAXIMUM_TEST_BATCH_PLAN_IDENTITY_BYTES + 1),
                [],
                1,
                None,
            ),
            Err(TestBatchPlanError::Identity)
        );

        assert_eq!(
            TestBatchPlan::try_new(
                "filters",
                (0..=MAXIMUM_TEST_BATCH_PLAN_FILTERS).map(|index| index.to_string()),
                1,
                None,
            ),
            Err(TestBatchPlanError::FilterCount)
        );

        assert_eq!(
            TestBatchPlan::try_new(
                "filter-text",
                ["x".repeat(MAXIMUM_TEST_BATCH_FILTER_BYTES + 1)],
                1,
                None,
            ),
            Err(TestBatchPlanError::Filter)
        );
    }

    #[test]
    fn batch_construction_rejects_oversized_aggregate_text() {
        let plans = (0..64).map(|index| {
            TestBatchPlan::try_new(
                format!("plan-{index}"),
                (0..MAXIMUM_TEST_BATCH_PLAN_FILTERS)
                    .map(|_| "x".repeat(MAXIMUM_TEST_BATCH_FILTER_BYTES)),
                1,
                None,
            )
            .unwrap_or_else(|error| panic!("individual plan must be bounded: {error:?}"))
        });

        assert_eq!(
            TestBatchRequest::try_new(plans),
            Err(TestBatchRequestError::AggregateBytes)
        );
    }
}
