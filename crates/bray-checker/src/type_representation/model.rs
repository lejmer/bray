use std::{num::NonZeroU16, sync::Arc};

use bray_base::{Cancellation, shared_slice};
use bray_compiler_known::{IntegerRepresentation, RepresentationRole};
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::DiagnosticResult;
use bray_source::SourceSpan;
use bray_symbols::{
    AvailableCompilerKnownSymbols, DeclarationExpressionTemplate, DirectiveSurface,
    IntegerConstant, NamedTypeSymbolId, SemanticValueStore, TypeExpressionTemplate, TypeId,
    UnionVariantSymbolId,
};

use crate::{CheckerFactResult, CheckerInfrastructureError, CheckerSource};

/// A resolved integer type and the representation used to validate tag values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RepresentationIntegerType {
    ty: TypeId,
    representation: IntegerRepresentation,
    target_width: NonZeroU16,
}

impl RepresentationIntegerType {
    /// Creates one integer type usable by a source-level tag contract.
    pub const fn new(
        ty: TypeId,
        representation: IntegerRepresentation,
        target_width: NonZeroU16,
    ) -> Self {
        Self {
            ty,
            representation,
            target_width,
        }
    }

    /// Returns the canonical semantic type.
    pub const fn ty(self) -> TypeId {
        self.ty
    }

    /// Returns whether this type can represent one integer tag.
    pub fn accepts(self, value: &IntegerConstant) -> bool {
        crate::constant::fits_integer_representation(value, self.representation, || {
            self.target_width
        })
    }
}

/// One represented field or union payload field.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclaredStorageMember {
    ty: TypeExpressionTemplate,
    span: SourceSpan,
    recovered: bool,
}

impl DeclaredStorageMember {
    /// Creates one source-backed represented member.
    pub const fn new(ty: TypeExpressionTemplate, span: SourceSpan, recovered: bool) -> Self {
        Self {
            ty,
            span,
            recovered,
        }
    }

    /// Returns the member's bound type template.
    pub const fn ty(&self) -> &TypeExpressionTemplate {
        &self.ty
    }

    /// Returns the member declaration span.
    pub const fn span(&self) -> SourceSpan {
        self.span
    }

    /// Returns whether syntax recovery affected the member.
    pub const fn is_recovered(&self) -> bool {
        self.recovered
    }
}

/// One union variant participating in representation checking.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclaredUnionVariant {
    id: UnionVariantSymbolId,
    span: SourceSpan,
    payload: Arc<[DeclaredStorageMember]>,
    directives: DirectiveSurface,
    recovered: bool,
}

impl DeclaredUnionVariant {
    /// Creates one source-backed union variant.
    pub fn new(
        id: UnionVariantSymbolId,
        span: SourceSpan,
        payload: impl IntoIterator<Item = DeclaredStorageMember>,
        directives: DirectiveSurface,
        recovered: bool,
    ) -> Self {
        Self {
            id,
            span,
            payload: shared_slice(payload),
            directives,
            recovered,
        }
    }

    /// Returns the exact variant identity.
    pub const fn id(&self) -> UnionVariantSymbolId {
        self.id
    }

    /// Returns the variant declaration span.
    pub const fn span(&self) -> SourceSpan {
        self.span
    }

    /// Returns represented payload fields in declaration order.
    pub fn payload(&self) -> &[DeclaredStorageMember] {
        &self.payload
    }

    /// Returns directives attached to this variant.
    pub const fn directives(&self) -> &DirectiveSurface {
        &self.directives
    }

    /// Returns whether syntax recovery affected the variant.
    pub const fn is_recovered(&self) -> bool {
        self.recovered
    }
}

/// The declaration data needed to derive one named type's representation contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclaredTypeDefinition {
    subject: NamedTypeSymbolId,
    span: SourceSpan,
    fields: Arc<[DeclaredStorageMember]>,
    variants: Arc<[DeclaredUnionVariant]>,
    directives: DirectiveSurface,
    has_lifecycle: bool,
    is_generic: bool,
    recovered: bool,
}

