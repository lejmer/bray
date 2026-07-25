use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

/// Scheduling priority for one compiler fact request.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum QueryPriority {
    /// Latency-sensitive work requested by interactive language tooling.
    Interactive,
    /// Ordinary compiler work.
    #[default]
    Normal,
    /// Speculative or broad work that may yield to interactive requests.
    Background,
}

#[derive(Clone, Debug)]
pub(crate) struct QueryPriorityDemand {
    value: Arc<AtomicU8>,
}

impl QueryPriority {
    const fn rank(self) -> u8 {
        match self {
            Self::Interactive => 0,
            Self::Normal => 1,
            Self::Background => 2,
        }
    }

    const fn from_rank(rank: u8) -> Self {
        match rank {
            0 => Self::Interactive,
            1 => Self::Normal,
            _ => Self::Background,
        }
    }
}

impl QueryPriorityDemand {
    pub(crate) fn new(priority: QueryPriority) -> Self {
        Self {
            value: Arc::new(AtomicU8::new(priority.rank())),
        }
    }

    pub(crate) fn promote(&self, priority: QueryPriority) {
        self.value.fetch_min(priority.rank(), Ordering::AcqRel);
    }

    pub(crate) fn current(&self) -> QueryPriority {
        QueryPriority::from_rank(self.value.load(Ordering::Acquire))
    }
}
