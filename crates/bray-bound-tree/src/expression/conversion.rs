use std::sync::Arc;

use bray_base::shared_slice;
use bray_symbols::{ImplementationInstanceId, TypeId};

use super::CheckedCallableReference;

/// A selected built-in scalar conversion operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CheckedScalarConversion {
    /// A value-preserving widening within the signed integer domain.
    SignedIntegerWidening,
    /// A value-preserving widening within the unsigned integer domain.
    UnsignedIntegerWidening,
    /// A value-preserving signed-to-unsigned integer conversion.
    SignedToUnsigned,
    /// A value-preserving unsigned-to-signed integer conversion.
    UnsignedToSigned,
    /// An exact integer-to-real conversion.
    IntegerToReal,
    /// A value-preserving real-format widening.
    RealWidening,
    /// A value-preserving complex-format widening.
    ComplexWidening,
}

/// The execution plan selected for one explicit conversion.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum CheckedConversionKind {
    /// Source and target have identical semantic type and representation.
    Identity,
    /// One selected built-in scalar operation.
    Scalar(CheckedScalarConversion),
    /// Element-wise tuple conversion with unchanged arity.
    Tuple(Arc<[CheckedConversion]>),
    /// Element-wise fixed-array conversion with unchanged length.
    Array(Box<CheckedConversion>),
    /// Present-value conversion preserving nullable absence.
    Nullable(Box<CheckedConversion>),
    /// Two-element tuple conversion into a built-in complex value.
    TupleToComplex {
        /// Conversion of the first tuple element into the real component.
        real: Box<CheckedConversion>,
        /// Conversion of the second tuple element into the imaginary component.
        imaginary: Box<CheckedConversion>,
    },
    /// A selected `ConvertTo<T>` implementation operation.
    UserDefined {
        /// Exact conversion callable and ABI.
        callable: CheckedCallableReference,
        /// Implementation witness selected for the source and target types.
        implementation: ImplementationInstanceId,
    },
}

impl CheckedConversionKind {
    /// Creates an element-wise tuple conversion plan.
    pub fn tuple(elements: impl IntoIterator<Item = CheckedConversion>) -> Self {
        Self::Tuple(shared_slice(elements))
    }
}

/// A checked conversion with explicit source, target, and recursive operation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CheckedConversion {
    source: TypeId,
    target: TypeId,
    kind: CheckedConversionKind,
}

impl CheckedConversion {
    /// Creates a checked conversion plan.
    pub const fn new(source: TypeId, target: TypeId, kind: CheckedConversionKind) -> Self {
        Self {
            source,
            target,
            kind,
        }
    }

    /// Returns the exact checked source type.
    pub const fn source(&self) -> TypeId {
        self.source
    }

    /// Returns the exact checked target type.
    pub const fn target(&self) -> TypeId {
        self.target
    }

    /// Returns the selected built-in, composite, or user-defined operation.
    pub const fn kind(&self) -> &CheckedConversionKind {
        &self.kind
    }

    pub(crate) fn is_structurally_valid(&self) -> bool {
        match &self.kind {
            CheckedConversionKind::Identity => self.source == self.target,
            CheckedConversionKind::Scalar(_) | CheckedConversionKind::UserDefined { .. } => true,
            CheckedConversionKind::Tuple(elements) => {
                elements.iter().all(Self::is_structurally_valid)
            }
            CheckedConversionKind::Array(element) | CheckedConversionKind::Nullable(element) => {
                element.is_structurally_valid()
            }
            CheckedConversionKind::TupleToComplex { real, imaginary } => {
                real.is_structurally_valid() && imaginary.is_structurally_valid()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CheckedConversion, CheckedConversionKind, CheckedScalarConversion};

    #[test]
    fn composite_conversions_retain_recursive_checked_operations() {
        let ty = crate::test_support::error_type();
        let element = CheckedConversion::new(
            ty,
            ty,
            CheckedConversionKind::Scalar(CheckedScalarConversion::SignedIntegerWidening),
        );
        let conversion = CheckedConversion::new(
            ty,
            ty,
            CheckedConversionKind::tuple([element.clone(), element]),
        );

        let CheckedConversionKind::Tuple(elements) = conversion.kind() else {
            panic!("tuple conversion must retain element plans");
        };

        assert_eq!(elements.len(), 2);
        assert_eq!(elements[0].source(), ty);
        assert_eq!(elements[1].target(), ty);
    }

    #[test]
    fn identity_conversions_require_identical_source_and_target_types() {
        let values = crate::test_support::semantic_values();
        let source = crate::test_support::error_type_in(&values);
        let target = crate::test_support::tuple_type_in(&values);
        let conversion = CheckedConversion::new(source, target, CheckedConversionKind::Identity);

        assert!(!conversion.is_structurally_valid());
    }
}
