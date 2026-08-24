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

fn valid_scheduler_statistics(report: &CompilationProfileReport) -> bool {
    let scheduler = report.scheduler;

    if scheduler.worker_budget == 0
        || scheduler.maximum_active_workers > scheduler.worker_budget
        || scheduler.maximum_ready_width > scheduler.ready_items
    {
        return false;
    }

    if scheduler.ready_waves == 0 {
        return scheduler.ready_items == 0 && scheduler.maximum_ready_width == 0;
    }

    scheduler.ready_items >= scheduler.ready_waves && scheduler.maximum_ready_width > 0
}

fn valid_query_statistics(query: &crate::CompilationProfileQueryStatistics) -> bool {
    if query.cache_hits.saturating_add(query.cache_misses) > query.requests
        || query.evaluations > query.cache_misses
        || query.evaluation_latency.samples != query.evaluations
        || query.published_values > query.evaluations
    {
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
    use crate::test_support::report;

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
    fn validation_rejects_inconsistent_scheduler_statistics() {
        let mut invalid = report(1_000_000);

        invalid.scheduler.maximum_active_workers = 2;

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
