use std::sync::Arc;

use bray_base::{Cancellation, NonEmptySharedStr};
use bray_diagnostics::DiagnosticBag;

use crate::outcome::failed_outcome;
use crate::{LinkFailure, LinkOutcome, LinkPlan, LinkTarget, LinkedProductKind};

/// Supported category of one selected linker or archiver driver.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LinkerDriverKind {
    /// Linker embedded in the compiler process.
    EmbeddedLld,
    /// Explicit external LLD executable.
    ExternalLld,
    /// Configured platform system linker.
    System,
    /// Static-library archiver.
    Archiver,
    /// Explicit target-specific driver.
    TargetSpecific,
}

/// Stable identity of one selected linker driver and compatible toolchain.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LinkerDriverIdentity {
    kind: LinkerDriverKind,
    name: NonEmptySharedStr,
    revision: NonEmptySharedStr,
    toolchain_revision: NonEmptySharedStr,
}

impl LinkerDriverIdentity {
    /// Creates a driver identity when every canonical revision component is non-empty.
    pub fn try_new(
        kind: LinkerDriverKind,
        name: impl Into<Arc<str>>,
        revision: impl Into<Arc<str>>,
        toolchain_revision: impl Into<Arc<str>>,
    ) -> Option<Self> {
        Some(Self {
            kind,
            name: NonEmptySharedStr::try_new(name)?,
            revision: NonEmptySharedStr::try_new(revision)?,
            toolchain_revision: NonEmptySharedStr::try_new(toolchain_revision)?,
        })
    }

    /// Returns the selected driver category.
    pub const fn kind(&self) -> LinkerDriverKind {
        self.kind
    }

    /// Returns the stable driver name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Returns the Bray driver implementation revision.
    pub fn revision(&self) -> &str {
        self.revision.as_str()
    }

    /// Returns the compatible linker or archiver toolchain revision.
    pub fn toolchain_revision(&self) -> &str {
        self.toolchain_revision.as_str()
    }
}

/// Target-specific translation and invocation for one linker or archiver.
pub trait LinkerDriver: Send + Sync {
    /// Returns the exact driver and toolchain identity.
    fn identity(&self) -> &LinkerDriverIdentity;

    /// Returns whether the driver supports the selected target and product.
    fn supports(&self, target: &LinkTarget, product: LinkedProductKind) -> bool;

    /// Links one validated plan without publishing final destinations.
    fn link(&self, plan: &LinkPlan, cancellation: &dyn Cancellation) -> LinkOutcome;
}

/// Immutable set of linker drivers available from one compiler host.
pub struct Linker {
    drivers: Arc<[Arc<dyn LinkerDriver>]>,
}

impl Linker {
    /// Creates a linker after rejecting duplicate driver identities.
    pub fn try_new(
        drivers: impl IntoIterator<Item = Arc<dyn LinkerDriver>>,
    ) -> Result<Self, LinkerBuildError> {
        let mut drivers: Vec<_> = drivers.into_iter().collect();

        drivers.sort_unstable_by(|left, right| left.identity().cmp(right.identity()));

        if drivers
            .windows(2)
            .any(|pair| pair[0].identity() == pair[1].identity())
        {
            return Err(LinkerBuildError::DuplicateDriver);
        }

        Ok(Self {
            drivers: drivers.into(),
        })
    }

    /// Selects the exact planned driver after validating target and product support.
    pub fn select(&self, plan: &LinkPlan) -> Result<&dyn LinkerDriver, LinkFailure> {
        let driver = self
            .drivers
            .binary_search_by(|driver| driver.identity().cmp(plan.driver()))
            .ok()
            .map(|index| self.drivers[index].as_ref())
            .ok_or(LinkFailure::DriverUnavailable)?;

        if !driver.supports(plan.target(), plan.product_kind()) {
            return Err(LinkFailure::DriverIncompatible);
        }

        Ok(driver)
    }

    /// Selects the first compatible driver in canonical identity order.
    pub fn select_identity(
        &self,
        target: &LinkTarget,
        product: LinkedProductKind,
    ) -> Result<&LinkerDriverIdentity, LinkFailure> {
        self.drivers
            .iter()
            .find(|driver| driver.supports(target, product))
            .map(|driver| driver.identity())
            .ok_or(LinkFailure::DriverUnavailable)
    }

    /// Links one plan through its selected driver.
    pub fn link(&self, plan: &LinkPlan, cancellation: &dyn Cancellation) -> LinkOutcome {
        if cancellation.is_cancelled() {
            return LinkOutcome::cancelled(DiagnosticBag::new());
        }

        let driver = match self.select(plan) {
            Ok(driver) => driver,
            Err(failure) => {
                return failed_outcome(failure);
            }
        };

        let outcome = driver.link(plan, cancellation);

        if cancellation.is_cancelled() {
            return LinkOutcome::cancelled(outcome.into_diagnostics());
        }

        outcome
    }
}

