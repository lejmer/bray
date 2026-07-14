use std::path::PathBuf;
use std::sync::Arc;

use bray_base::shared_str;

/// Host-supplied identity resolved to an in-memory collector or writable stream at publication.
///
/// The identity deliberately carries no open handle or mutable collector state, which keeps
/// requests and plans immutable and safe to share between workers.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OutputSinkId(Arc<str>);

impl OutputSinkId {
    /// Creates a sink identity unless its canonical representation is empty.
    pub fn try_new(value: impl Into<Arc<str>>) -> Option<Self> {
        let value = shared_str(value);

        if value.is_empty() {
            return None;
        }

        Some(Self(value))
    }

    /// Returns the canonical host-supplied sink identity.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Exact immutable publication destination of one planned external artifact.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum OutputSink {
    /// Final filesystem artifact path.
    Filesystem(PathBuf),
    /// Host-owned in-memory collector resolved by identity during publication.
    Memory(OutputSinkId),
    /// Host-owned writable stream resolved by identity during publication.
    Stream(OutputSinkId),
}

/// Policy for a planned destination that already contains an artifact.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ReplacementPolicy {
    /// Publication fails when the destination already exists or contains an artifact.
    RequireAbsent,
    /// Publication may atomically replace an existing artifact.
    ReplaceExisting,
}

#[cfg(test)]
mod tests {
    use super::{OutputSink, OutputSinkId};

    #[test]
    fn indirect_sinks_are_immutable_identities_without_open_handles() {
        assert_eq!(OutputSinkId::try_new(""), None);

        let Some(id) = OutputSinkId::try_new("host.output") else {
            panic!("test sink identity must be valid");
        };

        assert_eq!(id.as_str(), "host.output");
        assert_send_sync::<OutputSink>();
    }

    fn assert_send_sync<T: Send + Sync>() {}
}
