define_catalog_enum! {
    /// Identifies one language-defined expression operation contract.
    #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
    pub enum CompilerKnownOperationRole {
        /// Prefix arithmetic negation through `Negate.negate`.
        UnaryNegate => "UnaryNegate",
        /// Prefix bitwise complement through `BitNot.bit_not`.
        UnaryBitNot => "UnaryBitNot",
        /// Infix addition through `Add.add`.
        BinaryAdd => "BinaryAdd",
        /// Infix subtraction through `Subtract.subtract`.
        BinarySubtract => "BinarySubtract",
        /// Infix multiplication through `Multiply.multiply`.
        BinaryMultiply => "BinaryMultiply",
        /// Infix division through `Divide.divide`.
        BinaryDivide => "BinaryDivide",
        /// Infix remainder through `Remainder.remainder`.
        BinaryRemainder => "BinaryRemainder",
        /// Infix exponentiation through `Exponentiate.exponentiate`.
        BinaryExponentiate => "BinaryExponentiate",
        /// Infix matrix multiplication through `MatrixMultiply.matrix_multiply`.
        BinaryMatrixMultiply => "BinaryMatrixMultiply",
        /// Infix bitwise conjunction through `BitAnd.bit_and`.
        BinaryBitAnd => "BinaryBitAnd",
        /// Infix bitwise disjunction through `BitOr.bit_or`.
        BinaryBitOr => "BinaryBitOr",
        /// Infix bitwise exclusive disjunction through `BitXor.bit_xor`.
        BinaryBitXor => "BinaryBitXor",
        /// Infix left shift through `ShiftLeft.shift_left`.
        BinaryShiftLeft => "BinaryShiftLeft",
        /// Infix right shift through `ShiftRight.shift_right`.
        BinaryShiftRight => "BinaryShiftRight",
        /// Equality and inequality through `Equatable.equals`.
        Equality => "Equality",
        /// Relational comparison through `Comparable.compare`.
        Comparison => "Comparison",
        /// Plain conversion through `ConvertTo.convert`.
        PlainConversion => "PlainConversion",
        /// Element projection through `ElementIndex.index`.
        ElementIndex => "ElementIndex",
        /// Mutable element projection through `MutableElementIndex.index`.
        MutableElementIndex => "MutableElementIndex",
        /// Contiguous projection through `SliceIndex.slice`.
        SliceIndex => "SliceIndex",
        /// Mutable contiguous projection through `MutableSliceIndex.slice`.
        MutableSliceIndex => "MutableSliceIndex",
        /// Owned-indirection construction supported by `Storage`.
        BoxConstruction => "BoxConstruction",
    }
}

impl CompilerKnownOperationRole {
    #[cfg(any(test, feature = "generation"))]
    pub(crate) const fn contract_shape(self) -> CompilerKnownOperationContractShape {
        if matches!(
            self,
            Self::UnaryNegate
                | Self::UnaryBitNot
                | Self::BinaryAdd
                | Self::BinarySubtract
                | Self::BinaryMultiply
                | Self::BinaryDivide
                | Self::BinaryRemainder
                | Self::BinaryExponentiate
                | Self::BinaryMatrixMultiply
                | Self::BinaryBitAnd
                | Self::BinaryBitOr
                | Self::BinaryBitXor
                | Self::BinaryShiftLeft
                | Self::BinaryShiftRight
                | Self::ElementIndex
                | Self::MutableElementIndex
                | Self::SliceIndex
                | Self::MutableSliceIndex
        ) {
            CompilerKnownOperationContractShape::AssociatedResultCallable
        } else if matches!(self, Self::Comparison) {
            CompilerKnownOperationContractShape::FixedCallableResult
        } else if matches!(self, Self::BoxConstruction) {
            CompilerKnownOperationContractShape::Trait
        } else {
            CompilerKnownOperationContractShape::Callable
        }
    }
}

#[cfg(any(test, feature = "generation"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CompilerKnownOperationContractShape {
    Trait,
    Callable,
    FixedCallableResult,
    AssociatedResultCallable,
}

#[cfg(test)]
mod tests {
    use super::{CompilerKnownOperationContractShape, CompilerKnownOperationRole};

    #[test]
    fn operation_roles_expose_their_canonical_catalog_spelling() {
        for role in CompilerKnownOperationRole::ALL {
            assert_eq!(
                CompilerKnownOperationRole::from_catalog_spelling(role.as_str()),
                Some(*role)
            );
        }
    }

    #[test]
    fn operation_shapes_distinguish_associated_results_and_type_forms() {
        assert_eq!(
            CompilerKnownOperationRole::BinaryAdd.contract_shape(),
            CompilerKnownOperationContractShape::AssociatedResultCallable
        );

        assert_eq!(
            CompilerKnownOperationRole::PlainConversion.contract_shape(),
            CompilerKnownOperationContractShape::Callable
        );

        assert_eq!(
            CompilerKnownOperationRole::Comparison.contract_shape(),
            CompilerKnownOperationContractShape::FixedCallableResult
        );

        assert_eq!(
            CompilerKnownOperationRole::BoxConstruction.contract_shape(),
            CompilerKnownOperationContractShape::Trait
        );
    }
}