/// A contract violation in the compiler-host driver set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkerBuildError {
    /// More than one driver has the same exact identity.
    DuplicateDriver,
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;

    use bray_base::Cancellation;
    use bray_diagnostics::DiagnosticBag;

    use super::{
        Linker, LinkerDriver, LinkerDriverIdentity, LinkerDriverKind,
        LinkerBuildError,
    };
    use crate::test_support::{link_plan, link_plan_with_driver};
    use crate::{
        LinkFailure, LinkOutcome, LinkPlan, LinkStatus, LinkTarget,
        LinkedProductKind,
    };

    #[test]
    fn driver_identities_require_complete_revision_metadata() {
        assert_eq!(
            LinkerDriverIdentity::try_new(LinkerDriverKind::EmbeddedLld, "", "1", "20"),
            None
        );

        assert_eq!(
            LinkerDriverIdentity::try_new(LinkerDriverKind::EmbeddedLld, "lld", "", "20"),
            None
        );

        assert_eq!(
            LinkerDriverIdentity::try_new(LinkerDriverKind::EmbeddedLld, "lld", "1", ""),
            None
        );

        let Some(identity) =
            LinkerDriverIdentity::try_new(LinkerDriverKind::EmbeddedLld, "lld", "1", "20")
        else {
            panic!("complete test driver identity must be valid");
        };

        assert_eq!(identity.kind(), LinkerDriverKind::EmbeddedLld);
        assert_eq!(identity.name(), "lld");
        assert_eq!(identity.revision(), "1");
        assert_eq!(identity.toolchain_revision(), "20");
    }

    #[test]
    fn linker_selects_exact_embedded_and_configured_drivers() {
        for kind in [
            LinkerDriverKind::EmbeddedLld,
            LinkerDriverKind::System,
        ] {
            let identity =
                LinkerDriverIdentity::try_new(kind, "test", "1", "1")
                    .unwrap_or_else(|| {
                        panic!("test driver identity should be valid")
                    });

            let plan = link_plan_with_driver(identity.clone());
            let calls = Arc::new(AtomicUsize::new(0));

            let driver = Arc::new(TestDriver::new(
                identity,
                true,
                Arc::clone(&calls),
                None,
            ));

            let linker =
                Linker::try_new([driver as Arc<dyn LinkerDriver>])
                    .unwrap_or_else(|error| {
                        panic!(
                            "test driver set should be valid: {error:?}"
                        )
                    });

            let outcome = linker.link(&plan, &|| false);

            assert_eq!(
                outcome.status(),
                &LinkStatus::Failed(LinkFailure::Invocation)
            );

            assert_eq!(calls.load(Ordering::Acquire), 1);
        }
    }

    #[test]
    fn linker_distinguishes_unavailable_and_incompatible_drivers() {
        let plan = link_plan();

        let unavailable = Linker::try_new([])
            .unwrap_or_else(|error| panic!("empty test driver set should be valid: {error:?}"));

        assert_eq!(
            unavailable.link(&plan, &|| false).status(),
            &LinkStatus::Failed(LinkFailure::DriverUnavailable)
        );

        let driver = Arc::new(TestDriver::new(
            plan.driver().clone(),
            false,
            Arc::new(AtomicUsize::new(0)),
            None,
        ));

        let incompatible = Linker::try_new([driver as Arc<dyn LinkerDriver>])
            .unwrap_or_else(|error| panic!("test driver set should be valid: {error:?}"));

        assert_eq!(
            incompatible.link(&plan, &|| false).status(),
            &LinkStatus::Failed(LinkFailure::DriverIncompatible)
        );
    }

    #[test]
    fn linker_rejects_duplicate_exact_driver_identities() {
        let plan = link_plan();
        let first = test_driver(plan.driver().clone());
        let second = test_driver(plan.driver().clone());

        assert_eq!(
            Linker::try_new([first, second]).err(),
            Some(LinkerBuildError::DuplicateDriver)
        );
    }

    #[test]
    fn linker_discards_driver_success_after_cancellation() {
        let plan = link_plan();
        let cancellation = Arc::new(AtomicBool::new(false));

        let driver = Arc::new(TestDriver::new(
            plan.driver().clone(),
            true,
            Arc::new(AtomicUsize::new(0)),
            Some(Arc::clone(&cancellation)),
        ));

        let linker = Linker::try_new([driver as Arc<dyn LinkerDriver>])
            .unwrap_or_else(|error| panic!("test driver set should be valid: {error:?}"));

        let outcome = linker.link(
            &plan,
            &TestCancellation(Arc::clone(&cancellation)),
        );

        assert_eq!(outcome.status(), &LinkStatus::Cancelled);
    }

    struct TestDriver {
        identity: LinkerDriverIdentity,
        supported: bool,
        calls: Arc<AtomicUsize>,
        cancel_during_link: Option<Arc<AtomicBool>>,
    }

    impl TestDriver {
        fn new(
            identity: LinkerDriverIdentity,
            supported: bool,
            calls: Arc<AtomicUsize>,
            cancel_during_link: Option<Arc<AtomicBool>>,
        ) -> Self {
            Self {
                identity,
                supported,
                calls,
                cancel_during_link,
            }
        }
    }

    impl LinkerDriver for TestDriver {
        fn identity(&self) -> &LinkerDriverIdentity {
            &self.identity
        }

        fn supports(&self, _target: &LinkTarget, _product: LinkedProductKind) -> bool {
            self.supported
        }

        fn link(&self, _plan: &LinkPlan, _cancellation: &dyn Cancellation) -> LinkOutcome {
            self.calls.fetch_add(1, Ordering::Release);

            if let Some(cancellation) = &self.cancel_during_link {
                cancellation.store(true, Ordering::Release);
            }

            LinkOutcome::failed(LinkFailure::Invocation, DiagnosticBag::new())
        }
    }

    struct TestCancellation(Arc<AtomicBool>);

    impl Cancellation for TestCancellation {
        fn is_cancelled(&self) -> bool {
            self.0.load(Ordering::Acquire)
        }
    }

    fn test_driver(identity: LinkerDriverIdentity) -> Arc<dyn LinkerDriver> {
        Arc::new(TestDriver::new(
            identity,
            true,
            Arc::new(AtomicUsize::new(0)),
            None,
        ))
    }
}
