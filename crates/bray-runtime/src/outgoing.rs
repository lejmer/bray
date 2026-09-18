use crate::TaskStartError;

use bray_runtime_abi::{NativePanicPrimary, NativeReportRecords, NativeReportSegment, NativeRunOutcome, NativeRunState};

use crate::report_provider as provider;

/// Exclusive ownership of records supplied by the linked Bray provider.
#[derive(Debug, Default)]
pub(crate) struct OutgoingRecords(NativeReportRecords);

impl OutgoingRecords {
    pub(crate) fn admit(count: usize) -> Result<Self, TaskStartError> {
        #[cfg(test)]
        if tests::admission_failure() {
            return Err(TaskStartError::OutgoingStorageUnavailable);
        }

        let mut admitted = Self::default();

        if provider::bray_runtime_report_records_admit(count, &mut admitted.0) != 0 {
            return Err(TaskStartError::OutgoingStorageUnavailable);
        }

        crate::frame::reserve_rust_panic_backings(&mut admitted).map_err(|_| TaskStartError::OutgoingStorageUnavailable)?;

        Ok(admitted)
    }

    pub(crate) fn admit_source(count: usize) -> Result<(), TaskStartError> {
        #[cfg(test)]
        if tests::admission_failure() {
            return Err(TaskStartError::OutgoingStorageUnavailable);
        }

        let mut outcome = NativeRunOutcome::new(NativeRunState::COMPLETED, 0);
        provider::bray_runtime_outgoing_admission(count, &mut outcome);

        if outcome.state() != NativeRunState::COMPLETED {
            return Err(TaskStartError::OutgoingStorageUnavailable);
        }

        Ok(())
    }

    pub(crate) fn discharge_source(count: usize) {
        provider::bray_runtime_outgoing_discharge(count);
    }

    pub(crate) fn take(&mut self, count: usize) -> Self {
        let mut taken = Self::default();
        provider::bray_runtime_report_records_take(&mut self.0, count, &mut taken.0);

        taken
    }

    pub(crate) fn into_parts(mut self) -> (usize, usize, usize) {
        let NativeReportRecords { head, tail, count } = std::mem::take(&mut self.0);

        (head, tail, count)
    }

    pub(crate) fn from_parts(head: usize, tail: usize, count: usize) -> Self {
        Self(NativeReportRecords { head, tail, count })
    }

    pub(crate) const fn len(&self) -> usize {
        self.0.count
    }

    pub(crate) fn exchange(&mut self, primary: &mut NativePanicPrimary) {
        assert_eq!(self.len(), 1, "one owned record is required for primary exchange");
        provider::bray_runtime_report_record_exchange(self.0.head, primary);
    }

    pub(crate) fn take_rust_primary(&mut self, payload: Box<dyn std::any::Any + Send>) -> (NativePanicPrimary, Self) {
        let mut reserved = self.take(1);
        let mut primary = NativePanicPrimary::empty();
        reserved.exchange(&mut primary);
        crate::frame::attach_rust_panic_payload(&mut primary, payload);

        (primary, reserved)
    }

    pub(crate) fn append(&mut self, other: &mut Self) {
        provider::bray_runtime_report_records_append(&mut self.0, &mut other.0);
    }

    pub(crate) fn pop(&mut self) -> Option<NativePanicPrimary> {
        if self.len() == 0 {
            return None;
        }

        let mut primary = NativePanicPrimary::empty();
        provider::bray_runtime_report_records_pop(&mut self.0, &mut primary);

        Some(primary)
    }

    pub(crate) fn push_incident(&mut self, panic: crate::RuntimePanic, metadata: crate::shutdown::CleanupIncidentMetadata, admitted: &mut Self) {
        let mut incident = panic.into_records(admitted);
        provider::bray_runtime_report_segment_mark(&mut incident.0, &metadata.into_native());
        self.append(&mut incident);
    }

    pub(crate) fn pop_incident(&mut self) -> Option<crate::shutdown::CleanupIncident> {
        if self.len() == 0 {
            return None;
        }

        let mut metadata = NativeReportSegment::default();
        let mut incident = Self::default();
        provider::bray_runtime_report_segment_take(&mut self.0, &mut metadata, &mut incident.0);

        Some(crate::shutdown::CleanupIncident::new(
            crate::shutdown::CleanupIncidentMetadata::from_native(metadata),
            crate::RuntimePanic::from_records(incident),
        ))
    }
}

impl Drop for OutgoingRecords {
    fn drop(&mut self) {
        while let Some(primary) = self.pop() {
            drop(primary);
        }
    }
}
#[cfg(test)]
pub(crate) mod tests {
    use std::cell::Cell;

    thread_local! {
        static REJECT_ADMISSION: Cell<bool> = const { Cell::new(false) };
    }

    pub(crate) struct AdmissionFailure(bool);

    pub(crate) fn reject_admission() -> AdmissionFailure {
        AdmissionFailure(REJECT_ADMISSION.replace(true))
    }

    pub(super) fn admission_failure() -> bool {
        REJECT_ADMISSION.get()
    }

    impl Drop for AdmissionFailure {
        fn drop(&mut self) {
            REJECT_ADMISSION.set(self.0);
        }
    }
}
