use std::any::Any;
use std::fmt;
use std::sync::{Arc, Mutex};

use bray_runtime_model::{ProtectedAsyncFrameId, ProtectedFrameStateId};

use crate::incident::dispose_report;
use crate::outgoing::OutgoingRecords;
use crate::{RunOutcome, TaskId};

/// A cleanup failure transferred to the product host for reporting.
pub struct CleanupIncident {
    metadata: CleanupIncidentMetadata,
    payload: Option<crate::RuntimePanic>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CleanupIncidentMetadata {
    ordinal: u64,
    producer: CleanupIncidentProducer,
    origin: CleanupIncidentOrigin,
}

impl CleanupIncident {
    pub(crate) fn new(metadata: CleanupIncidentMetadata, payload: crate::RuntimePanic) -> Self {
        Self {
            metadata,
            payload: Some(payload),
        }
    }

    pub(crate) fn dispose(&mut self) -> bray_runtime_abi::NativeRuntimeStatus {
        self.payload.take().map_or(
            bray_runtime_abi::NativeRuntimeStatus::SUCCESS,
            dispose_report,
        )
    }

    /// Returns the deterministic encounter ordinal.
    pub const fn ordinal(&self) -> u64 {
        self.metadata.ordinal
    }

    /// Returns the run that produced the incident.
    pub const fn producer(&self) -> CleanupIncidentProducer {
        self.metadata.producer
    }

    /// Returns the protected-frame location that produced the incident.
    pub const fn origin(&self) -> CleanupIncidentOrigin {
        self.metadata.origin
    }

    /// Returns whether the primary payload has one exact host representation.
    pub fn payload_is<T: Any>(&self) -> bool {
        self.payload
            .as_ref()
            .is_some_and(|panic| panic.primary_type_id() == std::any::TypeId::of::<T>())
    }

    /// Returns the host type identity carried by the primary payload.
    pub fn payload_type_id(&self) -> std::any::TypeId {
        self.payload
            .as_ref()
            .map(crate::RuntimePanic::primary_type_id)
            .unwrap_or_else(|| unreachable!("live cleanup incident must own its payload"))
    }
}

impl Drop for CleanupIncident {
    fn drop(&mut self) {
        self.dispose();
    }
}

impl fmt::Debug for CleanupIncident {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CleanupIncident")
            .field("ordinal", &self.metadata.ordinal)
            .field("producer", &self.metadata.producer)
            .field("origin", &self.metadata.origin)
            .finish_non_exhaustive()
    }
}

/// Runtime run boundary that produced a cleanup incident.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CleanupIncidentProducer {
    /// The directly executing synchronous root run.
    SynchronousRoot,
    /// One host-owned or source-owned runtime task.
    Task(TaskId),
}

/// Protected-frame location correlated with a cleanup incident.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CleanupIncidentOrigin {
    frame: ProtectedAsyncFrameId,
    state: ProtectedFrameStateId,
}

impl CleanupIncidentOrigin {
    /// Creates a cleanup origin from its frame and retained state.
    pub const fn new(frame: ProtectedAsyncFrameId, state: ProtectedFrameStateId) -> Self {
        Self { frame, state }
    }

    /// Returns the protected frame that ran cleanup.
    pub const fn frame(self) -> ProtectedAsyncFrameId {
        self.frame
    }

    /// Returns the retained frame state that produced the incident.
    pub const fn state(self) -> ProtectedFrameStateId {
        self.state
    }
}

/// Mandatory product-host sink for cleanup incidents not attached to a returned panic.
#[derive(Clone, Debug, Default)]
pub struct CleanupReportSink {
    state: Arc<Mutex<CleanupReportState>>,
}

#[derive(Debug, Default)]
struct CleanupReportState {
    next_ordinal: u64,
    incidents: OutgoingRecords,
    pending_count: usize,
    draining: bool,
}

struct CleanupDrain<'a>(Option<&'a Mutex<CleanupReportState>>);

impl Drop for CleanupDrain<'_> {
    fn drop(&mut self) {
        if let Some(state) = self.0 {
            state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .draining = false;
        }
    }
}

impl CleanupReportSink {
    /// Creates an empty cleanup-report sink.
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn transfer(
        &self,
        producer: CleanupIncidentProducer,
        origin: CleanupIncidentOrigin,
        payload: crate::RuntimePanic,
        admitted: &mut OutgoingRecords,
    ) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let ordinal = state.next_ordinal;
        state.next_ordinal = state.next_ordinal.saturating_add(1);

        state.incidents.push_incident(
            payload,
            CleanupIncidentMetadata {
                ordinal,
                producer,
                origin,
            },
            admitted,
        );

        state.pending_count += 1;
    }

    /// Reports and removes every incident in transfer order.
    ///
    /// Callbacks receive detached incidents. Reentrant transfers join the next batch.
    /// Nested or concurrent drains leave reporting to the active drain.
    /// If a callback unwinds, the remaining detached incidents are released.
    pub fn drain(&self, mut report: impl FnMut(CleanupIncident)) {
        {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            if state.draining {
                return;
            }

            state.draining = true;
        }

        let mut drain = CleanupDrain(Some(&self.state));

        loop {
            let mut incidents = {
                let mut state = self
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);

                if state.pending_count == 0 {
                    state.draining = false;
                    drain.0 = None;
                    return;
                }

                state.pending_count = 0;

                std::mem::take(&mut state.incidents)
            };

            while let Some(incident) = incidents.pop_incident() {
                report(incident);
            }
        }
    }

    /// Returns the number of incidents awaiting product-host reporting.
    pub fn pending_count(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pending_count
    }
}

