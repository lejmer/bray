use std::collections::BTreeSet;

use crate::{COMPILATION_PROFILE_SCHEMA_REVISION, CompilationProfileReport};

/// Structural reason a compiler profile report cannot be analyzed safely.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompilationProfileDescriptorKind {
    /// Compiler operation descriptor.
    Operation,
    /// Demand-driven query descriptor.
    Query,
    /// Compilation metric descriptor.
    Metric,
}

/// Structural reason a compiler profile report cannot be analyzed safely.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompilationProfileValidationError {
    /// The report uses a schema revision unsupported by this toolchain.
    SchemaRevision { expected: u32, actual: u32 },
    /// A descriptor catalog contains a duplicate numeric identity.
    DuplicateDescriptor {
        kind: CompilationProfileDescriptorKind,
        id: u16,
    },
    /// Sparse observations contain a duplicate descriptor reference.
    DuplicateObservation {
        kind: CompilationProfileDescriptorKind,
        id: u16,
    },
    /// An observation or event references an undeclared descriptor.
    UnknownDescriptor {
        kind: CompilationProfileDescriptorKind,
        id: u16,
    },
    /// A selected runtime artifact has no stable identity.
    InvalidRuntimeArtifactIdentity { index: usize },
    /// Selected runtime artifacts are duplicated or not in canonical identity order.
    NonCanonicalRuntimeArtifacts { first: String, second: String },
    /// One selected runtime role has no stable identity.
    InvalidRuntimeRole { index: usize },
    /// Selected runtime roles are duplicated or not in canonical identity order.
    NonCanonicalRuntimeRoles { first: String, second: String },
    /// One native callback entry has no stable symbol identity.
    InvalidNativeCallbackEntry { index: usize },
    /// Native callback entries are duplicated or not in canonical symbol order.
    NonCanonicalNativeCallbackEntries { first: String, second: String },
    /// Scheduler aggregates contradict the configured worker budget or ready-work counts.
    InvalidSchedulerStatistics,
    /// Query aggregates contradict their request, evaluation, or distribution counts.
    InvalidQueryStatistics { id: u16 },
}

impl CompilationProfileReport {
    /// Validates schema compatibility and descriptor references.
    pub fn validate(&self) -> Result<(), CompilationProfileValidationError> {
        if self.schema_revision != COMPILATION_PROFILE_SCHEMA_REVISION {
            return Err(CompilationProfileValidationError::SchemaRevision {
                expected: COMPILATION_PROFILE_SCHEMA_REVISION,
                actual: self.schema_revision,
            });
        }

        validate_unique(
            self.descriptors.operations.iter().map(|entry| entry.id),
            CompilationProfileDescriptorKind::Operation,
        )?;

        validate_unique(
            self.descriptors.queries.iter().map(|entry| entry.id),
            CompilationProfileDescriptorKind::Query,
        )?;

        validate_unique(
            self.descriptors.metrics.iter().map(|entry| entry.id),
            CompilationProfileDescriptorKind::Metric,
        )?;

        validate_observations(
            self.operations.iter().map(|entry| entry.id),
            self.descriptors.operations.iter().map(|entry| entry.id),
            CompilationProfileDescriptorKind::Operation,
        )?;

        if !valid_scheduler_statistics(self) {
            return Err(CompilationProfileValidationError::InvalidSchedulerStatistics);
        }

        if let Some(query) = self
            .queries
            .iter()
            .find(|query| !valid_query_statistics(query))
        {
            return Err(CompilationProfileValidationError::InvalidQueryStatistics { id: query.id });
        }

        validate_observations(
            self.queries.iter().map(|entry| entry.id),
            self.descriptors.queries.iter().map(|entry| entry.id),
            CompilationProfileDescriptorKind::Query,
        )?;

        validate_observations(
            self.metrics.iter().map(|entry| entry.id),
            self.descriptors.metrics.iter().map(|entry| entry.id),
            CompilationProfileDescriptorKind::Metric,
        )?;

        if let Some(index) = self
            .runtime_artifacts
            .iter()
            .position(|artifact| artifact.identity.trim().is_empty())
        {
            return Err(
                CompilationProfileValidationError::InvalidRuntimeArtifactIdentity { index },
            );
        }

        if let Some(entries) = self
            .runtime_artifacts
            .windows(2)
            .find(|entries| entries[0].identity >= entries[1].identity)
        {
            return Err(
                CompilationProfileValidationError::NonCanonicalRuntimeArtifacts {
                    first: entries[0].identity.clone(),
                    second: entries[1].identity.clone(),
                },
            );
        }

        validate_runtime_roles(&self.runtime_roles)?;
        validate_native_callback_entries(&self.native_callback_entries)?;

        for event in &self.events {
            if self.operation_descriptor(event.operation_id).is_none() {
                return Err(CompilationProfileValidationError::UnknownDescriptor {
                    kind: CompilationProfileDescriptorKind::Operation,
                    id: event.operation_id,
                });
            }

            if let Some(id) = event.query_id
                && self.query_descriptor(id).is_none()
            {
                return Err(CompilationProfileValidationError::UnknownDescriptor {
                    kind: CompilationProfileDescriptorKind::Query,
                    id,
                });
            }
        }

        Ok(())
    }
}

