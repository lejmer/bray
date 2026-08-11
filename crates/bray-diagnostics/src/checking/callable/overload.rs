use crate::{DiagnosticInterfaceSymbolIdentity, DiagnosticType};

/// Borrow layer used by an implementation overload family subject.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticImplementationBorrowKind {
    /// Shared observation through the subject borrow.
    Shared,
    /// Mutable access through the subject borrow.
    Mutable,
}

/// Exact named subject selected by an implementation overload family.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticImplementationFamilySubject {
    /// A directly named structural type.
    Named(DiagnosticInterfaceSymbolIdentity),
    /// A borrowed named structural type.
    Borrowed {
        /// Borrow capability applied to the subject.
        kind: DiagnosticImplementationBorrowKind,
        /// Named structural type beneath the borrow.
        subject: DiagnosticInterfaceSymbolIdentity,
    },
}

/// Complete subject and trait identity of one implementation overload family.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticImplementationFamily {
    subject: DiagnosticImplementationFamilySubject,
    trait_definition: DiagnosticInterfaceSymbolIdentity,
}

impl DiagnosticImplementationFamily {
    /// Creates an overload-family identity from its subject and trait declaration.
    pub const fn new(
        subject: DiagnosticImplementationFamilySubject,
        trait_definition: DiagnosticInterfaceSymbolIdentity,
    ) -> Self {
        Self {
            subject,
            trait_definition,
        }
    }

    /// Returns the family subject.
    pub const fn subject(&self) -> &DiagnosticImplementationFamilySubject {
        &self.subject
    }

    /// Returns the required trait declaration.
    pub const fn trait_definition(&self) -> &DiagnosticInterfaceSymbolIdentity {
        &self.trait_definition
    }
}

/// Exact reason an implementation overload header or arm is invalid.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticImplementationOverloadProblem {
    /// Header subject resolves to a declaration category that cannot define a family subject.
    HeaderSubjectKind {
        /// Exact resolved declaration identity.
        subject: DiagnosticInterfaceSymbolIdentity,
        /// Resolved declaration category.
        actual: crate::DiagnosticInterfaceSymbolKind,
    },
    /// Header trait path resolves to a declaration that is not a trait.
    HeaderTraitKind {
        /// Exact resolved declaration identity.
        trait_definition: DiagnosticInterfaceSymbolIdentity,
        /// Resolved declaration category.
        actual: crate::DiagnosticInterfaceSymbolKind,
    },
    /// Arm path resolves to a declaration that is not a named trait implementation.
    ArmSymbolKind {
        /// Exact resolved declaration identity.
        implementation: DiagnosticInterfaceSymbolIdentity,
        /// Resolved declaration category.
        actual: crate::DiagnosticInterfaceSymbolKind,
    },
    /// The same named implementation appears more than once in one family.
    DuplicateArm {
        /// Exact repeated implementation identity.
        implementation: DiagnosticInterfaceSymbolIdentity,
    },
    /// Arm implementation has a subject that cannot belong to a named overload family.
    ArmSubjectNotFamilyCompatible {
        /// Exact arm implementation identity.
        implementation: DiagnosticInterfaceSymbolIdentity,
    },
    /// Arm implementation belongs to a different subject/trait family.
    FamilyMismatch {
        /// Exact arm implementation identity.
        implementation: DiagnosticInterfaceSymbolIdentity,
        /// Family declared by the overload header.
        required: DiagnosticImplementationFamily,
        /// Family derived from the implementation header.
        provided: DiagnosticImplementationFamily,
    },
}

/// User-facing identity and call surface of one callable overload arm.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticCallableOverloadArm {
    identity: DiagnosticInterfaceSymbolIdentity,
    has_receiver: bool,
    parameter_types: Box<[DiagnosticType]>,
    result_type: DiagnosticType,
}

impl DiagnosticCallableOverloadArm {
    /// Creates an overload-arm description from its declaration and checked call surface.
    pub fn new(
        identity: DiagnosticInterfaceSymbolIdentity,
        has_receiver: bool,
        parameter_types: impl Into<Box<[DiagnosticType]>>,
        result_type: DiagnosticType,
    ) -> Self {
        Self {
            identity,
            has_receiver,
            parameter_types: parameter_types.into(),
            result_type,
        }
    }

    /// Returns the callable declaration identity.
    pub const fn identity(&self) -> &DiagnosticInterfaceSymbolIdentity {
        &self.identity
    }

    /// Returns whether calls through this arm require an implicit receiver.
    pub const fn has_receiver(&self) -> bool {
        self.has_receiver
    }

    /// Returns ordinary parameter types in declaration order.
    pub fn parameter_types(&self) -> &[DiagnosticType] {
        &self.parameter_types
    }

    /// Returns the declared result type.
    pub const fn result_type(&self) -> &DiagnosticType {
        &self.result_type
    }
}

/// Declaration context that owns one callable overload family or arm.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticCallableOverloadContext {
    /// A module owns the callable.
    Module(DiagnosticInterfaceSymbolIdentity),
    /// A named structural type owns the callable.
    NamedType(DiagnosticInterfaceSymbolIdentity),
    /// A trait owns the callable.
    Trait(DiagnosticInterfaceSymbolIdentity),
    /// An implementation owns the callable.
    Implementation(DiagnosticInterfaceSymbolIdentity),
}

/// Exact reason a callable overload arm or family is invalid.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticCallableOverloadProblem {
    /// An arm path resolves to a declaration that is not callable.
    ArmSymbolKind {
        /// Exact resolved declaration identity.
        symbol: DiagnosticInterfaceSymbolIdentity,
        /// Resolved declaration category.
        actual: crate::DiagnosticInterfaceSymbolKind,
    },
    /// The arm callable belongs to a different declaration context than the family.
    ContextMismatch {
        /// Exact arm callable and call surface.
        arm: DiagnosticCallableOverloadArm,
        /// Context that owns the overload family.
        required: DiagnosticCallableOverloadContext,
        /// Context that owns the callable arm.
        provided: DiagnosticCallableOverloadContext,
    },
    /// The same callable occurs more than once in one family.
    DuplicateArm {
        /// Exact repeated callable and call surface.
        arm: DiagnosticCallableOverloadArm,
    },
    /// One callable belongs to more than one overload family.
    ConflictingFamilies {
        /// Exact repeated callable and call surface.
        arm: DiagnosticCallableOverloadArm,
        /// First overload family declaration.
        first: DiagnosticInterfaceSymbolIdentity,
        /// Conflicting overload family declaration.
        second: DiagnosticInterfaceSymbolIdentity,
    },
    /// Two arms expose indistinguishable call surfaces.
    ConflictingSignatures {
        /// Arm at the primary diagnostic location.
        arm: DiagnosticCallableOverloadArm,
        /// Arm with the conflicting call surface.
        conflicting: DiagnosticCallableOverloadArm,
    },
}
