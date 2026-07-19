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
        /// Contiguous projection through `SliceIndex.slice`.
        SliceIndex => "SliceIndex",
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
                | Self::SliceIndex
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

#[cfg(any(test, feature = "generation"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CompilerKnownOperationComponentError {
    Duplicate,
    Incompatible,
}

#[cfg(any(test, feature = "generation"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CompilerKnownOperationComponents<T> {
    trait_definition: Option<T>,
    associated_result_type: Option<T>,
    fixed_callable_result_type: Option<T>,
    callable: Option<T>,
}

#[cfg(any(test, feature = "generation"))]
impl<T> CompilerKnownOperationComponents<T> {
    pub(crate) const fn new() -> Self {
        Self {
            trait_definition: None,
            associated_result_type: None,
            fixed_callable_result_type: None,
            callable: None,
        }
    }

    pub(crate) fn insert(
        &mut self,
        role: CompilerKnownOperationRole,
        declaration_kind: crate::catalog::CatalogDeclarationKind,
        value: T,
    ) -> Result<(), CompilerKnownOperationComponentError> {
        use crate::catalog::CatalogDeclarationKind;

        let slot = match (role.contract_shape(), declaration_kind) {
            (_, CatalogDeclarationKind::Trait) => &mut self.trait_definition,
            (
                CompilerKnownOperationContractShape::AssociatedResultCallable,
                CatalogDeclarationKind::TraitTypeMember,
            ) => &mut self.associated_result_type,
            (
                CompilerKnownOperationContractShape::FixedCallableResult,
                CatalogDeclarationKind::Struct | CatalogDeclarationKind::Union,
            ) => &mut self.fixed_callable_result_type,
            (
                CompilerKnownOperationContractShape::Callable
                | CompilerKnownOperationContractShape::FixedCallableResult
                | CompilerKnownOperationContractShape::AssociatedResultCallable,
                CatalogDeclarationKind::TraitCallableMember,
            ) => &mut self.callable,
            _ => return Err(CompilerKnownOperationComponentError::Incompatible),
        };

        if slot.is_some() {
            return Err(CompilerKnownOperationComponentError::Duplicate);
        }

        *slot = Some(value);

        Ok(())
    }

    pub(crate) const fn is_complete(&self, role: CompilerKnownOperationRole) -> bool {
        match role.contract_shape() {
            CompilerKnownOperationContractShape::Trait => self.trait_definition.is_some(),
            CompilerKnownOperationContractShape::Callable => {
                self.trait_definition.is_some() && self.callable.is_some()
            }
            CompilerKnownOperationContractShape::FixedCallableResult => {
                self.trait_definition.is_some()
                    && self.fixed_callable_result_type.is_some()
                    && self.callable.is_some()
            }
            CompilerKnownOperationContractShape::AssociatedResultCallable => {
                self.trait_definition.is_some()
                    && self.associated_result_type.is_some()
                    && self.callable.is_some()
            }
        }
    }
}

#[cfg(any(test, feature = "generation"))]
impl<T: Copy> CompilerKnownOperationComponents<T> {
    pub(crate) const fn trait_definition(&self) -> Option<T> {
        self.trait_definition
    }

    pub(crate) const fn associated_result_type(&self) -> Option<T> {
        self.associated_result_type
    }

    pub(crate) const fn fixed_callable_result_type(&self) -> Option<T> {
        self.fixed_callable_result_type
    }

    pub(crate) const fn callable(&self) -> Option<T> {
        self.callable
    }
}

#[cfg(test)]
mod tests {
    use crate::catalog::CatalogDeclarationKind;

    use super::{
        CompilerKnownOperationComponentError, CompilerKnownOperationComponents,
        CompilerKnownOperationContractShape, CompilerKnownOperationRole,
    };

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

    #[test]
    fn component_collection_uses_the_closed_contract_shape() {
        let mut comparison = CompilerKnownOperationComponents::new();

        assert_eq!(
            comparison.insert(
                CompilerKnownOperationRole::Comparison,
                CatalogDeclarationKind::Trait,
                1,
            ),
            Ok(())
        );

        assert_eq!(
            comparison.insert(
                CompilerKnownOperationRole::Comparison,
                CatalogDeclarationKind::Union,
                2,
            ),
            Ok(())
        );

        assert_eq!(
            comparison.insert(
                CompilerKnownOperationRole::Comparison,
                CatalogDeclarationKind::TraitCallableMember,
                3,
            ),
            Ok(())
        );

        assert!(comparison.is_complete(CompilerKnownOperationRole::Comparison));

        assert_eq!(
            comparison.insert(
                CompilerKnownOperationRole::Comparison,
                CatalogDeclarationKind::Struct,
                4,
            ),
            Err(CompilerKnownOperationComponentError::Duplicate)
        );

        let mut plain_conversion = CompilerKnownOperationComponents::new();

        assert_eq!(
            plain_conversion.insert(
                CompilerKnownOperationRole::PlainConversion,
                CatalogDeclarationKind::TraitTypeMember,
                1,
            ),
            Err(CompilerKnownOperationComponentError::Incompatible)
        );
    }
}
