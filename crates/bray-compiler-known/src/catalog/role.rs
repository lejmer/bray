use std::borrow::Cow;
#[cfg(any(test, feature = "generation"))]
use std::collections::BTreeMap;

use crate::{CompilerKnownOperationRole, ImplementationHook, RepresentationRole};

#[cfg(any(test, feature = "generation"))]
use super::operation::CompilerKnownOperationComponents;

#[cfg(any(test, feature = "generation"))]
use super::{CompilerKnownDeclarationDescriptor, CompilerKnownValueDescriptor};
use super::{CompilerKnownDeclarationId, CompilerKnownValueId};

/// The catalog entry carrying one language-defined representation role.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CompilerKnownRepresentationTarget {
    /// An ordinary compiler-known declaration has this representation.
    Declaration(CompilerKnownDeclarationId),
    /// A language-known special value has this representation.
    Value(CompilerKnownValueId),
}

/// One immutable typed representation-role binding.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CompilerKnownRepresentationBinding {
    pub(super) role: RepresentationRole,
    pub(super) target: CompilerKnownRepresentationTarget,
}

impl CompilerKnownRepresentationBinding {
    /// Returns the closed representation role selected by the catalog.
    pub const fn role(self) -> RepresentationRole {
        self.role
    }

    /// Returns the declaration or special value carrying the role.
    pub const fn target(self) -> CompilerKnownRepresentationTarget {
        self.target
    }
}

/// One immutable typed implementation-hook binding.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CompilerKnownImplementationBinding {
    pub(super) hook: ImplementationHook,
    pub(super) declaration: CompilerKnownDeclarationId,
}

/// One validated expression operation contract in catalog identity space.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CompilerKnownOperationBinding {
    pub(super) role: CompilerKnownOperationRole,
    pub(super) trait_definition: CompilerKnownDeclarationId,
    pub(super) result_type_member: Option<CompilerKnownDeclarationId>,
    pub(super) fixed_callable_result_type: Option<CompilerKnownDeclarationId>,
    pub(super) callable: Option<CompilerKnownDeclarationId>,
}

impl CompilerKnownOperationBinding {
    /// Returns the closed expression operation role.
    pub const fn role(self) -> CompilerKnownOperationRole {
        self.role
    }

    /// Returns the exact compiler-known trait declaration.
    pub const fn trait_definition(self) -> CompilerKnownDeclarationId {
        self.trait_definition
    }

    /// Returns the associated result member used by this operation, when required.
    pub const fn result_type_member(self) -> Option<CompilerKnownDeclarationId> {
        self.result_type_member
    }

    /// Returns the exact named result type required from the selected callable, when fixed.
    pub const fn fixed_callable_result_type(self) -> Option<CompilerKnownDeclarationId> {
        self.fixed_callable_result_type
    }

    /// Returns the trait callable selected by this operation, when it has one.
    pub const fn callable(self) -> Option<CompilerKnownDeclarationId> {
        self.callable
    }
}

impl CompilerKnownImplementationBinding {
    /// Returns the compiler-provided implementation hook.
    pub const fn hook(self) -> ImplementationHook {
        self.hook
    }

    /// Returns the declaration whose behavior is supplied by the hook.
    pub const fn declaration(self) -> CompilerKnownDeclarationId {
        self.declaration
    }
}

/// Immutable typed indexes over compiler-known semantic roles.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompilerKnownCatalogRoleRegistry {
    pub(super) representations: Cow<'static, [CompilerKnownRepresentationBinding]>,
    pub(super) implementations: Cow<'static, [CompilerKnownImplementationBinding]>,
    pub(super) operations: Cow<'static, [CompilerKnownOperationBinding]>,
}

