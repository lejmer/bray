/// Dead-code removal policy selected for one native link operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DeadStripPolicy {
    /// Preserve otherwise unreferenced link inputs and symbols.
    Preserve,
    /// Permit the driver to remove unreachable link content.
    RemoveUnreachable,
}

/// Section-level garbage-collection policy.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SectionGarbageCollectionPolicy {
    /// Preserve unreferenced sections.
    Preserve,
    /// Permit removal of unreferenced sections.
    RemoveUnreferenced,
}

/// Debug-information placement selected for a linked product.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DebugLinkPolicy {
    /// Do not request linked debug information.
    None,
    /// Retain debug information in the primary linked artifact.
    Embedded,
    /// Produce a separately staged debug companion.
    Companion,
}

/// Target subsystem selected before linker-driver translation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LinkSubsystem {
    /// Console-oriented executable environment.
    Console,
    /// Windowed application environment.
    Windowed,
    /// Platform-native service or kernel environment.
    Native,
    /// WebAssembly command environment.
    WasiCommand,
    /// WebAssembly reactor environment.
    WasiReactor,
}

/// Typed target-independent link policy.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LinkPolicy {
    dead_strip: DeadStripPolicy,
    section_garbage_collection: SectionGarbageCollectionPolicy,
    debug: DebugLinkPolicy,
    subsystem: Option<LinkSubsystem>,
}

impl LinkPolicy {
    /// Creates one complete policy selected before driver translation.
    pub const fn new(
        dead_strip: DeadStripPolicy,
        section_garbage_collection: SectionGarbageCollectionPolicy,
        debug: DebugLinkPolicy,
        subsystem: Option<LinkSubsystem>,
    ) -> Self {
        Self {
            dead_strip,
            section_garbage_collection,
            debug,
            subsystem,
        }
    }

    /// Returns the selected dead-code removal policy.
    pub const fn dead_strip(self) -> DeadStripPolicy {
        self.dead_strip
    }

    /// Returns the selected section garbage-collection policy.
    pub const fn section_garbage_collection(self) -> SectionGarbageCollectionPolicy {
        self.section_garbage_collection
    }

    /// Returns the selected debug-information placement.
    pub const fn debug(self) -> DebugLinkPolicy {
        self.debug
    }

    /// Returns the selected target subsystem when one is required.
    pub const fn subsystem(self) -> Option<LinkSubsystem> {
        self.subsystem
    }
}
