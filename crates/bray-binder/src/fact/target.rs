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

#[cfg(test)]
mod tests {
    use bray_symbols::{ConstantSymbolId, SymbolId};

    use super::TargetFactProvider;
    use crate::fact::test_support::TestFixture;
    use crate::{BinderFactContext, BinderFactError};

    #[test]
    fn target_facts_return_canonical_values_and_reject_unknown_symbols() {
        let fixture = TestFixture::new();
        let context = fixture.context();

        let target = match context.target_facts().target_fact(fixture.constant) {
            Ok(target) => target,
            Err(error) => panic!("test target fact should exist: {error:?}"),
        };

        assert_eq!(target.value(), &fixture.target_value);

        let unknown = ConstantSymbolId::from_symbol_id(SymbolId::new(99));

        assert_eq!(
            context.target_facts().target_fact(unknown),
            Err(BinderFactError::DependencyUnavailable)
        );
    }
}
