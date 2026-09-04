use crate::{
    BorrowCapabilityId, BoundExpressionId, BoundPatternId, BoundSourceAnchor, BoundUnitId,
    StorageAccessId, StorageIdentityId,
};
use bray_symbols::{
    AnonymousCallableParameterSymbolId, BorrowKind, CallableParameterSymbolId,
    LocalBindingSymbolId, PostconditionResultSymbolId, PredicateParameterSymbolId,
    ReceiverParameterSymbolId,
};
use std::sync::Arc;

/// The semantic construct that establishes a borrow capability.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BorrowCapabilityOrigin {
    /// A borrow or reborrow expression evaluated inside the unit.
    Expression(BoundExpressionId),
    /// A borrow supplied by the caller when the unit begins.
    Entry(StorageBindingTarget),
}

/// One borrow capability established by an evaluated storage access.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PlannedBorrowCapability {
    origin: BorrowCapabilityOrigin,
    kind: BorrowKind,
    access: StorageAccessId,
    parent: Option<BorrowCapabilityId>,
    source: BoundSourceAnchor,
    is_recovered: bool,
}

impl PlannedBorrowCapability {
    /// Creates one planned borrow or reborrow capability.
    pub const fn new(
        origin: BorrowCapabilityOrigin,
        kind: BorrowKind,
        access: StorageAccessId,
        parent: Option<BorrowCapabilityId>,
        source: BoundSourceAnchor,
        is_recovered: bool,
    ) -> Self {
        Self {
            origin,
            kind,
            access,
            parent,
            source,
            is_recovered,
        }
    }

    /// Returns the semantic construct that establishes the capability.
    pub const fn origin(self) -> BorrowCapabilityOrigin {
        self.origin
    }

    /// Returns the expression that establishes the capability, when evaluated in the unit.
    pub const fn expression(self) -> Option<BoundExpressionId> {
        match self.origin {
            BorrowCapabilityOrigin::Expression(expression) => Some(expression),
            BorrowCapabilityOrigin::Entry(_) => None,
        }
    }

    /// Returns the entry binding that supplies the capability, when any.
    pub const fn entry_binding(self) -> Option<StorageBindingTarget> {
        match self.origin {
            BorrowCapabilityOrigin::Expression(_) => None,
            BorrowCapabilityOrigin::Entry(target) => Some(target),
        }
    }

    /// Returns the shared or mutable borrow category.
    pub const fn kind(self) -> BorrowKind {
        self.kind
    }

    /// Returns the storage access from which the capability is derived.
    pub const fn access(self) -> StorageAccessId {
        self.access
    }

    /// Returns the parent capability when this is a reborrow.
    pub const fn parent(self) -> Option<BorrowCapabilityId> {
        self.parent
    }

    /// Returns the construct that establishes the capability.
    pub const fn source(self) -> BoundSourceAnchor {
        self.source
    }

    /// Returns whether recovery affected this capability.
    pub const fn is_recovered(self) -> bool {
        self.is_recovered
    }
}

/// A semantic identity that names storage within one bound unit.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageBindingTarget {
    /// An ordinary callable parameter.
    Parameter(CallableParameterSymbolId),
    /// A callable receiver.
    Receiver(ReceiverParameterSymbolId),
    /// One static declaration instance.
    Static(bray_symbols::StaticSymbolId),
    /// An anonymous callable parameter.
    AnonymousParameter(AnonymousCallableParameterSymbolId),
    /// A predicate parameter.
    PredicateParameter(PredicateParameterSymbolId),
    /// A pattern-introduced local binding.
    Local(LocalBindingSymbolId),
    /// An owned value consumed by a discard pattern.
    PatternDiscard(BoundPatternId),
    /// The exact input access observed by a structural pattern occurrence.
    PatternSubject(BoundPatternId),
    /// A callable result visible to a postcondition.
    PostconditionResult(PostconditionResultSymbolId),
    /// The value produced by the bound unit.
    Result,
}

/// The persistent storage or evaluated access named by a semantic identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageBinding {
    /// A persistent storage origin.
    Identity(StorageIdentityId),
    /// An evaluated access projected from existing storage.
    Access(StorageAccessId),
}

impl StorageBinding {
    pub(in crate::storage) const fn unit(self) -> BoundUnitId {
        match self {
            Self::Identity(storage) => storage.unit(),
            Self::Access(access) => access.unit(),
        }
    }
}

/// Exact source accesses represented by one branch-dependent logical binding.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct StorageAlternative {
    pattern: BoundPatternId,
    accesses: Arc<[StorageAccessId]>,
}

impl StorageAlternative {
    pub(in crate::storage) fn new(pattern: BoundPatternId, accesses: Vec<StorageAccessId>) -> Self {
        Self {
            pattern,
            accesses: accesses.into(),
        }
    }

    /// Returns the alternative pattern that establishes the logical binding.
    pub const fn pattern(&self) -> BoundPatternId {
        self.pattern
    }

    /// Returns one exact source access for each branch that establishes the binding.
    pub fn accesses(&self) -> &[StorageAccessId] {
        &self.accesses
    }
}

/// The source-semantic purpose of one evaluated storage access.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageAccessPurpose {
    /// Observe the reached value.
    Read,
    /// Establish an initialized value in newly produced storage.
    Initialize,
    /// Evaluate a destination that requires mutation authority.
    Write,
    /// Transfer ownership out of the reached storage.
    Move,
    /// Copy the reached value without transferring ownership.
    Copy,
    /// Transfer a value before its copy-or-move behavior has been resolved.
    ValueTransfer,
    /// Establish a borrow capability over the reached storage.
    Borrow(BorrowKind),
    /// Use the reached storage as an assignment destination.
    Assignment,
    /// Select a member substorage.
    Member,
    /// Select one indexed element.
    Index,
    /// Select a slice range.
    Slice,
    /// Apply another typed storage projection.
    Projection,
}

impl StorageAccessPurpose {
    /// Returns this access purpose's stable machine-readable name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Initialize => "initialize",
            Self::Write => "write",
            Self::Move => "move",
            Self::Copy => "copy",
            Self::ValueTransfer => "value_transfer",
            Self::Borrow(_) => "borrow",
            Self::Assignment => "assignment",
            Self::Member => "member",
            Self::Index => "index",
            Self::Slice => "slice",
            Self::Projection => "projection",
        }
    }

    /// Returns whether a checked purpose is a valid resolution of this planned purpose.
    pub fn matches_checked(self, checked: Self) -> bool {
        self == checked
            || matches!(self, Self::ValueTransfer)
                && matches!(checked, Self::Copy | Self::Move | Self::ValueTransfer)
    }
}

/// One expression occurrence and the exact storage access it evaluates.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageAccessPlan {
    expression: BoundExpressionId,
    purpose: StorageAccessPurpose,
    access: StorageAccessId,
}

impl StorageAccessPlan {
    pub(in crate::storage) const fn new(
        expression: BoundExpressionId,
        purpose: StorageAccessPurpose,
        access: StorageAccessId,
    ) -> Self {
        Self {
            expression,
            purpose,
            access,
        }
    }

    /// Returns the expression occurrence that evaluates this access.
    pub const fn expression(self) -> BoundExpressionId {
        self.expression
    }

    /// Returns how the expression uses the reached storage.
    pub const fn purpose(self) -> StorageAccessPurpose {
        self.purpose
    }

    /// Returns the evaluated storage-access identity.
    pub const fn access(self) -> StorageAccessId {
        self.access
    }
}
