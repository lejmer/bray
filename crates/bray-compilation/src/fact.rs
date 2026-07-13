mod cache;
mod cancellation;
mod completion;
mod control_flow;
mod error;
mod key;
mod runtime;
mod unit_identity;

pub use cancellation::CancellationToken;
pub use completion::{SymbolCompletionError, force_complete_symbol};
pub use error::{FactCycle, FactQueryError};
pub use key::{CompilationFactKey, SymbolFactKey};

pub(crate) use cache::FactCell;
pub(crate) use control_flow::{ControlFlowUnitFact, ControlFlowUnitFactCaches, PublishedUnit};
pub(crate) use runtime::FactRuntime;
pub(crate) use unit_identity::BoundUnitIdentityMap;