fn validate_runtime_roles(roles: &[String]) -> Result<(), CompilationProfileValidationError> {
    validate_canonical_strings(
        roles,
        |index| CompilationProfileValidationError::InvalidRuntimeRole { index },
        |first, second| CompilationProfileValidationError::NonCanonicalRuntimeRoles {
            first,
            second,
        },
    )
}

fn validate_native_callback_entries(
    entries: &[String],
) -> Result<(), CompilationProfileValidationError> {
    validate_canonical_strings(
        entries,
        |index| CompilationProfileValidationError::InvalidNativeCallbackEntry { index },
        |first, second| CompilationProfileValidationError::NonCanonicalNativeCallbackEntries {
            first,
            second,
        },
    )
}

fn validate_canonical_strings(
    entries: &[String],
    invalid: impl FnOnce(usize) -> CompilationProfileValidationError,
    noncanonical: impl FnOnce(String, String) -> CompilationProfileValidationError,
) -> Result<(), CompilationProfileValidationError> {
    if let Some(index) = entries.iter().position(|entry| entry.trim().is_empty()) {
        return Err(invalid(index));
    }

    if let Some(entries) = entries.windows(2).find(|entries| entries[0] >= entries[1]) {
        return Err(noncanonical(entries[0].clone(), entries[1].clone()));
    }

    Ok(())
}

fn valid_scheduler_statistics(report: &CompilationProfileReport) -> bool {
    let scheduler = &report.scheduler;

    if scheduler.worker_budget == 0
        || scheduler.maximum_active_workers > scheduler.worker_budget
        || scheduler.maximum_ready_width > scheduler.ready_items
    {
        return false;
    }

    if scheduler.ready_waves == 0
        && (scheduler.ready_items != 0 || scheduler.maximum_ready_width != 0)
    {
        return false;
    }

    let mut classes = BTreeSet::new();
    let mut waves = 0_u64;
    let mut ready_items = 0_u64;
    let mut maximum_ready_width = 0_u64;

    if scheduler.wave_classes.windows(2).any(|classes| {
        (classes[0].operation_id, classes[0].query_id)
            >= (classes[1].operation_id, classes[1].query_id)
    }) {
        return false;
    }

    for class in &scheduler.wave_classes {
        if !classes.insert((class.operation_id, class.query_id))
            || class.ready_items > class.planned_items
            || class.ready_width.samples != class.waves
            || class.active_workers.samples != class.waves
            || class.ready_width.maximum > class.planned_items
            || class.active_workers.maximum > scheduler.worker_budget
            || !valid_count_distribution(class.ready_width)
            || !valid_count_distribution(class.active_workers)
            || class
                .operation_id
                .is_some_and(|id| report.operation_descriptor(id).is_none())
            || class
                .query_id
                .is_some_and(|id| report.query_descriptor(id).is_none())
        {
            return false;
        }

        waves = waves.saturating_add(class.waves);
        ready_items = ready_items.saturating_add(class.ready_items);
        maximum_ready_width = maximum_ready_width.max(class.ready_width.maximum);
    }

    waves == scheduler.ready_waves
        && ready_items == scheduler.ready_items
        && maximum_ready_width == scheduler.maximum_ready_width
}