impl DeclaredTypeDefinition {
    /// Creates one product-type definition.
    pub fn structure(
        subject: NamedTypeSymbolId,
        span: SourceSpan,
        fields: impl IntoIterator<Item = DeclaredStorageMember>,
        directives: DirectiveSurface,
        has_lifecycle: bool,
        is_generic: bool,
        recovered: bool,
    ) -> Self {
        Self {
            subject,
            span,
            fields: shared_slice(fields),
            variants: Arc::from([]),
            directives,
            has_lifecycle,
            is_generic,
            recovered,
        }
    }

    /// Creates one union-type definition.
    pub fn union(
        subject: NamedTypeSymbolId,
        span: SourceSpan,
        variants: impl IntoIterator<Item = DeclaredUnionVariant>,
        directives: DirectiveSurface,
        has_lifecycle: bool,
        is_generic: bool,
        recovered: bool,
    ) -> Self {
        Self {
            subject,
            span,
            fields: Arc::from([]),
            variants: shared_slice(variants),
            directives,
            has_lifecycle,
            is_generic,
            recovered,
        }
    }

    /// Returns the named type described by this definition.
    pub const fn subject(&self) -> NamedTypeSymbolId {
        self.subject
    }

    /// Returns the declaration span.
    pub const fn span(&self) -> SourceSpan {
        self.span
    }

    /// Returns product fields in declaration order.
    pub fn fields(&self) -> &[DeclaredStorageMember] {
        &self.fields
    }

    /// Returns union variants in declaration order.
    pub fn variants(&self) -> &[DeclaredUnionVariant] {
        &self.variants
    }

    /// Returns directives attached to the type declaration.
    pub const fn directives(&self) -> &DirectiveSurface {
        &self.directives
    }

    /// Returns whether any lifecycle declaration is associated with the type.
    pub const fn has_lifecycle(&self) -> bool {
        self.has_lifecycle
    }

    /// Returns whether the declaration has generic parameters.
    pub const fn is_generic(&self) -> bool {
        self.is_generic
    }

    /// Returns whether syntax recovery affected the declaration.
    pub const fn is_recovered(&self) -> bool {
        self.recovered
    }
}

/// Dependencies used while checking declared type representation contracts.
pub trait TypeRepresentationContext: Sync {
    /// Returns the canonical semantic value store.
    fn semantic_values(&self) -> &SemanticValueStore;

    /// Returns compiler-known identities available for the selected target.
    fn available_compiler_known_symbols(&self) -> &AvailableCompilerKnownSymbols;

    /// Returns one named type definition and its dependency diagnostics.
    fn type_definition(
        &self,
        subject: NamedTypeSymbolId,
    ) -> CheckerFactResult<DiagnosticResult<DeclaredTypeDefinition>>;

    /// Resolves one directive expression to its exact source text.
    fn source(&self, syntax: SyntaxAnchor)
    -> Result<CheckerSource<'_>, CheckerInfrastructureError>;

    /// Evaluates one layout-size expression as a nonnegative integer.
    fn unsigned_integer(
        &self,
        expression: DeclarationExpressionTemplate,
    ) -> CheckerFactResult<DiagnosticResult<Option<u64>>>;

    /// Evaluates one union tag expression using its selected integer type.
    fn integer_constant(
        &self,
        expression: DeclarationExpressionTemplate,
        expected: Option<RepresentationIntegerType>,
    ) -> CheckerFactResult<DiagnosticResult<Option<IntegerConstant>>>;

    /// Resolves one directive expression as a built-in integer type.
    fn integer_type(
        &self,
        expression: DeclarationExpressionTemplate,
    ) -> CheckerFactResult<Option<RepresentationIntegerType>>;

    /// Resolves one fixed-width integer representation role.
    fn integer_type_for_role(
        &self,
        role: RepresentationRole,
    ) -> CheckerFactResult<RepresentationIntegerType>;

    /// Returns the cancellation source for this request.
    fn cancellation(&self) -> &dyn Cancellation;
}
