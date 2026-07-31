use std::collections::{BTreeMap, BTreeSet};

use bray_declarations::SyntaxAnchor;

use crate::{
    AnySymbolId, CallableContractSymbolId, CallableOverloadSymbolId, CallableParameterSymbolId,
    CallableSymbolId, ConstantSymbolId, ConstructorSymbolId, DestructorSymbolId, FinalizerSymbolId,
    FunctionSymbolId, GenericConstParameterSymbolId, GenericOwnerId, GenericTypeParameterSymbolId,
    ImplementationOverloadSymbolId, InherentImplementationSymbolId, InherentTypeMemberSymbolId,
    NamedTraitImplementationSymbolId, PredicateDefinitionSymbolId, PredicateParameterSymbolId,
    PredicateSymbolId, ReceiverParameterSymbolId, ScopeEnterSymbolId, ScopeExitSymbolId,
    StructFieldSymbolId, StructSymbolId, TraitCallableFulfillmentSymbolId,
    TraitCallableMemberSymbolId, TraitConstantFulfillmentSymbolId, TraitConstantMemberSymbolId,
    TraitDestructorRequirementSymbolId, TraitFinalizerRequirementSymbolId,
    TraitPredicateFulfillmentSymbolId, TraitPredicateMemberSymbolId,
    TraitScopeEnterFulfillmentSymbolId, TraitScopeEnterRequirementSymbolId,
    TraitScopeExitFulfillmentSymbolId, TraitScopeExitRequirementSymbolId, TraitSymbolId,
    TraitTypeFulfillmentSymbolId, TraitTypeMemberSymbolId, TrustedCapabilitySymbolId,
    TypeCallableMemberSymbolId, UnionPayloadDefaultProviderSymbolId, UnionPayloadFieldSymbolId,
    UnionSymbolId, UnionVariantSymbolId, UnnamedTraitImplementationSymbolId,
};