/// Maps a terminal root result, drains cleanup reports, and shuts runtime infrastructure down.
pub fn finish_product_shutdown<T, R, E>(
    outcome: RunOutcome<T>,
    cleanup_reports: &CleanupReportSink,
    map_outcome: impl FnOnce(RunOutcome<T>) -> R,
    report_incident: impl FnMut(CleanupIncident),
    shutdown_runtime: impl FnOnce() -> Result<(), E>,
) -> Result<R, E> {
    let result = map_outcome(outcome);

    cleanup_reports.drain(report_incident);
    shutdown_runtime()?;

    Ok(result)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::{
        CleanupIncidentOrigin, CleanupIncidentProducer, CleanupReportSink, finish_product_shutdown,
    };
    use crate::RunOutcome;

    #[test]
    fn grouped_reports_transfer_without_further_admission_and_dispose_in_order() {
        use crate::RuntimePanic;
        use crate::outgoing::OutgoingRecords;
        use std::cell::Cell;

        thread_local! { static RELEASE_ORDER: Cell<u64> = const { Cell::new(0) }; }
        struct Payload(u64);

        impl Drop for Payload {
            fn drop(&mut self) {
                RELEASE_ORDER.set(RELEASE_ORDER.get() * 10 + self.0);
            }
        }

        let sink = CleanupReportSink::new();

        let origin = CleanupIncidentOrigin::new(
            bray_runtime_model::ProtectedAsyncFrameId::new([4; 32]),
            bray_runtime_model::ProtectedFrameStateId::new(1),
        );

        let mut admitted = OutgoingRecords::admit(4).unwrap();
        let denied = crate::outgoing::tests::reject_admission();
        let mut first = RuntimePanic::new(Payload(1));
        first.push_suppressed(Box::new(Payload(2)), &mut admitted);
        first.push_suppressed(Box::new(Payload(3)), &mut admitted);

        sink.transfer(
            CleanupIncidentProducer::SynchronousRoot,
            origin,
            first,
            &mut admitted,
        );

        sink.transfer(
            CleanupIncidentProducer::SynchronousRoot,
            origin,
            RuntimePanic::new(Payload(4)),
            &mut admitted,
        );

        assert!(OutgoingRecords::admit(1).is_err());
        assert_eq!(admitted.len(), 0);
        assert_eq!(sink.pending_count(), 2);
        assert_eq!(RELEASE_ORDER.get(), 0);
        let mut ordinal = 0;

        sink.drain(|incident| {
            assert_eq!(incident.ordinal(), ordinal);
            assert_eq!(incident.origin(), origin);
            assert!(incident.payload_is::<Payload>());

            assert_eq!(
                incident.payload.as_ref().unwrap().suppressed_count(),
                if ordinal == 0 { 2 } else { 0 }
            );

            ordinal += 1;
        });

        assert_eq!(ordinal, 2);
        assert_eq!(RELEASE_ORDER.get(), 1234);
        assert_eq!(sink.pending_count(), 0);
        drop(denied);
    }

    #[test]
    fn product_shutdown_maps_then_reports_then_stops_infrastructure() {
        let reports = CleanupReportSink::new();

        let origin = CleanupIncidentOrigin::new(
            bray_runtime_model::ProtectedAsyncFrameId::new([9; 32]),
            bray_runtime_model::ProtectedFrameStateId::new(3),
        );

        reports.transfer(
            CleanupIncidentProducer::SynchronousRoot,
            origin,
            crate::RuntimePanic::new("first"),
            &mut crate::outgoing::OutgoingRecords::admit(1).unwrap(),
        );

        reports.transfer(
            CleanupIncidentProducer::SynchronousRoot,
            origin,
            crate::RuntimePanic::new("second"),
            &mut crate::outgoing::OutgoingRecords::admit(1).unwrap(),
        );

        let events = RefCell::new(Vec::new());

        let result = finish_product_shutdown(
            RunOutcome::Completed(7),
            &reports,
            |outcome| {
                events.borrow_mut().push("map");

                match outcome {
                    RunOutcome::Completed(value) => value,
                    RunOutcome::Cancelled | RunOutcome::Panicked(_) => 0,
                }
            },
            |incident| {
                assert!(incident.payload_is::<&'static str>());

                assert_eq!(
                    incident.payload_type_id(),
                    std::any::TypeId::of::<&'static str>()
                );

                events.borrow_mut().push("report");
            },
            || {
                events.borrow_mut().push("shutdown");

                Ok::<_, ()>(())
            },
        )
        .unwrap_or_else(|()| panic!("runtime shutdown must succeed"));

        assert_eq!(result, 7);

        assert_eq!(events.into_inner(), ["map", "report", "report", "shutdown"]);

        assert_eq!(reports.pending_count(), 0);
    }
}
