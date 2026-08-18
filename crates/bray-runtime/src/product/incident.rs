use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};

use bray_runtime_abi::NativeCleanupIncident;

pub(crate) struct CleanupIncident {
    kind: CleanupIncidentKind,
}

enum CleanupIncidentKind {
    Native(Option<NativeCleanupIncident>),
    Panic { _payload: Box<dyn Any + Send> },
    RuntimeFailure,
}

impl CleanupIncident {
    pub(crate) fn native(incident: NativeCleanupIncident) -> Option<Self> {
        incident.is_valid().then_some(Self {
            kind: CleanupIncidentKind::Native(Some(incident)),
        })
    }

    pub(crate) fn panic(payload: Box<dyn Any + Send>) -> Self {
        Self {
            kind: CleanupIncidentKind::Panic { _payload: payload },
        }
    }

    pub(crate) const fn runtime_failure() -> Self {
        Self {
            kind: CleanupIncidentKind::RuntimeFailure,
        }
    }

    pub(super) fn report(mut self) -> bool {
        let CleanupIncidentKind::Native(incident) = &mut self.kind else {
            return false;
        };

        let Some(incident) = incident.take() else {
            return true;
        };

        let reporting_failed =
            catch_unwind(AssertUnwindSafe(|| (incident.report())(incident.payload())))
                .map_or(true, |status| !status.is_success());

        let destruction_failed = catch_unwind(AssertUnwindSafe(|| {
            (incident.destroy())(incident.payload());
        }))
        .is_err();

        reporting_failed || destruction_failed
    }
}

impl Drop for CleanupIncident {
    fn drop(&mut self) {
        let CleanupIncidentKind::Native(incident) = &mut self.kind else {
            return;
        };

        if let Some(incident) = incident.take() {
            let _ = catch_unwind(AssertUnwindSafe(|| {
                (incident.destroy())(incident.payload());
            }));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bray_runtime_abi::{
        NativeCleanupIncident, NativeRuntimeStatus, NativeSourceAnchor, NativeTypeIdentity,
    };

    use super::CleanupIncident;

    static REPORTS: AtomicUsize = AtomicUsize::new(0);
    static DESTROYS: AtomicUsize = AtomicUsize::new(0);

    extern "C-unwind" fn report(payload: usize) -> NativeRuntimeStatus {
        REPORTS.fetch_add(payload, Ordering::SeqCst);

        NativeRuntimeStatus::SUCCESS
    }

    extern "C-unwind" fn destroy(payload: usize) {
        DESTROYS.fetch_add(payload, Ordering::SeqCst);
    }

    fn native_incident(payload: usize) -> CleanupIncident {
        CleanupIncident::native(NativeCleanupIncident::new(
            payload,
            NativeTypeIdentity::new([7; 32]),
            NativeSourceAnchor::unavailable(),
            report,
            destroy,
        ))
        .unwrap_or_else(|| panic!("test incident must satisfy the native contract"))
    }

    #[test]
    fn reporting_borrows_then_destroys_the_owned_payload_once() {
        REPORTS.store(0, Ordering::SeqCst);
        DESTROYS.store(0, Ordering::SeqCst);

        assert!(!native_incident(3).report());
        assert_eq!(REPORTS.load(Ordering::SeqCst), 3);
        assert_eq!(DESTROYS.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn dropping_an_unreported_incident_releases_its_payload_once() {
        DESTROYS.store(0, Ordering::SeqCst);

        drop(native_incident(5));

        assert_eq!(DESTROYS.load(Ordering::SeqCst), 5);
    }
}