/// Describes whether declaration syntax supplies a runtime default.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeDefaultPresence {
    /// No default syntax was written.
    Absent,
    /// A complete default expression was written.
    Present,
    /// Default syntax exists but contains parser recovery.
    Recovered,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct LeafRelationships;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct GenericRelationships {
    pub(crate) type_parameters: Box<[GenericTypeParameterSymbolId]>,
    pub(crate) const_parameters: Box<[GenericConstParameterSymbolId]>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct CallableRelationships {
    pub(crate) generics: GenericRelationships,
    pub(crate) receiver: Option<ReceiverParameterSymbolId>,
    pub(crate) parameters: Box<[CallableParameterSymbolId]>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct PredicateRelationships {
    pub(crate) generics: GenericRelationships,
    pub(crate) parameters: Box<[PredicateParameterSymbolId]>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct StructRelationships {
    pub(crate) generics: GenericRelationships,
    pub(crate) fields: Box<[StructFieldSymbolId]>,
    pub(crate) constructors: Box<[ConstructorSymbolId]>,
    pub(crate) finalizers: Box<[FinalizerSymbolId]>,
    pub(crate) destructors: Box<[DestructorSymbolId]>,
    pub(crate) scope_enters: Box<[ScopeEnterSymbolId]>,
    pub(crate) scope_exits: Box<[ScopeExitSymbolId]>,
    pub(crate) callables: Box<[TypeCallableMemberSymbolId]>,
    pub(crate) constants: Box<[ConstantSymbolId]>,
    pub(crate) predicates: Box<[PredicateSymbolId]>,
    pub(crate) type_members: Box<[InherentTypeMemberSymbolId]>,
    pub(crate) overloads: Box<[CallableOverloadSymbolId]>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct UnionRelationships {
    pub(crate) generics: GenericRelationships,
    pub(crate) variants: Box<[UnionVariantSymbolId]>,
    pub(crate) constructors: Box<[ConstructorSymbolId]>,
    pub(crate) finalizers: Box<[FinalizerSymbolId]>,
    pub(crate) destructors: Box<[DestructorSymbolId]>,
    pub(crate) scope_enters: Box<[ScopeEnterSymbolId]>,
    pub(crate) scope_exits: Box<[ScopeExitSymbolId]>,
    pub(crate) callables: Box<[TypeCallableMemberSymbolId]>,
    pub(crate) constants: Box<[ConstantSymbolId]>,
    pub(crate) predicates: Box<[PredicateSymbolId]>,
    pub(crate) type_members: Box<[InherentTypeMemberSymbolId]>,
    pub(crate) overloads: Box<[CallableOverloadSymbolId]>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct TraitRelationships {
    pub(crate) generics: GenericRelationships,
    pub(crate) callables: Box<[TraitCallableMemberSymbolId]>,
    pub(crate) constants: Box<[TraitConstantMemberSymbolId]>,
    pub(crate) types: Box<[TraitTypeMemberSymbolId]>,
    pub(crate) predicates: Box<[TraitPredicateMemberSymbolId]>,
    pub(crate) finalizers: Box<[TraitFinalizerRequirementSymbolId]>,
    pub(crate) destructors: Box<[TraitDestructorRequirementSymbolId]>,
    pub(crate) scope_enters: Box<[TraitScopeEnterRequirementSymbolId]>,
    pub(crate) scope_exits: Box<[TraitScopeExitRequirementSymbolId]>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ImplementationRelationships {
    pub(crate) generics: GenericRelationships,
    pub(crate) callables: Box<[TypeCallableMemberSymbolId]>,
    pub(crate) trait_callables: Box<[TraitCallableFulfillmentSymbolId]>,
    pub(crate) constants: Box<[ConstantSymbolId]>,
    pub(crate) trait_constants: Box<[TraitConstantFulfillmentSymbolId]>,
    pub(crate) predicates: Box<[PredicateSymbolId]>,
    pub(crate) trait_predicates: Box<[TraitPredicateFulfillmentSymbolId]>,
    pub(crate) types: Box<[InherentTypeMemberSymbolId]>,
    pub(crate) trait_types: Box<[TraitTypeFulfillmentSymbolId]>,
    pub(crate) constructors: Box<[ConstructorSymbolId]>,
    pub(crate) finalizers: Box<[FinalizerSymbolId]>,
    pub(crate) destructors: Box<[DestructorSymbolId]>,
    pub(crate) scope_enters: Box<[ScopeEnterSymbolId]>,
    pub(crate) scope_exits: Box<[ScopeExitSymbolId]>,
    pub(crate) trait_scope_enters: Box<[TraitScopeEnterFulfillmentSymbolId]>,
    pub(crate) trait_scope_exits: Box<[TraitScopeExitFulfillmentSymbolId]>,
    pub(crate) overloads: Box<[CallableOverloadSymbolId]>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VariantRelationships {
    pub(crate) owner: UnionSymbolId,
    pub(crate) payload_fields: Box<[UnionPayloadFieldSymbolId]>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CallableParameterRelationships {
    pub(crate) owner: CallableSymbolId,
    pub(crate) ordinal: u32,
    pub(crate) default_presence: RuntimeDefaultPresence,
    pub(crate) default_provider: Option<crate::CallableParameterDefaultProviderSymbolId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PredicateParameterRelationships {
    pub(crate) owner: PredicateDefinitionSymbolId,
    pub(crate) ordinal: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GenericParameterRelationships {
    pub(crate) owner: GenericOwnerId,
    pub(crate) ordinal: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StructFieldRelationships {
    pub(crate) owner: StructSymbolId,
    pub(crate) ordinal: u32,
    pub(crate) allows_mutation: bool,
    pub(crate) default_presence: RuntimeDefaultPresence,
    pub(crate) default_provider: Option<crate::StructFieldDefaultProviderSymbolId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UnionPayloadFieldRelationships {
    pub(crate) owner: UnionVariantSymbolId,
    pub(crate) ordinal: u32,
    pub(crate) position: crate::CallablePosition,
    pub(crate) allows_mutation: bool,
    pub(crate) default_presence: RuntimeDefaultPresence,
    pub(crate) default_provider: Option<UnionPayloadDefaultProviderSymbolId>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct OverloadRelationships {
    pub(crate) arm_syntax: Box<[SyntaxAnchor]>,
    pub(crate) arms: Box<[AnySymbolId]>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RelationshipIndex {
    children: BTreeMap<AnySymbolId, Vec<AnySymbolId>>,
    runtime_defaults: BTreeMap<AnySymbolId, SyntaxAnchor>,
    overload_arms: BTreeMap<AnySymbolId, Box<[SyntaxAnchor]>>,
    imported_overload_arms: BTreeMap<AnySymbolId, Vec<AnySymbolId>>,
    providers: BTreeMap<AnySymbolId, AnySymbolId>,
    mutable_members: BTreeSet<AnySymbolId>,
    positional_members: BTreeSet<AnySymbolId>,
}

pub(crate) trait BuildRelationships<I>: Sized {
    fn build(id: I, index: &RelationshipIndex) -> Option<Self>;
}

impl RelationshipIndex {
    pub(crate) fn add_symbol(&mut self, id: AnySymbolId, owner: AnySymbolId) {
        self.children.entry(owner).or_default().push(id);
    }

    pub(crate) fn add_runtime_default(&mut self, owner: AnySymbolId, syntax: SyntaxAnchor) {
        self.runtime_defaults.insert(owner, syntax);
    }

    pub(crate) fn add_overload_arms(&mut self, owner: AnySymbolId, arms: Box<[SyntaxAnchor]>) {
        self.overload_arms.insert(owner, arms);
    }

    pub(crate) fn add_provider(&mut self, owner: AnySymbolId, provider: AnySymbolId) {
        self.providers.insert(owner, provider);
    }

    pub(crate) fn allow_mutation(&mut self, member: AnySymbolId) {
        self.mutable_members.insert(member);
    }

    pub(crate) fn allow_positional(&mut self, member: AnySymbolId) {
        self.positional_members.insert(member);
    }

    pub(crate) fn add_imported_overload_arm(&mut self, owner: AnySymbolId, arm: AnySymbolId) {
        self.imported_overload_arms
            .entry(owner)
            .or_default()
            .push(arm);
    }

    pub(crate) fn children(&self, owner: AnySymbolId) -> &[AnySymbolId] {
        self.children.get(&owner).map_or(&[], Vec::as_slice)
    }

    pub(crate) fn into_completion_children(self) -> BTreeMap<AnySymbolId, Box<[AnySymbolId]>> {
        let mut children = self.children;

        for (owner, provider) in self.providers {
            children.entry(owner).or_default().push(provider);
        }

        children
            .into_iter()
            .map(|(owner, children)| (owner, children.into_boxed_slice()))
            .collect()
    }

    fn runtime_default(&self, owner: AnySymbolId) -> RuntimeDefaultPresence {
        match self.runtime_defaults.get(&owner) {
            None => RuntimeDefaultPresence::Absent,
            Some(syntax) if syntax.is_recovered() => RuntimeDefaultPresence::Recovered,
            Some(_) => RuntimeDefaultPresence::Present,
        }
    }
}

macro_rules! collect_children {
    ($index:expr, $owner:expr, $variant:ident) => {
        $index
            .children($owner)
            .iter()
            .filter_map(|child| match child {
                AnySymbolId::$variant(id) => Some(*id),
                _ => None,
            })
            .collect::<Vec<_>>()
            .into_boxed_slice()
    };
}

impl GenericRelationships {
    pub(crate) fn new(owner: AnySymbolId, index: &RelationshipIndex) -> Self {
        Self {
            type_parameters: collect_children!(index, owner, GenericTypeParameter),
            const_parameters: collect_children!(index, owner, GenericConstParameter),
        }
    }
}

impl CallableRelationships {
    pub(crate) fn new(owner: AnySymbolId, index: &RelationshipIndex) -> Self {
        Self {
            generics: GenericRelationships::new(owner, index),
            receiver: collect_children!(index, owner, ReceiverParameter)
                .first()
                .copied(),
            parameters: collect_children!(index, owner, CallableParameter),
        }
    }
}

impl PredicateRelationships {
    pub(crate) fn new(owner: AnySymbolId, index: &RelationshipIndex) -> Self {
        Self {
            generics: GenericRelationships::new(owner, index),
            parameters: collect_children!(index, owner, PredicateParameter),
        }
    }
}

impl StructRelationships {
    pub(crate) fn new(owner: AnySymbolId, index: &RelationshipIndex) -> Self {
        Self {
            generics: GenericRelationships::new(owner, index),
            fields: collect_children!(index, owner, StructField),
            constructors: collect_children!(index, owner, Constructor),
            finalizers: collect_children!(index, owner, Finalizer),
            destructors: collect_children!(index, owner, Destructor),
            scope_enters: collect_children!(index, owner, ScopeEnter),
            scope_exits: collect_children!(index, owner, ScopeExit),
            callables: collect_children!(index, owner, TypeCallableMember),
            constants: collect_children!(index, owner, Constant),
            predicates: collect_children!(index, owner, Predicate),
            type_members: collect_children!(index, owner, InherentTypeMember),
            overloads: collect_children!(index, owner, CallableOverload),
        }
    }
}

impl UnionRelationships {
    pub(crate) fn new(owner: AnySymbolId, index: &RelationshipIndex) -> Self {
        Self {
            generics: GenericRelationships::new(owner, index),
            variants: collect_children!(index, owner, UnionVariant),
            constructors: collect_children!(index, owner, Constructor),
            finalizers: collect_children!(index, owner, Finalizer),
            destructors: collect_children!(index, owner, Destructor),
            scope_enters: collect_children!(index, owner, ScopeEnter),
            scope_exits: collect_children!(index, owner, ScopeExit),
            callables: collect_children!(index, owner, TypeCallableMember),
            constants: collect_children!(index, owner, Constant),
            predicates: collect_children!(index, owner, Predicate),
            type_members: collect_children!(index, owner, InherentTypeMember),
            overloads: collect_children!(index, owner, CallableOverload),
        }
    }
}

impl TraitRelationships {
    pub(crate) fn new(owner: AnySymbolId, index: &RelationshipIndex) -> Self {
        Self {
            generics: GenericRelationships::new(owner, index),
            callables: collect_children!(index, owner, TraitCallableMember),
            constants: collect_children!(index, owner, TraitConstantMember),
            types: collect_children!(index, owner, TraitTypeMember),
            predicates: collect_children!(index, owner, TraitPredicateMember),
            finalizers: collect_children!(index, owner, TraitFinalizerRequirement),
            destructors: collect_children!(index, owner, TraitDestructorRequirement),
            scope_enters: collect_children!(index, owner, TraitScopeEnterRequirement),
            scope_exits: collect_children!(index, owner, TraitScopeExitRequirement),
        }
    }
}

impl ImplementationRelationships {
    pub(crate) fn new(owner: AnySymbolId, index: &RelationshipIndex) -> Self {
        Self {
            generics: GenericRelationships::new(owner, index),
            callables: collect_children!(index, owner, TypeCallableMember),
            trait_callables: collect_children!(index, owner, TraitCallableFulfillment),
            constants: collect_children!(index, owner, Constant),
            trait_constants: collect_children!(index, owner, TraitConstantFulfillment),
            predicates: collect_children!(index, owner, Predicate),
            trait_predicates: collect_children!(index, owner, TraitPredicateFulfillment),
            types: collect_children!(index, owner, InherentTypeMember),
            trait_types: collect_children!(index, owner, TraitTypeFulfillment),
            constructors: collect_children!(index, owner, Constructor),
            finalizers: collect_children!(index, owner, Finalizer),
            destructors: collect_children!(index, owner, Destructor),
            scope_enters: collect_children!(index, owner, ScopeEnter),
            scope_exits: collect_children!(index, owner, ScopeExit),
            trait_scope_enters: collect_children!(index, owner, TraitScopeEnterFulfillment),
            trait_scope_exits: collect_children!(index, owner, TraitScopeExitFulfillment),
            overloads: collect_children!(index, owner, CallableOverload),
        }
    }
}

impl VariantRelationships {
    pub(crate) fn new(owner: AnySymbolId, index: &RelationshipIndex) -> Option<Self> {
        let (containing_symbol, _) = owner_and_ordinal(owner, index)?;

        let AnySymbolId::Union(containing_union) = containing_symbol else {
            return None;
        };

        Some(Self {
            owner: containing_union,
            payload_fields: collect_children!(index, owner, UnionPayloadField),
        })
    }
}

impl CallableParameterRelationships {
    pub(crate) fn new(id: CallableParameterSymbolId, index: &RelationshipIndex) -> Option<Self> {
        let erased = AnySymbolId::from(id);

        let (owner, _) = owner_and_ordinal(erased, index)?;

        Some(Self {
            owner: callable_owner(owner)?,
            ordinal: ordinal_within_kind(erased, index),
            default_presence: index.runtime_default(erased),
            default_provider: match index.providers.get(&erased) {
                Some(AnySymbolId::CallableParameterDefaultProvider(provider)) => Some(*provider),
                _ => None,
            },
        })
    }
}

impl PredicateParameterRelationships {
    pub(crate) fn new(id: PredicateParameterSymbolId, index: &RelationshipIndex) -> Option<Self> {
        let erased = id.into();

        let (owner, _) = owner_and_ordinal(erased, index)?;

        Some(Self {
            owner: predicate_owner(owner)?,
            ordinal: ordinal_within_kind(erased, index),
        })
    }
}

impl GenericParameterRelationships {
    fn new(id: AnySymbolId, index: &RelationshipIndex) -> Option<Self> {
        let (owner, _) = owner_and_ordinal(id, index)?;

        Some(Self {
            owner: GenericOwnerId::try_new(owner)?,
            ordinal: ordinal_within_generic_parameters(id, index),
        })
    }
}

impl StructFieldRelationships {
    pub(crate) fn new(id: StructFieldSymbolId, index: &RelationshipIndex) -> Option<Self> {
        let erased = id.into();

        let (owner, _) = owner_and_ordinal(erased, index)?;

        let AnySymbolId::Struct(owner) = owner else {
            return None;
        };

        Some(Self {
            owner,
            ordinal: ordinal_within_kind(erased, index),
            allows_mutation: index.mutable_members.contains(&erased),
            default_presence: index.runtime_default(erased),
            default_provider: match index.providers.get(&erased) {
                Some(AnySymbolId::StructFieldDefaultProvider(provider)) => Some(*provider),
                _ => None,
            },
        })
    }
}

impl UnionPayloadFieldRelationships {
    pub(crate) fn new(id: UnionPayloadFieldSymbolId, index: &RelationshipIndex) -> Option<Self> {
        let erased = id.into();

        let (owner, _) = owner_and_ordinal(erased, index)?;

        let AnySymbolId::UnionVariant(owner) = owner else {
            return None;
        };

        Some(Self {
            owner,
            ordinal: ordinal_within_kind(erased, index),
            position: if index.positional_members.contains(&erased) {
                crate::CallablePosition::PositionalOrNamed
            } else {
                crate::CallablePosition::NamedOnly
            },
            allows_mutation: index.mutable_members.contains(&erased),
            default_presence: index.runtime_default(erased),
            default_provider: match index.providers.get(&erased) {
                Some(AnySymbolId::UnionPayloadDefaultProvider(provider)) => Some(*provider),
                _ => None,
            },
        })
    }
}

impl OverloadRelationships {
    pub(crate) fn new(owner: AnySymbolId, index: &RelationshipIndex) -> Self {
        Self {
            // The record owns its immutable source-order view independently of builder storage.
            arm_syntax: index.overload_arms.get(&owner).cloned().unwrap_or_default(),
            arms: index
                .imported_overload_arms
                .get(&owner)
                .cloned()
                .unwrap_or_default()
                .into_boxed_slice(),
        }
    }
}

fn owner_and_ordinal(child: AnySymbolId, index: &RelationshipIndex) -> Option<(AnySymbolId, u32)> {
    index.children.iter().find_map(|(owner, children)| {
        children
            .iter()
            .position(|candidate| *candidate == child)
            .and_then(|ordinal| u32::try_from(ordinal).ok())
            .map(|ordinal| (*owner, ordinal))
    })
}

fn ordinal_within_kind(child: AnySymbolId, index: &RelationshipIndex) -> u32 {
    ordinal_among(child, index, |candidate| candidate.kind() == child.kind())
}

fn ordinal_within_generic_parameters(child: AnySymbolId, index: &RelationshipIndex) -> u32 {
    ordinal_among(child, index, |candidate| {
        matches!(
            candidate,
            AnySymbolId::GenericTypeParameter(_) | AnySymbolId::GenericConstParameter(_)
        )
    })
}

fn ordinal_among(
    child: AnySymbolId,
    index: &RelationshipIndex,
    mut belongs_to_group: impl FnMut(AnySymbolId) -> bool,
) -> u32 {
    let Some((owner, _)) = owner_and_ordinal(child, index) else {
        return 0;
    };

    index
        .children(owner)
        .iter()
        .copied()
        .filter(|candidate| belongs_to_group(*candidate))
        .position(|candidate| candidate == child)
        .and_then(|ordinal| u32::try_from(ordinal).ok())
        .unwrap_or(0)
}

pub(crate) fn callable_owner(owner: AnySymbolId) -> Option<CallableSymbolId> {
    match owner {
        AnySymbolId::Function(id) => Some(id.into()),
        AnySymbolId::TypeCallableMember(id) => Some(id.into()),
        AnySymbolId::TraitCallableMember(id) => Some(id.into()),
        AnySymbolId::TraitCallableFulfillment(id) => Some(id.into()),
        AnySymbolId::Constructor(id) => Some(id.into()),
        AnySymbolId::Finalizer(id) => Some(id.into()),
        AnySymbolId::Destructor(id) => Some(id.into()),
        AnySymbolId::ScopeEnter(id) => Some(id.into()),
        AnySymbolId::ScopeExit(id) => Some(id.into()),
        AnySymbolId::TraitFinalizerRequirement(id) => Some(id.into()),
        AnySymbolId::TraitDestructorRequirement(id) => Some(id.into()),
        AnySymbolId::TraitScopeEnterRequirement(id) => Some(id.into()),
        AnySymbolId::TraitScopeExitRequirement(id) => Some(id.into()),
        AnySymbolId::TraitScopeEnterFulfillment(id) => Some(id.into()),
        AnySymbolId::TraitScopeExitFulfillment(id) => Some(id.into()),
        _ => None,
    }
}

fn predicate_owner(owner: AnySymbolId) -> Option<PredicateDefinitionSymbolId> {
    match owner {
        AnySymbolId::Predicate(id) => Some(id.into()),
        AnySymbolId::TraitPredicateMember(id) => Some(id.into()),
        AnySymbolId::TraitPredicateFulfillment(id) => Some(id.into()),
        _ => None,
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ModuleRelationships {
    pub(crate) trusted_capabilities: Box<[TrustedCapabilitySymbolId]>,
    pub(crate) constants: Box<[ConstantSymbolId]>,
    pub(crate) functions: Box<[FunctionSymbolId]>,
    pub(crate) predicates: Box<[PredicateSymbolId]>,
    pub(crate) callable_contracts: Box<[CallableContractSymbolId]>,
    pub(crate) callable_overloads: Box<[CallableOverloadSymbolId]>,
    pub(crate) implementation_overloads: Box<[ImplementationOverloadSymbolId]>,
    pub(crate) structures: Box<[StructSymbolId]>,
    pub(crate) unions: Box<[UnionSymbolId]>,
    pub(crate) traits: Box<[TraitSymbolId]>,
    pub(crate) inherent_implementations: Box<[InherentImplementationSymbolId]>,
    pub(crate) unnamed_trait_implementations: Box<[UnnamedTraitImplementationSymbolId]>,
    pub(crate) named_trait_implementations: Box<[NamedTraitImplementationSymbolId]>,
}

impl ModuleRelationships {
    pub(crate) fn new(owner: AnySymbolId, index: &RelationshipIndex) -> Self {
        Self {
            trusted_capabilities: collect_children!(index, owner, TrustedCapability),
            constants: collect_children!(index, owner, Constant),
            functions: collect_children!(index, owner, Function),
            predicates: collect_children!(index, owner, Predicate),
            callable_contracts: collect_children!(index, owner, CallableContract),
            callable_overloads: collect_children!(index, owner, CallableOverload),
            implementation_overloads: collect_children!(index, owner, ImplementationOverload),
            structures: collect_children!(index, owner, Struct),
            unions: collect_children!(index, owner, Union),
            traits: collect_children!(index, owner, Trait),
            inherent_implementations: collect_children!(index, owner, InherentImplementation),
            unnamed_trait_implementations: collect_children!(
                index,
                owner,
                UnnamedTraitImplementation
            ),
            named_trait_implementations: collect_children!(index, owner, NamedTraitImplementation),
        }
    }
}

macro_rules! build_from_erased {
    ($relationships:ty: $($id:ty),+ $(,)?) => {
        $(
            impl BuildRelationships<$id> for $relationships {
                fn build(id: $id, index: &RelationshipIndex) -> Option<Self> {
                    Some(Self::new(id.into(), index))
                }
            }
        )+
    };
}

macro_rules! build_leaf {
    ($($id:ty),+ $(,)?) => {
        $(
            impl BuildRelationships<$id> for LeafRelationships {
                fn build(_id: $id, _index: &RelationshipIndex) -> Option<Self> {
                    Some(Self)
                }
            }
        )+
    };
}

build_from_erased!(CallableRelationships:
    FunctionSymbolId,
    TypeCallableMemberSymbolId,
    TraitCallableMemberSymbolId,
    TraitCallableFulfillmentSymbolId,
    ConstructorSymbolId,
    FinalizerSymbolId,
    DestructorSymbolId,
    ScopeEnterSymbolId,
    ScopeExitSymbolId,
    TraitFinalizerRequirementSymbolId,
    TraitDestructorRequirementSymbolId,
    TraitScopeEnterRequirementSymbolId,
    TraitScopeExitRequirementSymbolId,
    TraitScopeEnterFulfillmentSymbolId,
    TraitScopeExitFulfillmentSymbolId,
);
build_from_erased!(PredicateRelationships:
    PredicateSymbolId,
    TraitPredicateMemberSymbolId,
    TraitPredicateFulfillmentSymbolId,
);
build_from_erased!(GenericRelationships: CallableContractSymbolId);
build_from_erased!(OverloadRelationships:
    CallableOverloadSymbolId,
    ImplementationOverloadSymbolId,
);
build_from_erased!(StructRelationships: StructSymbolId);
build_from_erased!(UnionRelationships: UnionSymbolId);
build_from_erased!(TraitRelationships: TraitSymbolId);
build_from_erased!(ImplementationRelationships:
    InherentImplementationSymbolId,
    UnnamedTraitImplementationSymbolId,
    NamedTraitImplementationSymbolId,
);

build_leaf!(
    TrustedCapabilitySymbolId,
    ConstantSymbolId,
    InherentTypeMemberSymbolId,
    TraitConstantMemberSymbolId,
    TraitTypeMemberSymbolId,
    TraitConstantFulfillmentSymbolId,
    TraitTypeFulfillmentSymbolId,
);

impl BuildRelationships<GenericTypeParameterSymbolId> for GenericParameterRelationships {
    fn build(id: GenericTypeParameterSymbolId, index: &RelationshipIndex) -> Option<Self> {
        Self::new(id.into(), index)
    }
}

impl BuildRelationships<GenericConstParameterSymbolId> for GenericParameterRelationships {
    fn build(id: GenericConstParameterSymbolId, index: &RelationshipIndex) -> Option<Self> {
        Self::new(id.into(), index)
    }
}

impl BuildRelationships<CallableParameterSymbolId> for CallableParameterRelationships {
    fn build(id: CallableParameterSymbolId, index: &RelationshipIndex) -> Option<Self> {
        Self::new(id, index)
    }
}

impl BuildRelationships<PredicateParameterSymbolId> for PredicateParameterRelationships {
    fn build(id: PredicateParameterSymbolId, index: &RelationshipIndex) -> Option<Self> {
        Self::new(id, index)
    }
}

impl BuildRelationships<UnionVariantSymbolId> for VariantRelationships {
    fn build(id: UnionVariantSymbolId, index: &RelationshipIndex) -> Option<Self> {
        Self::new(id.into(), index)
    }
}

impl BuildRelationships<StructFieldSymbolId> for StructFieldRelationships {
    fn build(id: StructFieldSymbolId, index: &RelationshipIndex) -> Option<Self> {
        Self::new(id, index)
    }
}

impl BuildRelationships<UnionPayloadFieldSymbolId> for UnionPayloadFieldRelationships {
    fn build(id: UnionPayloadFieldSymbolId, index: &RelationshipIndex) -> Option<Self> {
        Self::new(id, index)
    }
}