impl CompilerKnownCatalogRoleRegistry {
    #[cfg(any(test, feature = "generation"))]
    pub(super) fn from_descriptors(
        declarations: &[CompilerKnownDeclarationDescriptor],
        values: &[CompilerKnownValueDescriptor],
        operation_declarations: impl IntoIterator<
            Item = (
                CompilerKnownOperationRole,
                CompilerKnownDeclarationId,
                super::CatalogDeclarationKind,
            ),
        >,
    ) -> Self {
        let mut representations = declarations
            .iter()
            .filter_map(|declaration| {
                Some(CompilerKnownRepresentationBinding {
                    role: declaration.representation_role()?,
                    target: CompilerKnownRepresentationTarget::Declaration(declaration.id()),
                })
            })
            .chain(
                values
                    .iter()
                    .map(|value| CompilerKnownRepresentationBinding {
                        role: value.representation_role(),
                        target: CompilerKnownRepresentationTarget::Value(value.id()),
                    }),
            )
            .collect::<Vec<_>>();

        representations.sort_unstable_by_key(|binding| binding.role);

        let mut implementations = declarations
            .iter()
            .filter_map(|declaration| {
                Some(CompilerKnownImplementationBinding {
                    hook: declaration.implementation_hook()?,
                    declaration: declaration.id(),
                })
            })
            .collect::<Vec<_>>();

        implementations.sort_unstable();

        let mut operation_components = BTreeMap::new();

        for (role, declaration, kind) in operation_declarations {
            let components = operation_components
                .entry(role)
                .or_insert_with(CompilerKnownOperationComponents::new);

            let insert_result = components.insert(role, kind, declaration);

            debug_assert!(
                insert_result.is_ok(),
                "operation role registry input must already be validated"
            );
        }

        let operations = CompilerKnownOperationRole::ALL
            .iter()
            .filter_map(|role| {
                let components = operation_components.get(role)?;

                if !components.is_complete(*role) {
                    return None;
                }

                Some(CompilerKnownOperationBinding {
                    role: *role,
                    trait_definition: components.trait_definition()?,
                    result_type_member: components.associated_result_type(),
                    fixed_callable_result_type: components.fixed_callable_result_type(),
                    callable: components.callable(),
                })
            })
            .collect::<Vec<_>>();

        Self {
            representations: representations.into(),
            implementations: implementations.into(),
            operations: operations.into(),
        }
    }

    /// Returns representation bindings in stable role order.
    pub fn representations(&self) -> &[CompilerKnownRepresentationBinding] {
        &self.representations
    }

    /// Returns implementation bindings in stable hook and declaration order.
    pub fn implementations(&self) -> &[CompilerKnownImplementationBinding] {
        &self.implementations
    }

    /// Returns expression operation contracts in stable role order.
    pub fn operations(&self) -> &[CompilerKnownOperationBinding] {
        &self.operations
    }

    /// Resolves the unique declaration or special value carrying a representation role.
    pub fn representation_target(
        &self,
        role: RepresentationRole,
    ) -> Option<CompilerKnownRepresentationTarget> {
        self.representations
            .binary_search_by_key(&role, |binding| binding.role)
            .ok()
            .and_then(|index| self.representations.get(index))
            .map(|binding| binding.target)
    }

    /// Returns every declaration carrying one implementation hook.
    pub fn implementation_declarations(
        &self,
        hook: ImplementationHook,
    ) -> impl Iterator<Item = CompilerKnownDeclarationId> + '_ {
        let start = self
            .implementations
            .partition_point(|binding| binding.hook < hook);

        let end = self
            .implementations
            .partition_point(|binding| binding.hook <= hook);

