use std::any::Any;
use std::collections::VecDeque;
use std::fmt;
use std::sync::{Arc, Mutex};

use bray_runtime_model::{ProtectedAsyncFrameId, ProtectedFrameStateId};

use crate::incident::dispose_panic;
use crate::{RunOutcome, TaskId};

/// A cleanup failure transferred to the product host for reporting.
pub struct CleanupIncident {
    ordinal: u64,
    producer: CleanupIncidentProducer,
    origin: CleanupIncidentOrigin,
    payload: Option<Box<dyn Any + Send>>,
}

impl CleanupIncident {
    pub(crate) fn dispose(&mut self) -> bray_runtime_abi::NativeRuntimeStatus {
        self.payload.take().map_or(
            bray_runtime_abi::NativeRuntimeStatus::SUCCESS,
            dispose_panic,
        )
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
        self.payload
            .as_ref()
            .is_some_and(|payload| payload.is::<T>())
    }

    /// Returns the host type identity carried by the erased payload descriptor.
    pub fn payload_type_id(&self) -> std::any::TypeId {
        self.payload
            .as_ref()
            .map(|payload| payload.as_ref().type_id())
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
    incidents: VecDeque<CleanupIncident>,
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

    /// Transfers one owned cleanup failure to the product host.
    pub fn transfer(
        &self,
        producer: CleanupIncidentProducer,
        origin: CleanupIncidentOrigin,
        payload: impl Any + Send,
    ) {
        self.transfer_erased(producer, origin, Box::new(payload));
    }

    pub(crate) fn transfer_erased(
        &self,
        producer: CleanupIncidentProducer,
        origin: CleanupIncidentOrigin,
        payload: Box<dyn Any + Send>,
    ) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let ordinal = state.next_ordinal;

        state.next_ordinal = state.next_ordinal.saturating_add(1);

        state.incidents.push_back(CleanupIncident {
            ordinal,
            producer,
            origin,
            payload: Some(payload),
        });
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
            let incidents = {
                let mut state = self
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);

                if state.incidents.is_empty() {
                    state.draining = false;
                    drain.0 = None;
                    return;
                }

                std::mem::take(&mut state.incidents)
            };

            for incident in incidents {
                report(incident);
            }
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
