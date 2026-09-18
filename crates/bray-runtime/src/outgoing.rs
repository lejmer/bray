use std::collections::TryReserveError;
use std::sync::{Mutex, MutexGuard};

#[derive(Default)]
struct Record {
    primary: Option<bray_runtime_abi::NativePanicPrimary>,
    next: Option<usize>,
    incident: Option<(crate::shutdown::CleanupIncidentMetadata, usize, usize)>,
}

#[derive(Default)]
struct Records {
    slots: Vec<Record>,
    free: Option<usize>,
    free_count: usize,
    source_credits: usize,
}

// Reports outlive their producing tasks and cross native threads. Their linked runtime
// therefore owns the record provider until the final report releases its records.
static RECORDS: Mutex<Records> = Mutex::new(Records {
    slots: Vec::new(),
    free: None,
    free_count: 0,
    source_credits: 0,
});

/// Detached record ownership. Empty records are admitted before their producer is accepted.
#[derive(Debug, Default)]
pub(crate) struct OutgoingRecords {
    head: Option<usize>,
    tail: Option<usize>,
    count: usize,
}

impl OutgoingRecords {
    pub(crate) fn admit(count: usize) -> Result<Self, TryReserveError> {
        #[cfg(test)]
        if let Some(error) = tests::admission_failure() {
            return Err(error);
        }

        let mut records = records();
        let required = records.source_credits.saturating_add(count);

        records.reserve_free(required)?;

        let mut admitted = Self::default();

        for _ in 0..count {
            let index = records.take_free();

            admitted.push_record(&mut records, index);
        }

        Ok(admitted)
    }

    pub(crate) fn admit_source(count: usize) -> Result<(), TryReserveError> {
        #[cfg(test)]
        if let Some(error) = tests::admission_failure() {
            return Err(error);
        }

        let mut records = records();
        let required = records.source_credits.saturating_add(count);

        records.reserve_free(required)?;

        records.source_credits = required;

        Ok(())
    }

    pub(crate) fn discharge_source(count: usize) {
        let mut records = records();

        assert!(
            count <= records.source_credits,
            "only accepted owner credits can be discharged"
        );

        records.source_credits -= count;
    }

    pub(crate) fn activate_source() -> Self {
        let mut records = records();
        let index = records.take_free();

        let mut admitted = Self::default();
        admitted.push_record(&mut records, index);

        admitted
    }

    pub(crate) fn take(&mut self, count: usize) -> Self {
        let mut records = records();
        let mut taken = Self::default();

        for _ in 0..count {
            let index = self
                .detach_front(&mut records)
                .unwrap_or_else(|| unreachable!("the admitted bridge owns its records"));

            taken.push_record(&mut records, index);
        }

        taken
    }

    pub(crate) fn into_parts(mut self) -> (usize, usize, usize) {
        let parts = (
            self.head.map_or(0, |id| id + 1),
            self.tail.map_or(0, |id| id + 1),
            self.count,
        );

        self.head = None;
        self.tail = None;
        self.count = 0;

        parts
    }

    pub(crate) fn from_parts(head: usize, tail: usize, count: usize) -> Self {
        Self {
            head: head.checked_sub(1),
            tail: tail.checked_sub(1),
            count,
        }
    }

    pub(crate) const fn len(&self) -> usize {
        self.count
    }

    pub(crate) fn push(
        &mut self,
        primary: bray_runtime_abi::NativePanicPrimary,
        admitted: &mut Self,
    ) {
        let mut provider = records();

        let index = admitted
            .detach_front(&mut provider)
            .unwrap_or_else(|| unreachable!("an accepted producer owns its outgoing record"));

        let reserved = provider.slots[index].primary.replace(primary);

        drop(provider);
        drop(reserved);

        let mut records = records();

        self.push_record(&mut records, index);
    }

    pub(crate) fn push_rust(
        &mut self,
        payload: Box<dyn std::any::Any + Send>,
        admitted: &mut Self,
    ) {
        let mut records = records();

        let index = admitted
            .detach_front(&mut records)
            .unwrap_or_else(|| unreachable!("an accepted producer owns its outgoing record"));

        let primary = records.slots[index]
            .primary
            .as_mut()
            .unwrap_or_else(|| unreachable!("an admitted record owns Rust callback backing"));

        crate::frame::attach_rust_panic_payload(primary, payload);

        self.push_record(&mut records, index);
    }