        self.implementations[start..end]
            .iter()
            .map(|binding| binding.declaration)
    }

    /// Resolves the exact declarations assigned to one expression operation role.
    pub fn operation_contract(
        &self,
        role: CompilerKnownOperationRole,
    ) -> Option<CompilerKnownOperationBinding> {
        self.operations
            .binary_search_by_key(&role, |binding| binding.role)
            .ok()
            .and_then(|index| self.operations.get(index))
            .copied()
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        COMPILER_KNOWN_CATALOG, CatalogDeclarationKind, CompilerKnownDeclarationId,
        CompilerKnownOperationRole, ImplementationHook, RepresentationRole,
        catalog::CompilerKnownRepresentationTarget,
    };

    #[test]
    fn generated_roles_resolve_without_catalog_keys_or_names() {
        let roles = COMPILER_KNOWN_CATALOG.role_registry();

        let bool_target = roles.representation_target(RepresentationRole::ScalarBool);
        let true_target = roles.representation_target(RepresentationRole::BooleanTrue);

        let memory_copy = roles
            .implementation_declarations(ImplementationHook::MemoryCopy)
            .collect::<Vec<_>>();

        assert!(matches!(
            bool_target,
            Some(CompilerKnownRepresentationTarget::Declaration(_))
        ));

        assert!(matches!(
            true_target,
            Some(CompilerKnownRepresentationTarget::Value(_))
        ));

        assert_eq!(memory_copy.len(), 1);

        assert_eq!(
            memory_copy
                .first()
                .and_then(
                    |declaration| COMPILER_KNOWN_CATALOG.compiler_known_declaration(*declaration)
                )
                .and_then(|declaration| declaration.implementation_hook()),
            Some(ImplementationHook::MemoryCopy)
        );
    }

    #[test]
    fn generated_operation_contracts_cover_the_complete_expression_surface() {
        let expected = [
            (
                CompilerKnownOperationRole::UnaryNegate,
                "Negate",
                Some("NegateOutput"),
                Some("NegateCall"),
            ),
            (
                CompilerKnownOperationRole::UnaryBitNot,
                "BitNot",
                Some("BitNotOutput"),
                Some("BitNotCall"),
            ),
            (
                CompilerKnownOperationRole::BinaryAdd,
                "Add",
                Some("AddOutput"),
                Some("AddCall"),
            ),
            (
                CompilerKnownOperationRole::BinarySubtract,
                "Subtract",
                Some("SubtractOutput"),
                Some("SubtractCall"),
            ),
            (
                CompilerKnownOperationRole::BinaryMultiply,
                "Multiply",
                Some("MultiplyOutput"),
                Some("MultiplyCall"),
            ),
            (
                CompilerKnownOperationRole::BinaryDivide,
                "Divide",
                Some("DivideOutput"),
                Some("DivideCall"),
            ),
            (
                CompilerKnownOperationRole::BinaryRemainder,
                "Remainder",
                Some("RemainderOutput"),
                Some("RemainderCall"),
            ),
            (
                CompilerKnownOperationRole::BinaryExponentiate,
                "Exponentiate",
                Some("ExponentiateOutput"),
                Some("ExponentiateCall"),
            ),
            (
                CompilerKnownOperationRole::BinaryMatrixMultiply,
                "MatrixMultiply",
                Some("MatrixMultiplyOutput"),
                Some("MatrixMultiplyCall"),
            ),
            (
                CompilerKnownOperationRole::BinaryBitAnd,
                "BitAnd",
                Some("BitAndOutput"),
                Some("BitAndCall"),
            ),
            (
                CompilerKnownOperationRole::BinaryBitOr,
                "BitOr",
                Some("BitOrOutput"),
                Some("BitOrCall"),
            ),
            (
                CompilerKnownOperationRole::BinaryBitXor,
                "BitXor",
                Some("BitXorOutput"),
                Some("BitXorCall"),
            ),
            (
                CompilerKnownOperationRole::BinaryShiftLeft,
                "ShiftLeft",
                Some("ShiftLeftOutput"),
                Some("ShiftLeftCall"),
            ),
            (
                CompilerKnownOperationRole::BinaryShiftRight,
                "ShiftRight",
                Some("ShiftRightOutput"),
                Some("ShiftRightCall"),
            ),
            (
                CompilerKnownOperationRole::Equality,
                "Equatable",
                None,
                Some("EquatableCall"),
            ),
            (
                CompilerKnownOperationRole::Comparison,
                "Comparable",
                None,
                Some("ComparableCall"),
            ),
            (
                CompilerKnownOperationRole::PlainConversion,
                "ConvertTo",
                None,
                Some("ConvertToCall"),
            ),
            (
                CompilerKnownOperationRole::ElementIndex,
                "ElementIndex",
                Some("ElementIndexOutput"),
                Some("ElementIndexCall"),
            ),
            (
                CompilerKnownOperationRole::SliceIndex,
                "SliceIndex",
                Some("SliceIndexOutput"),
                Some("SliceIndexCall"),
            ),
            (
                CompilerKnownOperationRole::BoxConstruction,
                "Storage",
                None,
                None,
            ),
        ];

        let roles = COMPILER_KNOWN_CATALOG.role_registry();

        assert_eq!(
            roles.operations().len(),
            CompilerKnownOperationRole::ALL.len()
        );

        for (role, trait_key, result_key, callable_key) in expected {
            let Some(contract) = roles.operation_contract(role) else {
                panic!("{role:?} must have a generated operation contract");
            };

            assert_eq!(declaration_key(contract.trait_definition()), trait_key);

            assert_eq!(
                contract.result_type_member().map(declaration_key),
                result_key
            );

            let fixed_callable_result_key =
                (role == CompilerKnownOperationRole::Comparison).then_some("Ordering");

            assert_eq!(
                contract.fixed_callable_result_type().map(declaration_key),
                fixed_callable_result_key
            );

            assert_eq!(contract.callable().map(declaration_key), callable_key);
        }
    }

    #[test]
    fn operation_contract_order_is_independent_of_component_input_order() {
        let components = [
            (
                CompilerKnownOperationRole::BinaryAdd,
                CompilerKnownDeclarationId::new(2),
                CatalogDeclarationKind::TraitCallableMember,
            ),
            (
                CompilerKnownOperationRole::BinaryAdd,
                CompilerKnownDeclarationId::new(0),
                CatalogDeclarationKind::Trait,
            ),
            (
                CompilerKnownOperationRole::BinaryAdd,
                CompilerKnownDeclarationId::new(1),
                CatalogDeclarationKind::TraitTypeMember,
            ),
            (
                CompilerKnownOperationRole::Comparison,
                CompilerKnownDeclarationId::new(5),
                CatalogDeclarationKind::TraitCallableMember,
            ),
            (
                CompilerKnownOperationRole::Comparison,
                CompilerKnownDeclarationId::new(3),
                CatalogDeclarationKind::Trait,
            ),
            (
                CompilerKnownOperationRole::Comparison,
                CompilerKnownDeclarationId::new(4),
                CatalogDeclarationKind::Union,
            ),
        ];

        let forward =
            super::CompilerKnownCatalogRoleRegistry::from_descriptors(&[], &[], components);

        let reversed = super::CompilerKnownCatalogRoleRegistry::from_descriptors(
            &[],
            &[],
            components.into_iter().rev(),
        );

        assert_eq!(forward, reversed);
    }

    #[test]
    fn execution_roles_resolve_the_minimal_language_surface() {
        let roles = COMPILER_KNOWN_CATALOG.role_registry();

        for (role, expected_key) in [
            (RepresentationRole::Future, "Future"),
            (RepresentationRole::Task, "Task"),
            (RepresentationRole::RunResult, "RunResult"),
            (RepresentationRole::PanicReport, "PanicReport"),
        ] {
            let Some(CompilerKnownRepresentationTarget::Declaration(declaration)) =
                roles.representation_target(role)
            else {
                panic!("execution representation role must resolve to a declaration");
            };

            assert_eq!(
                COMPILER_KNOWN_CATALOG
                    .compiler_known_declaration(declaration)
                    .map(|descriptor| descriptor.key().as_str()),
                Some(expected_key)
            );
        }

        for (hook, expected_key) in [
            (ImplementationHook::FutureStart, "FutureStart"),
            (ImplementationHook::TaskJoin, "TaskJoin"),
            (ImplementationHook::TaskCancel, "TaskCancel"),
            (ImplementationHook::BlockingExecution, "BlockingExecution"),
            (ImplementationHook::ComputeExecution, "ComputeExecution"),
            (
                ImplementationHook::MainThreadExecution,
                "MainThreadExecution",
            ),
        ] {
            let declarations = roles
                .implementation_declarations(hook)
                .filter_map(|declaration| {
                    COMPILER_KNOWN_CATALOG.compiler_known_declaration(declaration)
                })
                .map(|descriptor| descriptor.key().as_str())
                .collect::<Vec<_>>();

            assert_eq!(declarations, [expected_key]);
        }
    }

    fn declaration_key(declaration: super::CompilerKnownDeclarationId) -> &'static str {
        COMPILER_KNOWN_CATALOG
            .compiler_known_declaration(declaration)
            .map(|descriptor| descriptor.key().as_str())
            .unwrap_or_else(|| panic!("operation declaration must exist"))
    }
}
