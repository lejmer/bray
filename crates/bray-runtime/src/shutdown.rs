use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::{RunOutcome, RuntimePanic};

/// A cleanup failure transferred to the product host for reporting.
#[derive(Debug)]
pub struct CleanupIncident {
    panic: RuntimePanic,
}

impl CleanupIncident {
    /// Creates a host-owned cleanup incident.
    pub const fn new(panic: RuntimePanic) -> Self {
        Self { panic }
    }

    /// Returns the retained cleanup panic.
    pub const fn panic(&self) -> &RuntimePanic {
        &self.panic
    }
}

/// Mandatory product-host sink for cleanup incidents not attached to a returned panic.
#[derive(Clone, Debug, Default)]
pub struct CleanupReportSink {
    incidents: Arc<Mutex<VecDeque<CleanupIncident>>>,
}

impl CleanupReportSink {
    /// Creates an empty cleanup-report sink.
    pub fn new() -> Self {
        Self::default()
    }

    /// Transfers one cleanup incident to the product host.
    pub fn transfer(&self, incident: CleanupIncident) {
        self.incidents
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push_back(incident);
    }

    /// Reports and removes every incident in transfer order.
    pub fn drain(&self, mut report: impl FnMut(CleanupIncident)) {
        loop {
            let incident = self
                .incidents
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .pop_front();

            let Some(incident) = incident else {
                return;
            };

            report(incident);
        }
    }

    /// Returns the number of incidents awaiting product-host reporting.
    pub fn pending_count(&self) -> usize {
        self.incidents
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
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
        CleanupIncident, CleanupReportSink, finish_product_shutdown,
    };
    use crate::{RunOutcome, RuntimePanic};

    #[test]
    fn product_shutdown_maps_then_reports_then_stops_infrastructure() {
        let reports = CleanupReportSink::new();

        reports.transfer(CleanupIncident::new(RuntimePanic::new("first")));
        reports.transfer(CleanupIncident::new(RuntimePanic::new("second")));

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
            |_| events.borrow_mut().push("report"),
            || {
                events.borrow_mut().push("shutdown");

                Ok::<_, ()>(())
            },
        )
        .unwrap_or_else(|()| panic!("runtime shutdown must succeed"));

        assert_eq!(result, 7);

        assert_eq!(
            events.into_inner(),
            ["map", "report", "report", "shutdown"]
        );

        assert_eq!(reports.pending_count(), 0);
    }
}
