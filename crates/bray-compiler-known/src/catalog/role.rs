use std::borrow::Cow;

use crate::{ImplementationHook, RepresentationRole};

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
}

impl CompilerKnownCatalogRoleRegistry {
    #[cfg(any(test, feature = "generation"))]
    pub(super) fn from_descriptors(
        declarations: &[CompilerKnownDeclarationDescriptor],
        values: &[CompilerKnownValueDescriptor],
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

        Self {
            representations: representations.into(),
            implementations: implementations.into(),
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
}

#[cfg(test)]
mod tests {
    use crate::{
        COMPILER_KNOWN_CATALOG, ImplementationHook, RepresentationRole,
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
}
