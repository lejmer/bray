use std::sync::Arc;

use bray_base::{Cancellation, NonEmptySharedStr};
use bray_diagnostics::DiagnosticBag;

use crate::outcome::failed_outcome;
use crate::{LinkFailure, LinkOutcome, LinkPlan, LinkerDriverCapabilities};

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
    capability_revision: NonEmptySharedStr,
    toolchain_revision: NonEmptySharedStr,
}

impl LinkerDriverIdentity {
    /// Creates a driver identity when every canonical revision component is non-empty.
    pub fn try_new(
        kind: LinkerDriverKind,
        name: impl Into<Arc<str>>,
        capability_revision: impl Into<Arc<str>>,
        toolchain_revision: impl Into<Arc<str>>,
    ) -> Option<Self> {
        Some(Self {
            kind,
            name: NonEmptySharedStr::try_new(name)?,
            capability_revision: NonEmptySharedStr::try_new(capability_revision)?,
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

    /// Returns the revision of the published capability contract.
    pub fn capability_revision(&self) -> &str {
        self.capability_revision.as_str()
    }

    /// Returns the compatible linker or archiver toolchain revision.
    pub fn toolchain_revision(&self) -> &str {
        self.toolchain_revision.as_str()
    }
}

/// Target-specific translation and invocation for one linker or archiver.
pub trait LinkerDriver: Send + Sync {
    /// Returns the complete immutable capability record without probing ambient tools.
    fn capabilities(&self) -> &LinkerDriverCapabilities;

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

        drivers.sort_unstable_by(|left, right| {
            left.capabilities()
                .identity()
                .cmp(right.capabilities().identity())
        });

        if drivers
            .windows(2)
            .any(|pair| pair[0].capabilities().identity() == pair[1].capabilities().identity())
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
            .binary_search_by(|driver| driver.capabilities().identity().cmp(plan.driver()))
            .ok()
            .map(|index| self.drivers[index].as_ref())
            .ok_or(LinkFailure::DriverUnavailable)?;

        driver
            .capabilities()
            .validate(plan)
            .map_err(LinkFailure::UnsupportedRequirement)?;

        Ok(driver)
    }

    /// Selects a driver only after constructing and validating its complete immutable plan.
    pub fn select_plan<E>(
        &self,
        mut construct: impl FnMut(&LinkerDriverIdentity) -> Result<LinkPlan, E>,
    ) -> Result<LinkPlan, LinkPlanSelectionError<E>> {
        let mut first_unsupported = None;

        for driver in self.drivers.iter() {
            let capabilities = driver.capabilities();

            let plan =
                construct(capabilities.identity()).map_err(LinkPlanSelectionError::Construction)?;

            if plan.driver() != capabilities.identity() {
                return Err(LinkPlanSelectionError::Link(
                    LinkFailure::DriverIncompatible,
                ));
            }

            match capabilities.validate(&plan) {
                Ok(()) => return Ok(plan),
                Err(unsupported) if first_unsupported.is_none() => {
                    first_unsupported = Some(unsupported);
                }
                Err(_) => {}
            }
        }

        Err(LinkPlanSelectionError::Link(
            first_unsupported.map_or(LinkFailure::DriverUnavailable, |unsupported| {
                LinkFailure::UnsupportedRequirement(unsupported)
            }),
        ))
    }

    /// Returns configured driver identities in deterministic selection order.
    pub fn driver_identities(&self) -> impl Iterator<Item = &LinkerDriverIdentity> {
        self.drivers
            .iter()
            .map(|driver| driver.capabilities().identity())
    }

    /// Returns complete driver capability records in deterministic selection order.
    pub fn driver_capabilities(&self) -> impl Iterator<Item = &LinkerDriverCapabilities> {
        self.drivers.iter().map(|driver| driver.capabilities())
    }

    /// Links one plan through its selected driver.
    pub fn link(&self, plan: &LinkPlan, cancellation: &dyn Cancellation) -> LinkOutcome {
        if cancellation.is_cancelled() {
            return LinkOutcome::cancelled(DiagnosticBag::new());
        }

        let driver = match self.select(plan) {
            Ok(driver) => driver,
            Err(failure) => {
                return failed_outcome(plan, failure);
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

/// A failure to construct or select one complete immutable link plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkPlanSelectionError<E> {
    /// Complete plan construction failed independently of driver capabilities.
    Construction(E),
    /// No configured driver accepted the complete plan.
    Link(LinkFailure),
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use super::{Linker, LinkerBuildError, LinkerDriver, LinkerDriverIdentity, LinkerDriverKind};
    use crate::test_support::{link_plan, link_plan_with_driver};
    use crate::{
        DeadStripPolicy, DebugLinkPolicy, LinkCancellationCapability, LinkDeterminismCapability,
        LinkEnvironmentCapability, LinkFailure, LinkInputKind, LinkInputMode, LinkModel,
        LinkOutcome, LinkPlan, LinkPlanCapability, LinkResponseFileCapability, LinkStartupMode,
        LinkStatus, LinkedArtifactKind, LinkedProductKind, LinkerDriverCapabilities,
        LinkerOperationalCapabilities, LinkerTargetCapabilities, SectionGarbageCollectionPolicy,
    };
    use bray_base::Cancellation;
    use bray_diagnostics::DiagnosticBag;
    use bray_target::{ObjectFormat, TargetArchitecture};

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
        assert_eq!(identity.capability_revision(), "1");
        assert_eq!(identity.toolchain_revision(), "20");
    }

    #[test]
    fn linker_selects_exact_embedded_and_configured_drivers() {
        for kind in [LinkerDriverKind::EmbeddedLld, LinkerDriverKind::System] {
            let identity = LinkerDriverIdentity::try_new(kind, "test", "1", "1")
                .unwrap_or_else(|| panic!("test driver identity should be valid"));

            let plan = link_plan_with_driver(identity.clone());
            let calls = Arc::new(AtomicUsize::new(0));

            let driver = Arc::new(TestDriver::new(identity, true, Arc::clone(&calls), None));

            let linker = Linker::try_new([driver as Arc<dyn LinkerDriver>])
                .unwrap_or_else(|error| panic!("test driver set should be valid: {error:?}"));

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
            &LinkStatus::Failed(LinkFailure::UnsupportedRequirement(
                crate::UnsupportedLinkRequirement::Target {
                    identity: plan.target().identity().clone(),
                    triple: Arc::from(plan.target().triple()),
                    architecture: TargetArchitecture::X86_64,
                    object_format: ObjectFormat::Elf,
                }
            ))
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
    fn linker_selects_only_after_complete_plan_validation() {
        let first_identity = LinkerDriverIdentity::try_new(
            LinkerDriverKind::System,
            "a-incompatible-startup",
            "1",
            "1",
        )
        .unwrap_or_else(|| panic!("test driver identity must be valid"));

        let second_identity =
            LinkerDriverIdentity::try_new(LinkerDriverKind::System, "b-compatible-plan", "1", "1")
                .unwrap_or_else(|| panic!("test driver identity must be valid"));

        let first = Arc::new(TestDriver {
            capabilities: test_capabilities_with_startup(
                first_identity,
                true,
                LinkStartupMode::ExplicitInputs,
            ),
            calls: Arc::new(AtomicUsize::new(0)),
            cancel_during_link: None,
        });

        let second = Arc::new(TestDriver {
            capabilities: test_capabilities_with_startup(
                second_identity.clone(),
                true,
                LinkStartupMode::PlatformCompilerDriver,
            ),
            calls: Arc::new(AtomicUsize::new(0)),
            cancel_during_link: None,
        });

        let linker = Linker::try_new([
            first as Arc<dyn LinkerDriver>,
            second as Arc<dyn LinkerDriver>,
        ])
        .unwrap_or_else(|error| panic!("test linker must be valid: {error:?}"));

        let selected = linker
            .select_plan(|identity| Ok::<_, ()>(link_plan_with_driver(identity.clone())))
            .unwrap_or_else(|error| panic!("one complete plan must be supported: {error:?}"));

        assert_eq!(selected.driver(), &second_identity);
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

        let outcome = linker.link(&plan, &TestCancellation(Arc::clone(&cancellation)));

        assert_eq!(outcome.status(), &LinkStatus::Cancelled);
    }

    struct TestDriver {
        capabilities: LinkerDriverCapabilities,
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
                capabilities: test_capabilities(identity, supported),
                calls,
                cancel_during_link,
            }
        }
    }

    impl LinkerDriver for TestDriver {
        fn capabilities(&self) -> &LinkerDriverCapabilities {
            &self.capabilities
        }

        fn link(&self, plan: &LinkPlan, _cancellation: &dyn Cancellation) -> LinkOutcome {
            self.calls.fetch_add(1, Ordering::Release);

            if let Some(cancellation) = &self.cancel_during_link {
                cancellation.store(true, Ordering::Release);
            }

            let failure = LinkFailure::Invocation;

            LinkOutcome::failed(plan, failure, DiagnosticBag::new())
        }
    }

    fn test_capabilities(
        identity: LinkerDriverIdentity,
        supported: bool,
    ) -> LinkerDriverCapabilities {
        test_capabilities_with_startup(identity, supported, LinkStartupMode::PlatformCompilerDriver)
    }

    fn test_capabilities_with_startup(
        identity: LinkerDriverIdentity,
        supported: bool,
        startup: LinkStartupMode,
    ) -> LinkerDriverCapabilities {
        let (architecture, object_format) = if supported {
            (TargetArchitecture::X86_64, ObjectFormat::Elf)
        } else {
            (TargetArchitecture::Wasm32, ObjectFormat::WebAssembly)
        };

        let target = LinkerTargetCapabilities::new(
            architecture,
            object_format,
            [
                LinkPlanCapability::Product(LinkedProductKind::Executable),
                LinkPlanCapability::Input(LinkInputKind::RelocatableObject),
                LinkPlanCapability::InputMode(LinkInputMode::Ordinary),
                LinkPlanCapability::Output(LinkedArtifactKind::Executable),
                LinkPlanCapability::LinkModel(LinkModel::Dynamic),
                LinkPlanCapability::DeadStrip(DeadStripPolicy::Preserve),
                LinkPlanCapability::SectionGarbageCollection(
                    SectionGarbageCollectionPolicy::Preserve,
                ),
                LinkPlanCapability::Debug(DebugLinkPolicy::None),
                LinkPlanCapability::Symbol(crate::LinkSymbolRequirement::EntryPoint),
                LinkPlanCapability::Startup(startup),
            ],
        );

        LinkerDriverCapabilities::try_new(
            identity,
            [target],
            LinkerOperationalCapabilities::new(
                LinkResponseFileCapability::InlineArguments,
                LinkEnvironmentCapability::NotApplicable,
                LinkCancellationCapability::Cooperative,
                LinkDeterminismCapability::ToolchainDependent,
            ),
        )
        .unwrap_or_else(|error| panic!("test capabilities must be valid: {error:?}"))
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
