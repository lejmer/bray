use crate::operation::{CompilerKnownOperationContractShape, CompilerKnownOperationRole};

use super::CatalogDeclarationKind;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CompilerKnownOperationComponentError {
    Duplicate,
    Incompatible,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CompilerKnownOperationComponents<T> {
    trait_definition: Option<T>,
    associated_result_type: Option<T>,
    fixed_callable_result_type: Option<T>,
    callable: Option<T>,
}

impl<T> CompilerKnownOperationComponents<T> {
    pub(super) const fn new() -> Self {
        Self {
            trait_definition: None,
            associated_result_type: None,
            fixed_callable_result_type: None,
            callable: None,
        }
    }

    pub(super) fn insert(
        &mut self,
        role: CompilerKnownOperationRole,
        declaration_kind: CatalogDeclarationKind,
        value: T,
    ) -> Result<(), CompilerKnownOperationComponentError> {
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

    pub(super) const fn is_complete(&self, role: CompilerKnownOperationRole) -> bool {
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

impl<T: Copy> CompilerKnownOperationComponents<T> {
    pub(super) const fn trait_definition(&self) -> Option<T> {
        self.trait_definition
    }

    pub(super) const fn associated_result_type(&self) -> Option<T> {
        self.associated_result_type
    }

    pub(super) const fn fixed_callable_result_type(&self) -> Option<T> {
        self.fixed_callable_result_type
    }

    pub(super) const fn callable(&self) -> Option<T> {
        self.callable
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CatalogDeclarationKind, CompilerKnownOperationComponentError,
        CompilerKnownOperationComponents, CompilerKnownOperationRole,
    };

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
