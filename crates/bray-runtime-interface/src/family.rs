/// One independently retainable platform capability family.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PlatformServiceFamily {
    /// Process context, clocks, and host entropy.
    Core,
    /// Standard input, standard output, and standard error.
    StandardStreams,
    /// Files, directories, and paths.
    Filesystem,
    /// Child processes and their pipes.
    Process,
    /// Operating-system thread creation and ownership.
    Thread,
    /// Calendar, timezone, and temporal representation services.
    Temporal,
    /// Dynamic-library loading and symbol lookup.
    DynamicLibrary,
}