    pub(crate) fn take_rust_primary(
        &mut self,
        payload: Box<dyn std::any::Any + Send>,
    ) -> (bray_runtime_abi::NativePanicPrimary, Self) {
        let mut records = records();

        let index = self
            .detach_front(&mut records)
            .unwrap_or_else(|| unreachable!("an accepted producer owns Rust callback backing"));

        let mut primary = records.slots[index]
            .primary
            .take()
            .unwrap_or_else(|| unreachable!("an admitted record owns Rust callback backing"));

        let mut reserved = Self::default();
        reserved.push_record(&mut records, index);

        drop(records);

        crate::frame::attach_rust_panic_payload(&mut primary, payload);

        (primary, reserved)
    }

    pub(crate) fn append(&mut self, other: &mut Self) {
        let Some(head) = other.head.take() else {
            return;
        };

        if let Some(tail) = self.tail {
            records().slots[tail].next = Some(head);
        } else {
            self.head = Some(head);
        }

        self.tail = other.tail.take();
        self.count += std::mem::take(&mut other.count);
    }

    pub(crate) fn pop(&mut self) -> Option<bray_runtime_abi::NativePanicPrimary> {
        let mut records = records();
        let index = self.detach_front(&mut records)?;

        let primary = records.slots[index].primary.take();

        records.slots[index].incident = None;
        records.slots[index].next = records.free;
        records.free = Some(index);
        records.free_count += 1;

        primary
    }

    pub(crate) fn push_incident(
        &mut self,
        panic: crate::RuntimePanic,
        metadata: crate::shutdown::CleanupIncidentMetadata,
        admitted: &mut Self,
    ) {
        let mut incident = panic.into_records(admitted);

        let (Some(head), Some(tail)) = (incident.head, incident.tail) else {
            unreachable!("a transferred cleanup report owns its primary");
        };

        records().slots[head].incident = Some((metadata, tail, incident.count));

        self.append(&mut incident);
    }

    pub(crate) fn pop_incident(&mut self) -> Option<crate::shutdown::CleanupIncident> {
        let mut records = records();
        let head = self.head?;

        let (metadata, tail, count) = records.slots[head]
            .incident
            .take()
            .unwrap_or_else(|| unreachable!("a cleanup segment starts with its metadata"));

        self.head = records.slots[tail].next.take();
        self.count -= count;

        if self.head.is_none() {
            self.tail = None;
        }

        drop(records);

        let incident = Self {
            head: Some(head),
            tail: Some(tail),
            count,
        };

        Some(crate::shutdown::CleanupIncident::new(
            metadata,
            crate::RuntimePanic::from_records(incident),
        ))
    }

    fn push_record(&mut self, records: &mut Records, index: usize) {
        if let Some(tail) = self.tail {
            records.slots[tail].next = Some(index);
        } else {
            self.head = Some(index);
        }

        self.tail = Some(index);
        self.count += 1;
    }

    fn detach_front(&mut self, records: &mut Records) -> Option<usize> {
        let index = self.head.take()?;

        self.head = records.slots[index].next.take();
        self.count -= 1;

        if self.head.is_none() {
            self.tail = None;
        }

        Some(index)
    }
}

impl Drop for OutgoingRecords {
    #[inline(never)]
    fn drop(&mut self) {
        while self.head.is_some() {
            drop(self.pop());
        }
    }
}

impl Records {
    fn reserve_free(&mut self, count: usize) -> Result<(), TryReserveError> {
        let additional = count.saturating_sub(self.free_count);

        crate::frame::reserve_rust_panic_backings(count)?;

        self.slots.try_reserve(additional)?;

        for _ in 0..additional {
            let index = self.slots.len();

            self.slots.push(Record {
                primary: None,
                next: self.free,
                incident: None,
            });

            self.free = Some(index);
        }

        self.free_count += additional;

        Ok(())
    }

    fn take_free(&mut self) -> usize {
        let Some(index) = self.free else {
            unreachable!("an accepted obligation must retain an outgoing record");
        };

        self.free = self.slots[index].next.take();
        self.free_count -= 1;
        self.slots[index].primary = Some(crate::frame::reserved_rust_panic_primary());

        index
    }
}

fn records() -> MutexGuard<'static, Records> {
    RECORDS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
pub(crate) mod tests {
    use std::cell::Cell;
    use std::collections::TryReserveError;

    thread_local! {
        static REJECT_ADMISSION: Cell<bool> = const { Cell::new(false) };
    }

    pub(crate) struct AdmissionFailure(bool);

    pub(crate) fn reject_admission() -> AdmissionFailure {
        AdmissionFailure(REJECT_ADMISSION.replace(true))
    }

    pub(super) fn admission_failure() -> Option<TryReserveError> {
        REJECT_ADMISSION
            .get()
            .then(|| Vec::<u8>::new().try_reserve(usize::MAX).unwrap_err())
    }

    impl Drop for AdmissionFailure {
        fn drop(&mut self) {
            REJECT_ADMISSION.set(self.0);
        }
    }
}