fn valid_count_distribution(distribution: crate::CompilationProfileCountDistribution) -> bool {
    if distribution.samples == 0 {
        return distribution == crate::CompilationProfileCountDistribution::default();
    }

    distribution.minimum <= distribution.maximum
        && distribution.minimum <= distribution.median_upper_bound
        && distribution.median_upper_bound <= distribution.p95_upper_bound
}

fn valid_query_statistics(query: &crate::CompilationProfileQueryStatistics) -> bool {
    if query.cache_hits.saturating_add(query.cache_misses) > query.requests
        || query.evaluations > query.cache_misses
        || query.evaluation_latency.samples != query.evaluations
        || query.published_values > query.evaluations
        || query.diagnostic_collections > query.published_values
        || query.diagnostic_copies > query.cloned_values
    {
        return false;
    }

    if query.diagnostic_collections == 0 && query.result_diagnostics > 0 {
        return false;
    }

    if query.diagnostic_copies == 0 && query.cloned_diagnostics > 0 {
        return false;
    }

    if query.cache_hits == 0
        && (query.ready_value_nanoseconds > 0 || query.ready_value_maximum_nanoseconds > 0)
    {
        return false;
    }

    if query.ready_value_maximum_nanoseconds > query.ready_value_nanoseconds {
        return false;
    }

    if query.evaluations == 0 {
        return query.evaluation_nanoseconds == 0
            && query.evaluation_self_nanoseconds == 0
            && query.evaluation_latency
                == crate::CompilationProfileDurationDistribution::default();
    }

    query.evaluation_latency.minimum_nanoseconds <= query.evaluation_latency.maximum_nanoseconds
        && query.evaluation_latency.minimum_nanoseconds
            <= query.evaluation_latency.median_upper_bound_nanoseconds
        && query.evaluation_latency.median_upper_bound_nanoseconds
            <= query.evaluation_latency.p95_upper_bound_nanoseconds
        && query.evaluation_self_nanoseconds <= query.evaluation_nanoseconds
}

fn validate_unique(
    ids: impl IntoIterator<Item = u16>,
    kind: CompilationProfileDescriptorKind,
) -> Result<(), CompilationProfileValidationError> {
    let mut observed = BTreeSet::new();

    for id in ids {
        if !observed.insert(id) {
            return Err(CompilationProfileValidationError::DuplicateDescriptor { kind, id });
        }
    }

    Ok(())
}

