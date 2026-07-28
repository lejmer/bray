/// Host family supplying native mechanisms to the current process.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum HostPlatform {
    /// Microsoft Windows.
    Windows,
    /// Linux.
    Linux,
    /// Apple Darwin platforms.
    Darwin,
    /// Another Unix-family host.
    Unix,
    /// WebAssembly without native process mechanisms.
    WebAssembly,
    /// A host not yet classified by Bray.
    Other,
}

impl HostPlatform {
    /// Returns the stable host-family spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Linux => "linux",
            Self::Darwin => "darwin",
            Self::Unix => "unix",
            Self::WebAssembly => "webassembly",
            Self::Other => "other",
        }
    }
}

/// Independently testable native mechanism capability.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PlatformCapability {
    /// Native threads and thread-local runtime initialization.
    NativeThreads,
    /// Monotonic clocks and deadlines.
    MonotonicClock,
    /// Waitable and pollable host wake events.
    EventPolling,
    /// Anonymous virtual-memory mappings.
    AnonymousVirtualMemory,
    /// Child-process creation, signalling, and reaping.
    ChildProcesses,
    /// TCP and UDP sockets.
    Sockets,
}

/// Immutable capabilities of the native mechanism provider in this process.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PlatformContract {
    host: HostPlatform,
}

impl PlatformContract {
    /// Detects the mechanisms available to the current executable.
    pub const fn current() -> Self {
        Self {
            host: current_host(),
        }
    }

    /// Returns the current host family.
    pub const fn host(self) -> HostPlatform {
        self.host
    }

    /// Returns whether the current host provides a mechanism.
    pub const fn supports(self, capability: PlatformCapability) -> bool {
        match capability {
            PlatformCapability::MonotonicClock
            | PlatformCapability::EventPolling
            | PlatformCapability::AnonymousVirtualMemory => true,
            PlatformCapability::NativeThreads
            | PlatformCapability::ChildProcesses
            | PlatformCapability::Sockets => {
                !matches!(
                    self.host,
                    HostPlatform::WebAssembly | HostPlatform::Other
                )
            }
        }
    }
}

const fn current_host() -> HostPlatform {
    if cfg!(target_os = "windows") {
        HostPlatform::Windows
    } else if cfg!(target_os = "linux") {
        HostPlatform::Linux
    } else if cfg!(target_vendor = "apple") {
        HostPlatform::Darwin
    } else if cfg!(target_family = "unix") {
        HostPlatform::Unix
    } else if cfg!(target_family = "wasm") {
        HostPlatform::WebAssembly
    } else {
        HostPlatform::Other
    }
}

#[cfg(test)]
mod tests {
    use super::{PlatformCapability, PlatformContract};

    #[test]
    fn current_contract_exposes_required_portable_mechanisms() {
        let contract = PlatformContract::current();

        assert!(contract.supports(PlatformCapability::MonotonicClock));
        assert!(contract.supports(PlatformCapability::EventPolling));
        assert!(contract.supports(PlatformCapability::AnonymousVirtualMemory));
    }
}
