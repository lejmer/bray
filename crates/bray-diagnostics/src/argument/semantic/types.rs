use std::sync::Arc;

use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates an expected semantic-type argument.
    pub const fn expected_type(ty: DiagnosticType) -> Self {
        Self::new(
            DiagnosticArgName::ExpectedType,
            DiagnosticArgValue::Type(ty),
        )
    }

    /// Creates an actual semantic-type argument.
    pub const fn actual_type(ty: DiagnosticType) -> Self {
        Self::new(DiagnosticArgName::ActualType, DiagnosticArgValue::Type(ty))
    }
}

/// One source-visible named type retained by structured diagnostics.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticNamedType {
    path: Arc<[String]>,
    arguments: Arc<[DiagnosticTypeArgument]>,
}

impl DiagnosticNamedType {
    /// Creates a named type from its qualified path and ordered generic arguments.
    pub fn new(
        path: impl IntoIterator<Item = String>,
        arguments: impl IntoIterator<Item = DiagnosticTypeArgument>,
    ) -> Self {
        Self {
            path: path.into_iter().collect(),
            arguments: arguments.into_iter().collect(),
        }
    }

    /// Returns the qualified source path components.
    pub fn path(&self) -> &[String] {
        &self.path
    }

    /// Returns the ordered generic arguments.
    pub fn arguments(&self) -> &[DiagnosticTypeArgument] {
        &self.arguments
    }
}

/// One generic argument retained by a diagnostic type.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticTypeArgument {
    /// A semantic type argument.
    Type(DiagnosticType),
    /// A constant argument whose exact value is not available to diagnostics.
    Constant,
}

/// Locale-neutral semantic type categories used by structured diagnostics.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticType {
    /// The canonical recovery type.
    Error,
    /// The Boolean scalar type.
    Boolean,
    /// The character scalar type.
    Character,
    /// The 8-bit signed integer type.
    I8,
    /// The 16-bit signed integer type.
    I16,
    /// The 32-bit signed integer type.
    I32,
    /// The 64-bit signed integer type.
    I64,
    /// The 128-bit signed integer type.
    I128,
    /// The 8-bit unsigned integer type.
    U8,
    /// The 16-bit unsigned integer type.
    U16,
    /// The 32-bit unsigned integer type.
    U32,
    /// The 64-bit unsigned integer type.
    U64,
    /// The 128-bit unsigned integer type.
    U128,
    /// The machine-sized signed integer type.
    Isize,
    /// The machine-sized unsigned integer type.
    Usize,
    /// The unit type.
    Unit,
    /// The uninhabited type.
    Never,
    /// The string type.
    String,
    /// The 16-bit real type.
    R16,
    /// The 32-bit real type.
    R32,
    /// The 64-bit real type.
    R64,
    /// The 128-bit real type.
    R128,
    /// The 32-bit complex type.
    C32,
    /// The 64-bit complex type.
    C64,
    /// The 128-bit complex type.
    C128,
    /// The 256-bit complex type.
    C256,
    /// A named type with its qualified identity and generic arguments.
    Named(DiagnosticNamedType),
    /// A type whose source-visible identity is unavailable.
    Unknown,
    /// A generic type parameter.
    TypeParameter,
    /// The contextual `Self` type.
    ContextualSelf,
    /// A type-valued member projection.
    TypeValuedMember,
    /// A tuple with the supplied element count.
    Tuple(u64),
    /// A fixed-size array.
    Array,
    /// A dynamically sized slice.
    Slice,
    /// A lazy homogeneous generator.
    Generator,
    /// A nullable type.
    Nullable,
    /// A borrowed type.
    Borrow,
    /// A dynamically dispatched trait view.
    TraitView,
    /// An owned indirection type.
    OwnedIndirection,
    /// A callable type.
    Callable,
}
