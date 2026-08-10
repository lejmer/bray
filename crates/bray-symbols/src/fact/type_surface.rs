use std::collections::BTreeSet;
use std::sync::Arc;

use crate::{
    AnySymbolId, GenericDeclarationTemplate, InherentImplementationSymbolId, MemberEntry,
    MemberLookupIndex, MemberLookupResult, NamedTypeSymbolId, TargetFactDependency,
};

/// Identifies one type-wide lifecycle declaration category.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TypeAssociatedLifecycleSlot {
    /// The unnamed primary constructor.
    PrimaryConstructor,
    /// The type finalizer.
    Finalizer,
    /// The type destructor.
    Destructor,
    /// The scoped-use entry declaration.
    ScopeEnter,
    /// The scoped-use exit declaration.
    ScopeExit,
}

/// Identifies where one member enters a named type's associated surface.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TypeAssociatedMemberOrigin {
    /// The named type declares the member directly.
    Direct,
    /// An inherent implementation contributes the member.
    InherentImplementation(InherentImplementationSymbolId),
}

/// One member retained by a named type's associated surface.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct TypeAssociatedMember {
    id: AnySymbolId,
    entry: Option<MemberEntry<AnySymbolId>>,
    origin: TypeAssociatedMemberOrigin,
}

impl TypeAssociatedMember {
    const fn new(
        id: AnySymbolId,
        entry: Option<MemberEntry<AnySymbolId>>,
        origin: TypeAssociatedMemberOrigin,
    ) -> Self {
        Self { id, entry, origin }
    }

    /// Returns the exact member identity.
    pub const fn id(&self) -> AnySymbolId {
        self.id
    }

    /// Returns the member's ordinary-name metadata when valid name syntax is present.
    pub const fn entry(&self) -> Option<&MemberEntry<AnySymbolId>> {
        self.entry.as_ref()
    }

    /// Returns whether recovery prevents this member from entering ordinary-name lookup.
    pub fn is_recovered(&self) -> bool {
        self.entry
            .as_ref()
            .is_none_or(|entry| entry.validity() == crate::MemberValidity::Malformed)
    }

    /// Returns where the member enters the associated surface.
    pub const fn origin(&self) -> TypeAssociatedMemberOrigin {
        self.origin
    }
}

macro_rules! type_associated_member_accessors {
    () => {
        /// Iterates struct fields in canonical declaration order.
        pub fn fields(&self) -> impl Iterator<Item = crate::StructFieldSymbolId> + '_ {
            self.members.iter().filter_map(|member| match member.id {
                AnySymbolId::StructField(id) => Some(id),
                _ => None,
            })
        }

        /// Iterates union variants in canonical declaration order.
        pub fn variants(&self) -> impl Iterator<Item = crate::UnionVariantSymbolId> + '_ {
            self.members.iter().filter_map(|member| match member.id {
                AnySymbolId::UnionVariant(id) => Some(id),
                _ => None,
            })
        }

        /// Iterates callable members in canonical declaration order.
        pub fn callable_members(
            &self,
        ) -> impl Iterator<Item = crate::TypeCallableMemberSymbolId> + '_ {
            self.members.iter().filter_map(|member| match member.id {
                AnySymbolId::TypeCallableMember(id) => Some(id),
                _ => None,
            })
        }

        /// Iterates constructors in canonical declaration order.
        pub fn constructors(&self) -> impl Iterator<Item = crate::ConstructorSymbolId> + '_ {
            self.members.iter().filter_map(|member| match member.id {
                AnySymbolId::Constructor(id) => Some(id),
                _ => None,
            })
        }

        /// Iterates constants in canonical declaration order.
        pub fn constants(&self) -> impl Iterator<Item = crate::ConstantSymbolId> + '_ {
            self.members.iter().filter_map(|member| match member.id {
                AnySymbolId::Constant(id) => Some(id),
                _ => None,
            })
        }

        /// Iterates predicates in canonical declaration order.
        pub fn predicates(&self) -> impl Iterator<Item = crate::PredicateSymbolId> + '_ {
            self.members.iter().filter_map(|member| match member.id {
                AnySymbolId::Predicate(id) => Some(id),
                _ => None,
            })
        }

        /// Iterates type-valued members in canonical declaration order.
        pub fn type_members(&self) -> impl Iterator<Item = crate::InherentTypeMemberSymbolId> + '_ {
            self.members.iter().filter_map(|member| match member.id {
                AnySymbolId::InherentTypeMember(id) => Some(id),
                _ => None,
            })
        }

        /// Iterates callable overload families in canonical declaration order.
        pub fn callable_overloads(
            &self,
        ) -> impl Iterator<Item = crate::CallableOverloadSymbolId> + '_ {
            self.members.iter().filter_map(|member| match member.id {
                AnySymbolId::CallableOverload(id) => Some(id),
                _ => None,
            })
        }
    };
}