fn validate_observations(
    observations: impl IntoIterator<Item = u16>,
    descriptors: impl IntoIterator<Item = u16>,
    kind: CompilationProfileDescriptorKind,
) -> Result<(), CompilationProfileValidationError> {
    let descriptors = descriptors.into_iter().collect::<BTreeSet<_>>();
    let mut observed = BTreeSet::new();

    for id in observations {
        if !observed.insert(id) {
            return Err(CompilationProfileValidationError::DuplicateObservation { kind, id });
        }

        if !descriptors.contains(&id) {
            return Err(CompilationProfileValidationError::UnknownDescriptor { kind, id });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{CompilationProfileDescriptorKind, CompilationProfileValidationError};
    use crate::{
        CompilationProfileCountDistribution, CompilationProfileSchedulingWaveStatistics,
        test_support::report,
    };

    #[test]
    fn validation_rejects_unknown_and_duplicate_observation_descriptors() {
        let mut unknown = report(1_000_000);
        unknown.operations[0].id = 99;

        assert_eq!(
            unknown.validate(),
            Err(CompilationProfileValidationError::UnknownDescriptor {
                kind: CompilationProfileDescriptorKind::Operation,
                id: 99,
            })
        );

        let mut duplicate = report(1_000_000);
        duplicate.operations.push(duplicate.operations[0].clone());

        assert_eq!(
            duplicate.validate(),
            Err(CompilationProfileValidationError::DuplicateObservation {
                kind: CompilationProfileDescriptorKind::Operation,
                id: duplicate.operations[0].id,
            })
        );
    }

    #[test]
    fn validation_requires_canonical_runtime_artifact_identities() {
        let mut invalid = report(1_000_000);

        invalid.runtime_artifacts = vec![crate::CompilationProfileRuntimeArtifact {
            identity: " ".to_owned(),
            bytes: 1,
        }];

        assert_eq!(
            invalid.validate(),
            Err(CompilationProfileValidationError::InvalidRuntimeArtifactIdentity { index: 0 })
        );

        let mut duplicate = report(1_000_000);

        duplicate.runtime_artifacts = vec![
            crate::CompilationProfileRuntimeArtifact {
                identity: "runtime.host".to_owned(),
                bytes: 1,
            },
            crate::CompilationProfileRuntimeArtifact {
                identity: "runtime.host".to_owned(),
                bytes: 1,
            },
        ];

        assert_eq!(
            duplicate.validate(),
            Err(
                CompilationProfileValidationError::NonCanonicalRuntimeArtifacts {
                    first: "runtime.host".to_owned(),
                    second: "runtime.host".to_owned(),
                }
            )
        );
    }

    #[test]
    fn validation_requires_canonical_runtime_roles_and_callback_entries() {
        let mut invalid_role = report(1_000_000);
        invalid_role.runtime_roles = vec![" ".to_owned()];

        assert_eq!(
            invalid_role.validate(),
            Err(CompilationProfileValidationError::InvalidRuntimeRole { index: 0 })
        );

        let mut duplicate_role = report(1_000_000);
        duplicate_role.runtime_roles = vec!["panic_reporting".to_owned(); 2];

        assert_eq!(
            duplicate_role.validate(),
            Err(
                CompilationProfileValidationError::NonCanonicalRuntimeRoles {
                    first: "panic_reporting".to_owned(),
                    second: "panic_reporting".to_owned(),
                }
            )
        );

        let mut invalid_entry = report(1_000_000);
        invalid_entry.native_callback_entries = vec![String::new()];

        assert_eq!(
            invalid_entry.validate(),
            Err(CompilationProfileValidationError::InvalidNativeCallbackEntry { index: 0 })
        );

        let mut duplicate_entry = report(1_000_000);
        duplicate_entry.native_callback_entries = vec!["callback".to_owned(); 2];

        assert_eq!(
            duplicate_entry.validate(),
            Err(
                CompilationProfileValidationError::NonCanonicalNativeCallbackEntries {
                    first: "callback".to_owned(),
                    second: "callback".to_owned(),
                }
            )
        );
    }

    #[test]
    fn validation_rejects_inconsistent_scheduler_statistics() {
        let mut invalid = report(1_000_000);

        invalid.scheduler.maximum_active_workers = 2;

        assert_eq!(
            invalid.validate(),
            Err(CompilationProfileValidationError::InvalidSchedulerStatistics)
        );
    }

    #[test]
    fn validation_rejects_noncanonical_scheduling_wave_classes() {
        let mut invalid = report(1_000_000);

        let distribution = CompilationProfileCountDistribution {
            samples: 1,
            minimum: 1,
            median_upper_bound: 1,
            p95_upper_bound: 1,
            maximum: 1,
        };

        let class = CompilationProfileSchedulingWaveStatistics {
            operation_id: Some(1),
            query_id: Some(1_000),
            waves: 1,
            planned_items: 1,
            ready_items: 1,
            ready_width: distribution,
            active_workers: distribution,
        };

        invalid.scheduler.ready_waves = 2;
        invalid.scheduler.ready_items = 2;
        invalid.scheduler.maximum_ready_width = 1;
        invalid.scheduler.wave_classes = vec![class.clone(), class];

        assert_eq!(
            invalid.validate(),
            Err(CompilationProfileValidationError::InvalidSchedulerStatistics)
        );
    }

    #[test]
    fn validation_rejects_inconsistent_query_statistics() {
        let mut invalid = report(1_000_000);

        invalid.queries[0].evaluation_latency.samples = 2;

        assert_eq!(
            invalid.validate(),
            Err(CompilationProfileValidationError::InvalidQueryStatistics {
                id: invalid.queries[0].id,
            })
        );
    }
}
