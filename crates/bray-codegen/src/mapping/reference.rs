use std::sync::Arc;

use bray_base::shared_slice;
use bray_ir::{MirBlockId, MirCallableReference, MirHelperReference, MirOperationId};
use bray_symbols::ConstantValueId;

use crate::{CodegenInstanceKey, CodegenSymbolKey};

/// Maps one semantic callable reference in MIR to its concrete generated definition.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenCallableMapping {
    owner: CodegenInstanceKey,
    reference: MirCallableReference,
    instance: CodegenInstanceKey,
}

impl CodegenCallableMapping {
    /// Creates an exact callable-reference mapping.
    pub const fn new(
        owner: CodegenInstanceKey,
        reference: MirCallableReference,
        instance: CodegenInstanceKey,
    ) -> Self {
        Self {
            owner,
            reference,
            instance,
        }
    }

    /// Returns the concrete definition containing the reference occurrence.
    pub const fn owner(&self) -> &CodegenInstanceKey {
        &self.owner
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
    owner: CodegenInstanceKey,
    operation: MirOperationId,
    helpers: Arc<[CodegenHelperMapping]>,
}

/// Extra realization facts required by one MIR terminator.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenTerminatorMapping {
    owner: CodegenInstanceKey,
    block: MirBlockId,
    constants: Arc<[ConstantValueId]>,
}

impl CodegenTerminatorMapping {
    /// Creates one terminator mapping from ordered constant references.
    pub fn new(
        owner: CodegenInstanceKey,
        block: MirBlockId,
        constants: impl IntoIterator<Item = ConstantValueId>,
    ) -> Self {
        Self {
            owner,
            block,
            constants: shared_slice(constants),
        }
    }

    /// Returns the concrete definition containing the terminator.
    pub const fn owner(&self) -> &CodegenInstanceKey {
        &self.owner
    }

    /// Returns the block whose terminator requires these facts.
    pub const fn block(&self) -> MirBlockId {
        self.block
    }

    /// Returns materialized constants in semantic evaluation order.
    pub fn constants(&self) -> &[ConstantValueId] {
        &self.constants
    }
}

impl CodegenOperationMapping {
    /// Creates one operation mapping with helper symbols in semantic execution order.
    pub fn new(
        owner: CodegenInstanceKey,
        operation: MirOperationId,
        helpers: impl IntoIterator<Item = CodegenHelperMapping>,
    ) -> Self {
        Self {
            owner,
            operation,
            helpers: shared_slice(helpers),
        }
    }

    /// Returns the concrete definition containing the operation.
    pub const fn owner(&self) -> &CodegenInstanceKey {
        &self.owner
    }

    /// Returns the exact MIR operation.
    pub const fn operation(&self) -> MirOperationId {
        self.operation
    }

    /// Returns helper symbols in semantic execution order.
    pub fn helpers(&self) -> &[CodegenHelperMapping] {
        &self.helpers
    }
}

/// One helper symbol selected for an exact semantic MIR helper role.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenHelperMapping {
    reference: MirHelperReference,
    symbol: Option<CodegenSymbolKey>,
}

impl CodegenHelperMapping {
    /// Creates one symbol-backed helper realization.
    pub const fn new(reference: MirHelperReference, symbol: CodegenSymbolKey) -> Self {
        Self {
            reference,
            symbol: Some(symbol),
        }
    }

    /// Creates one helper realization performed directly by the selected backend.
    pub const fn lowered(reference: MirHelperReference) -> Self {
        Self {
            reference,
            symbol: None,
        }
    }

    /// Returns the semantic helper role retained by MIR.
    pub const fn reference(&self) -> &MirHelperReference {
        &self.reference
    }

    /// Returns the selected binary symbol when the helper requires one.
    pub const fn symbol(&self) -> Option<&CodegenSymbolKey> {
        self.symbol.as_ref()
    }
}