/// One declaration retained in a named type's typed lifecycle slots.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TypeAssociatedLifecycleMember {
    id: AnySymbolId,
    slot: TypeAssociatedLifecycleSlot,
    origin: TypeAssociatedMemberOrigin,
}

impl TypeAssociatedLifecycleMember {
    const fn new(
        id: AnySymbolId,
        slot: TypeAssociatedLifecycleSlot,
        origin: TypeAssociatedMemberOrigin,
    ) -> Self {
        Self { id, slot, origin }
    }

    /// Returns the exact lifecycle declaration identity.
    pub const fn id(self) -> AnySymbolId {
        self.id
    }

    /// Returns the occupied lifecycle slot.
    pub const fn slot(self) -> TypeAssociatedLifecycleSlot {
        self.slot
    }

    /// Returns where the lifecycle declaration enters the associated surface.
    pub const fn origin(self) -> TypeAssociatedMemberOrigin {
        self.origin
    }
}

/// Members and applicability metadata contributed by one inherent implementation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct TypeAssociatedImplementation {
    implementation: InherentImplementationSymbolId,
    generic: GenericDeclarationTemplate,
    target_dependencies: Arc<[TargetFactDependency]>,
    members: Arc<[TypeAssociatedMember]>,
    lifecycle_members: Arc<[TypeAssociatedLifecycleMember]>,
}

impl TypeAssociatedImplementation {
    /// Creates one inherent implementation contribution.
    pub fn new(
        implementation: InherentImplementationSymbolId,
        generic: GenericDeclarationTemplate,
        target_dependencies: impl IntoIterator<Item = TargetFactDependency>,
        members: impl IntoIterator<Item = (AnySymbolId, Option<MemberEntry<AnySymbolId>>)>,
        lifecycle_members: impl IntoIterator<Item = (AnySymbolId, TypeAssociatedLifecycleSlot)>,
    ) -> Self {
        let mut target_dependencies = target_dependencies.into_iter().collect::<Vec<_>>();
        let origin = TypeAssociatedMemberOrigin::InherentImplementation(implementation);

        target_dependencies.sort();
        target_dependencies.dedup();

        Self {
            implementation,
            generic,
            target_dependencies: target_dependencies.into(),
            members: members
                .into_iter()
                .map(|(id, entry)| TypeAssociatedMember::new(id, entry, origin))
                .collect(),
            lifecycle_members: lifecycle_members
                .into_iter()
                .map(|(id, slot)| TypeAssociatedLifecycleMember::new(id, slot, origin))
                .collect(),
        }
    }

    /// Returns the contributing implementation.
    pub const fn implementation(&self) -> InherentImplementationSymbolId {
        self.implementation
    }

    /// Returns the implementation's open generic declaration template.
    pub const fn generic(&self) -> &GenericDeclarationTemplate {
        &self.generic
    }

    /// Returns target facts required by this contribution.
    pub fn target_dependencies(&self) -> &[TargetFactDependency] {
        &self.target_dependencies
    }

    /// Returns one exact contributed member when it belongs to this implementation.
    pub fn member(&self, id: AnySymbolId) -> Option<&TypeAssociatedMember> {
        self.members.iter().find(|member| member.id == id)
    }

    type_associated_member_accessors!();

    /// Returns contributed lifecycle declarations in source order.
    pub fn lifecycle_members(&self) -> &[TypeAssociatedLifecycleMember] {
        &self.lifecycle_members
    }
}

