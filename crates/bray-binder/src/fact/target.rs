use std::sync::Arc;

use bray_diagnostics::DiagnosticResult;
use bray_symbols::{ConstantSymbolId, ConstantValueId};

use crate::BinderFactResult;

/// The diagnostic-bearing value of one compiler-known target constant.
pub type TargetFactResult = DiagnosticResult<ConstantValueId>;

/// Provides read-only target-profile facts as canonical compiler-known constant values.
///
/// The selected profile, target validation, cache keys, and invalidation policy remain owned by
/// compilation. Unknown or malformed source-level target uses are represented by ordinary
/// semantic recovery and diagnostics; provider failure is reserved for an unavailable dependency
/// or cancellation.
pub trait TargetFactProvider: Send + Sync {
    /// Returns the selected target value for an exact compiler-known constant symbol.
    fn target_fact(&self, fact: ConstantSymbolId) -> BinderFactResult<Arc<TargetFactResult>>;
}
