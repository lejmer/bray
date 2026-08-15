use std::sync::Arc;

use bray_base::shared_slice;
use bray_ir::{MirBlockId, MirCallableReference, MirHelperReference, MirOperationId};
use bray_symbols::ConstantValueId;

use crate::{CodegenInstanceKey, CodegenSymbolKey};

/// A compiler-defined call realized directly by a code generator.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum IntrinsicCall {
    /// A scalar unary operation.
    Unary(bray_ir::MirUnaryOperator),
    /// A scalar binary operation.
    Binary(bray_ir::MirBinaryOperator),
    /// An exact compiler-defined conversion plan.
    Conversion(bray_ir::SelectedConversion),
}

/// Maps one semantic callable reference in MIR to its concrete generated definition.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenCallableMapping {
    owner: CodegenInstanceKey,
    site: CodegenCallSite,
    reference: MirCallableReference,
    target: CodegenCallableTarget,
}

/// One direct-call occurrence within a concrete MIR instance.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CodegenCallSite {
    /// A call stored as one MIR operation.
    Operation(MirOperationId),
    /// An iterator call stored by one block terminator.
    Terminator(MirBlockId),
    /// A callable symbol operand retained by one assembly operation.
    InlineAssemblyOperation {
        /// Containing MIR operation.
        operation: MirOperationId,
        /// Symbol ordinal among the operation's assembly symbol operands.
        symbol: usize,
    },
    /// A callable symbol operand retained by one assembly terminator.
    InlineAssemblyTerminator {
        /// Containing MIR block.
        block: MirBlockId,
        /// Symbol ordinal among the terminator's assembly symbol operands.
        symbol: usize,
    },
}

/// The concrete realization selected for one direct MIR call.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CodegenCallableTarget {
    /// A generated or imported callable definition.
    Instance(CodegenInstanceKey),
    /// A compiler-defined operation emitted directly by the backend.
    Intrinsic(IntrinsicCall),
}

impl CodegenCallableMapping {
    /// Creates an exact callable-reference mapping.
    pub const fn new(
        owner: CodegenInstanceKey,
        site: CodegenCallSite,
        reference: MirCallableReference,
        instance: CodegenInstanceKey,
    ) -> Self {
        Self {
            owner,
            site,
            reference,
            target: CodegenCallableTarget::Instance(instance),
        }
    }

    /// Creates a mapping to a compiler-defined operation emitted directly by the backend.
    pub const fn intrinsic(
        owner: CodegenInstanceKey,
        site: CodegenCallSite,
        reference: MirCallableReference,
        intrinsic: IntrinsicCall,
    ) -> Self {
        Self {
            owner,
            site,
            reference,
            target: CodegenCallableTarget::Intrinsic(intrinsic),
        }
    }

    /// Returns the concrete definition containing the reference occurrence.
    pub const fn owner(&self) -> &CodegenInstanceKey {
        &self.owner
    }

    /// Returns the exact MIR occurrence containing the call.
    pub const fn site(&self) -> CodegenCallSite {
        self.site
    }

    /// Returns the callable reference retained by MIR.
    pub const fn reference(&self) -> MirCallableReference {
        self.reference
    }

    /// Returns the concrete generated definition selected for the reference.
    pub const fn target(&self) -> &CodegenCallableTarget {
        &self.target
    }

    /// Returns the generated callable definition when this call is not intrinsic.
    pub const fn instance(&self) -> Option<&CodegenInstanceKey> {
        match &self.target {
            CodegenCallableTarget::Instance(instance) => Some(instance),
            CodegenCallableTarget::Intrinsic(_) => None,
        }
    }

    /// Returns the compiler-defined operation emitted for an intrinsic call.
    pub const fn intrinsic_operation(&self) -> Option<&IntrinsicCall> {
        match &self.target {
            CodegenCallableTarget::Intrinsic(intrinsic) => Some(intrinsic),
            CodegenCallableTarget::Instance(_) => None,
        }
    }
}

/// Ordered generated helpers required to realize one MIR operation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenOperationMapping {
    owner: CodegenInstanceKey,
    operation: MirOperationId,
    helpers: Arc<[CodegenHelperMapping]>,
}

/// Extra realization inputs required by one MIR terminator.
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

    /// Returns the block whose terminator requires these requirements.
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