/// Reports malformed input while assembling a type-associated surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TypeAssociatedSurfaceBuildError {
    /// The same member identity appears more than once.
    DuplicateMember(AnySymbolId),
    /// The same inherent implementation contributes more than once.
    DuplicateImplementation(InherentImplementationSymbolId),
}

/// The immutable declaration-level member surface associated with one named type.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct TypeAssociatedSurface {
    subject: NamedTypeSymbolId,
    generic: GenericDeclarationTemplate,
    members: Arc<[TypeAssociatedMember]>,
    lifecycle_members: Arc<[TypeAssociatedLifecycleMember]>,
    implementations: Arc<[TypeAssociatedImplementation]>,
    lookup: MemberLookupIndex<AnySymbolId>,
}

impl TypeAssociatedSurface {
    /// Aggregates direct members followed by canonical inherent implementation contributions.
    pub fn try_new(
        subject: NamedTypeSymbolId,
        generic: GenericDeclarationTemplate,
        direct_members: impl IntoIterator<Item = (AnySymbolId, Option<MemberEntry<AnySymbolId>>)>,
        direct_lifecycle_members: impl IntoIterator<Item = (AnySymbolId, TypeAssociatedLifecycleSlot)>,
        implementations: impl IntoIterator<Item = TypeAssociatedImplementation>,
    ) -> Result<Self, TypeAssociatedSurfaceBuildError> {
        let implementations = implementations.into_iter().collect::<Vec<_>>();
        let mut implementation_ids = BTreeSet::new();

        if let Some(duplicate) = implementations.iter().find_map(|implementation| {
            (!implementation_ids.insert(implementation.implementation))
                .then_some(implementation.implementation)
        }) {
            return Err(TypeAssociatedSurfaceBuildError::DuplicateImplementation(
                duplicate,
            ));
        }

        let mut members = direct_members
            .into_iter()
            .map(|(id, entry)| {
                TypeAssociatedMember::new(id, entry, TypeAssociatedMemberOrigin::Direct)
            })
            .collect::<Vec<_>>();

        let mut lifecycle_members = direct_lifecycle_members
            .into_iter()
            .map(|(id, slot)| {
                TypeAssociatedLifecycleMember::new(id, slot, TypeAssociatedMemberOrigin::Direct)
            })
            .collect::<Vec<_>>();

        for implementation in &implementations {
            // Lookup and lifecycle indexes own immutable entries independently of contributions.
            members.extend(implementation.members.iter().cloned());

            lifecycle_members.extend(implementation.lifecycle_members.iter().copied());
        }

        let mut member_ids = BTreeSet::new();

        if let Some(duplicate) = members
            .iter()
            .map(TypeAssociatedMember::id)
            .find(|id| !member_ids.insert(*id))
        {
            return Err(TypeAssociatedSurfaceBuildError::DuplicateMember(duplicate));
        }

        // The ordinary-name index owns present immutable entries independently of enumeration.
        let lookup = MemberLookupIndex::new(
            members
                .iter()
                .filter_map(|member| member.entry.as_ref().cloned()),
        )
        .map_err(|error| match error {
            crate::MemberCollectionBuildError::DuplicateMember(member) => {
                TypeAssociatedSurfaceBuildError::DuplicateMember(member)
            }
        })?;

        Ok(Self {
            subject,
            generic,
            members: members.into(),
            lifecycle_members: lifecycle_members.into(),
            implementations: implementations.into(),
            lookup,
        })
    }

    /// Returns the named type definition owning this surface.
    pub const fn subject(&self) -> NamedTypeSymbolId {
        self.subject
    }

    /// Returns the named type's open generic declaration template.
    pub const fn generic(&self) -> &GenericDeclarationTemplate {
        &self.generic
    }

    /// Returns one exact direct or inherent member when it belongs to this surface.
    pub fn member(&self, id: AnySymbolId) -> Option<&TypeAssociatedMember> {
        self.members.iter().find(|member| member.id == id)
    }

    type_associated_member_accessors!();

