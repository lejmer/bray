use std::sync::Arc;

use bray_base::shared_slice;
use bray_ir::{MirBlockId, MirCallableReference, MirOperationId};
use bray_symbols::ConstantValueId;

use crate::{CodegenInstanceKey, CodegenSymbolKey};

/// Maps one semantic callable reference in MIR to its concrete generated definition.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenCallableMapping {
    reference: MirCallableReference,
    instance: CodegenInstanceKey,
}

impl CodegenCallableMapping {
    /// Creates an exact callable-reference mapping.
    pub const fn new(reference: MirCallableReference, instance: CodegenInstanceKey) -> Self {
        Self {
            reference,
            instance,
        }
    }

    /// Returns the callable reference retained by MIR.
    pub const fn reference(&self) -> MirCallableReference {
        self.reference
    }

    /// Returns the concrete generated definition selected for the reference.
    pub const fn instance(&self) -> &CodegenInstanceKey {
        &self.instance
    }
}

/// Ordered generated helpers required to realize one MIR operation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenOperationMapping {
    operation: MirOperationId,
    helpers: Arc<[CodegenSymbolKey]>,
}

/// Extra realization facts required by one MIR terminator.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenTerminatorMapping {
    block: MirBlockId,
    helpers: Arc<[CodegenSymbolKey]>,
    constants: Arc<[ConstantValueId]>,
}

impl CodegenTerminatorMapping {
    /// Creates one terminator mapping from ordered helper and constant references.
    pub fn new(
        block: MirBlockId,
        helpers: impl IntoIterator<Item = CodegenSymbolKey>,
        constants: impl IntoIterator<Item = ConstantValueId>,
    ) -> Self {
        Self {
            block,
            helpers: shared_slice(helpers),
            constants: shared_slice(constants),
        }
    }

    /// Returns the block whose terminator requires these facts.
    pub const fn block(&self) -> MirBlockId {
        self.block
    }

    /// Returns helper symbols in semantic execution order.
    pub fn helpers(&self) -> &[CodegenSymbolKey] {
        &self.helpers
    }

    /// Returns materialized constants in semantic evaluation order.
    pub fn constants(&self) -> &[ConstantValueId] {
        &self.constants
    }
}

impl CodegenOperationMapping {
    /// Creates one operation mapping with helper symbols in semantic execution order.
    pub fn new(
        operation: MirOperationId,
        helpers: impl IntoIterator<Item = CodegenSymbolKey>,
    ) -> Self {
        Self {
            operation,
            helpers: shared_slice(helpers),
        }
    }

    /// Returns the exact MIR operation.
    pub const fn operation(&self) -> MirOperationId {
        self.operation
    }

    /// Returns helper symbols in semantic execution order.
    pub fn helpers(&self) -> &[CodegenSymbolKey] {
        &self.helpers
    }
}
