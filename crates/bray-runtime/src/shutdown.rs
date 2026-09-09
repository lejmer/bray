use std::any::Any;
use std::collections::VecDeque;
use std::fmt;
use std::sync::{Arc, Mutex};

use bray_runtime_model::{ProtectedAsyncFrameId, ProtectedFrameStateId};

use crate::incident::OwnedCleanupIncident;
use crate::{RunOutcome, TaskId};

/// A cleanup failure transferred to the product host for reporting.
pub struct CleanupIncident {
    ordinal: u64,
    producer: CleanupIncidentProducer,
    origin: CleanupIncidentOrigin,
    payload: OwnedCleanupIncident,
}

impl CleanupIncident {
    fn new(
        ordinal: u64,
        producer: CleanupIncidentProducer,
        origin: CleanupIncidentOrigin,
        payload: OwnedCleanupIncident,
    ) -> Self {
        Self {
            ordinal,
            producer,
            origin,
            payload,
        }
    }

    /// Returns the deterministic encounter ordinal.
    pub const fn ordinal(&self) -> u64 {
        self.ordinal
    }

    /// Returns the run that produced the incident.
    pub const fn producer(&self) -> CleanupIncidentProducer {
        self.producer
    }

    /// Returns the protected-frame location that produced the incident.
    pub const fn origin(&self) -> CleanupIncidentOrigin {
        self.origin
    }

    /// Returns whether the erased payload has one exact host representation.
    pub fn payload_is<T: Any>(&self) -> bool {
        self.payload.payload().is::<T>()
    }

    /// Returns the host type identity carried by the erased payload descriptor.
    pub fn payload_type_id(&self) -> std::any::TypeId {
        self.payload.payload().type_id()
    }

    /// Reports and destroys an owned native payload after its host-origin record is emitted.
    pub(crate) fn report_native_payload(self) -> bool {
        self.payload.report()
    }
}

impl fmt::Debug for CleanupIncident {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CleanupIncident")
            .field("ordinal", &self.ordinal)
            .field("producer", &self.producer)
            .field("origin", &self.origin)
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

/// Execution location correlated with a cleanup incident.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CleanupIncidentOrigin {
    /// Cleanup within a synchronous callback boundary.
    SynchronousRoot,
    /// Cleanup within a protected frame at its retained state.
    ProtectedFrame {
        /// The frame that ran cleanup.
        frame: ProtectedAsyncFrameId,
        /// The retained state that produced the incident.
        state: ProtectedFrameStateId,
    },
}

impl CleanupIncidentOrigin {
    /// Creates a cleanup origin from its frame and retained state.
    pub const fn new(frame: ProtectedAsyncFrameId, state: ProtectedFrameStateId) -> Self {
        Self::ProtectedFrame { frame, state }
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
    incidents: VecDeque<CleanupIncident>,
}

impl CleanupReportSink {
    /// Creates an empty cleanup-report sink.
    pub fn new() -> Self {
        Self::default()
    }

    /// Transfers one owned cleanup failure to the product host.
    pub fn transfer(
        &self,
        producer: CleanupIncidentProducer,
        origin: CleanupIncidentOrigin,
        payload: impl Any + Send,
    ) {
        self.transfer_owned(
            producer,
            origin,
            OwnedCleanupIncident::host(Box::new(payload)),
        );
    }

    pub(crate) fn transfer_owned(
        &self,
        producer: CleanupIncidentProducer,
        origin: CleanupIncidentOrigin,
        payload: OwnedCleanupIncident,
    ) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let ordinal = state.next_ordinal;

        state.next_ordinal = state.next_ordinal.saturating_add(1);

        state
            .incidents
            .push_back(CleanupIncident::new(ordinal, producer, origin, payload));
    }

    /// Reports and removes every incident in transfer order.
    pub fn drain(&self, mut report: impl FnMut(CleanupIncident)) {
        loop {
            let incident = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .incidents
                .pop_front();

            let Some(incident) = incident else {
                return;
            };

            report(incident);
        }
    }

    /// Returns the number of incidents awaiting product-host reporting.
    pub fn pending_count(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .incidents
            .len()
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
    fn product_shutdown_maps_then_reports_then_stops_infrastructure() {
        let reports = CleanupReportSink::new();

        let origin = CleanupIncidentOrigin::new(
            bray_runtime_model::ProtectedAsyncFrameId::new([9; 32]),
            bray_runtime_model::ProtectedFrameStateId::new(3),
        );

        reports.transfer(CleanupIncidentProducer::SynchronousRoot, origin, "first");

        reports.transfer(CleanupIncidentProducer::SynchronousRoot, origin, "second");

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