    /// Returns all lifecycle declarations in canonical declaration order.
    pub fn lifecycle_members(&self) -> &[TypeAssociatedLifecycleMember] {
        &self.lifecycle_members
    }

    /// Returns inherent implementation contributions in canonical declaration order.
    pub fn implementations(&self) -> &[TypeAssociatedImplementation] {
        &self.implementations
    }

    /// Resolves one ordinary member name without applying contextual accessibility.
    pub fn lookup(&self, name: &str) -> MemberLookupResult<AnySymbolId> {
        self.lookup.lookup(name)
    }

    /// Resolves one ordinary member name through the public surface.
    pub fn lookup_public(&self, name: &str) -> MemberLookupResult<AnySymbolId> {
        self.lookup.lookup_public(name)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        AnySymbolId, GenericDeclarationTemplate, GenericOwnerId, InherentImplementationSymbolId,
        MemberEntry, MemberLookupResult, MemberValidity, MemberVisibility, NamedTypeSymbolId,
        StructSymbolId, SymbolId, SymbolName, TypeAssociatedImplementation,
        TypeAssociatedLifecycleSlot, TypeAssociatedMemberOrigin, TypeAssociatedSurface,
        TypeCallableMemberSymbolId,
    };

    #[test]
    fn direct_and_inherent_members_retain_order_conflicts_and_origins() {
        let subject = NamedTypeSymbolId::from(StructSymbolId::from_symbol_id(SymbolId::new(1)));
        let implementation = InherentImplementationSymbolId::from_symbol_id(SymbolId::new(2));
        let direct = TypeCallableMemberSymbolId::from_symbol_id(SymbolId::new(3));
        let inherent = TypeCallableMemberSymbolId::from_symbol_id(SymbolId::new(4));

        let subject_owner = GenericOwnerId::try_new(subject.into_any())
            .unwrap_or_else(|| panic!("named type must be a generic owner"));

        let implementation_owner = GenericOwnerId::try_new(implementation.into())
            .unwrap_or_else(|| panic!("inherent implementation must be a generic owner"));

        let lifecycle_id =
            AnySymbolId::from(crate::FinalizerSymbolId::from_symbol_id(SymbolId::new(5)));

        let surface = TypeAssociatedSurface::try_new(
            subject,
            GenericDeclarationTemplate::new(subject_owner, [], []),
            [member(direct.into(), "shared")],
            [],
            [TypeAssociatedImplementation::new(
                implementation,
                GenericDeclarationTemplate::new(implementation_owner, [], []),
                [],
                [member(inherent.into(), "shared")],
                [(lifecycle_id, TypeAssociatedLifecycleSlot::Finalizer)],
            )],
        )
        .unwrap_or_else(|error| panic!("test surface must build: {error:?}"));

        assert_eq!(
            surface.lookup("shared"),
            MemberLookupResult::Ambiguous(Box::new([direct.into(), inherent.into()]))
        );

        assert_eq!(
            surface.callable_members().collect::<Vec<_>>(),
            [direct, inherent]
        );

        assert_eq!(
            surface.member(direct.into()).map(|member| member.origin()),
            Some(TypeAssociatedMemberOrigin::Direct)
        );

        assert_eq!(
            surface
                .member(inherent.into())
                .map(|member| member.origin()),
            Some(TypeAssociatedMemberOrigin::InherentImplementation(
                implementation
            ))
        );

        let [lifecycle] = surface.lifecycle_members() else {
            panic!("test surface must retain one lifecycle member");
        };

        assert_eq!(lifecycle.id(), lifecycle_id);
        assert_eq!(lifecycle.slot(), TypeAssociatedLifecycleSlot::Finalizer);

        assert_eq!(
            lifecycle.origin(),
            TypeAssociatedMemberOrigin::InherentImplementation(implementation)
        );
    }

    fn member(id: AnySymbolId, name: &str) -> (AnySymbolId, Option<MemberEntry<AnySymbolId>>) {
        let name = SymbolName::try_new(name)
            .unwrap_or_else(|| panic!("test member name must be valid: {name}"));

        (
            id,
            Some(MemberEntry::new(
                id,
                name,
                MemberVisibility::Public,
                MemberValidity::Valid,
            )),
        )
    }
}
