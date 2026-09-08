use std::sync::Arc;

use bray_symbols::{CallableTypeTemplate, LocalBindingSymbolId};

/// The signature and local parameter identities of a callable type's contract unit.
///
/// Parameters are ordered exactly as in the signature. Each identity belongs to the owning
/// bound unit and names the corresponding formal, independently of an executable body.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BoundContractInputs {
    signature: CallableTypeTemplate,
    parameters: Arc<[LocalBindingSymbolId]>,
}

impl BoundContractInputs {
    pub(crate) fn new(
        signature: CallableTypeTemplate,
        parameters: Arc<[LocalBindingSymbolId]>,
    ) -> Self {
        Self {
            signature,
            parameters,
        }
    }

    /// Returns the callable's declared parameter and completion types.
    pub const fn signature(&self) -> &CallableTypeTemplate {
        &self.signature
    }

    /// Returns the parameter identities in signature order.
    pub fn parameters(&self) -> &[LocalBindingSymbolId] {
        &self.parameters
    }
}
