mod cache;
mod cancellation;
mod checked_unit;
mod completion;
mod error;
mod key;
mod runtime;

pub use cancellation::CancellationToken;
pub use completion::{SymbolCompletionError, force_complete_symbol};
pub use error::{FactCycle, FactQueryError};
pub use key::{CompilationFactKey, SymbolFactKey};

pub(crate) use cache::FactCell;
pub(crate) use checked_unit::{CheckedUnitFact, CheckedUnitFactCaches, PublishedCheckedUnit};
pub(crate) use runtime::FactRuntime;
