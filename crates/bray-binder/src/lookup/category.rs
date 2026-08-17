use bray_symbols::{
    AnyLocalSymbolId, AnySymbolId, CallableContractSymbolId, CallableOverloadSymbolId,
    CallableParameterSymbolId, ConstantSymbolId, ConstructorSymbolId, FunctionSymbolId,
    GenericConstParameterSymbolId, GenericTypeParameterSymbolId, InherentTypeMemberSymbolId,
    NamedTypeSymbolId, PredicateParameterSymbolId, PredicateSymbolId, ReceiverParameterSymbolId,
    StaticSymbolId,
    StructFieldSymbolId, TraitCallableFulfillmentSymbolId, TraitCallableMemberSymbolId,
    TraitConstantFulfillmentSymbolId, TraitConstantMemberSymbolId,
    TraitPredicateFulfillmentSymbolId, TraitPredicateMemberSymbolId, TraitTypeFulfillmentSymbolId,
    TraitTypeMemberSymbolId, TypeCallableMemberSymbolId, UnionPayloadFieldSymbolId,
    UnionVariantSymbolId,
};

/// One candidate in the ordinary namespace visible to a binder request.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum ResolvedName {
    Local(AnyLocalSymbolId),
    Surface(AnySymbolId),
}

/// A name that denotes a type-valued semantic entity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum ResolvedTypeName {
    Named(NamedTypeSymbolId),
    CallableContract(CallableContractSymbolId),
    GenericParameter(GenericTypeParameterSymbolId),
    InherentMember(InherentTypeMemberSymbolId),
    TraitMember(TraitTypeMemberSymbolId),
    TraitFulfillment(TraitTypeFulfillmentSymbolId),
}

/// A name that denotes a value usable by expression binding.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum ResolvedValueName {
    Local(AnyLocalSymbolId),
    GenericConstParameter(GenericConstParameterSymbolId),
    CallableParameter(CallableParameterSymbolId),
    PredicateParameter(PredicateParameterSymbolId),
    ReceiverParameter(ReceiverParameterSymbolId),
    Constant(ConstantSymbolId),
    Static(StaticSymbolId),
    Function(FunctionSymbolId),
    Predicate(PredicateSymbolId),
    CallableOverload(CallableOverloadSymbolId),
    StructField(StructFieldSymbolId),
    UnionVariant(UnionVariantSymbolId),
    UnionPayloadField(UnionPayloadFieldSymbolId),
    TypeCallableMember(TypeCallableMemberSymbolId),
    Constructor(ConstructorSymbolId),
    TraitCallableMember(TraitCallableMemberSymbolId),
    TraitConstantMember(TraitConstantMemberSymbolId),
    TraitPredicateMember(TraitPredicateMemberSymbolId),
    TraitCallableFulfillment(TraitCallableFulfillmentSymbolId),
    TraitConstantFulfillment(TraitConstantFulfillmentSymbolId),
    TraitPredicateFulfillment(TraitPredicateFulfillmentSymbolId),
}

impl ResolvedValueName {
    pub(crate) fn into_name(self) -> ResolvedName {
        match self {
            Self::Local(id) => ResolvedName::Local(id),
            Self::GenericConstParameter(id) => ResolvedName::Surface(id.into()),
            Self::CallableParameter(id) => ResolvedName::Surface(id.into()),
            Self::PredicateParameter(id) => ResolvedName::Surface(id.into()),
            Self::ReceiverParameter(id) => ResolvedName::Surface(id.into()),
            Self::Constant(id) => ResolvedName::Surface(id.into()),
            Self::Static(id) => ResolvedName::Surface(id.into()),
            Self::Function(id) => ResolvedName::Surface(id.into()),
            Self::Predicate(id) => ResolvedName::Surface(id.into()),
            Self::CallableOverload(id) => ResolvedName::Surface(id.into()),
            Self::StructField(id) => ResolvedName::Surface(id.into()),
            Self::UnionVariant(id) => ResolvedName::Surface(id.into()),
            Self::UnionPayloadField(id) => ResolvedName::Surface(id.into()),
            Self::TypeCallableMember(id) => ResolvedName::Surface(id.into()),
            Self::Constructor(id) => ResolvedName::Surface(id.into()),
            Self::TraitCallableMember(id) => ResolvedName::Surface(id.into()),
            Self::TraitConstantMember(id) => ResolvedName::Surface(id.into()),
            Self::TraitPredicateMember(id) => ResolvedName::Surface(id.into()),
            Self::TraitCallableFulfillment(id) => ResolvedName::Surface(id.into()),
            Self::TraitConstantFulfillment(id) => ResolvedName::Surface(id.into()),
            Self::TraitPredicateFulfillment(id) => ResolvedName::Surface(id.into()),
        }
    }
}

/// A validated ordinary member reached through a qualified owner.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct ResolvedMemberName(AnySymbolId);

impl ResolvedMemberName {
    pub(crate) const fn symbol(self) -> AnySymbolId {
        self.0
    }
}

pub(crate) fn classify_type(name: ResolvedName) -> Option<ResolvedTypeName> {
    let ResolvedName::Surface(symbol) = name else {
        return None;
    };

    match symbol {
        AnySymbolId::Struct(id) => Some(ResolvedTypeName::Named(id.into())),
        AnySymbolId::Union(id) => Some(ResolvedTypeName::Named(id.into())),
        AnySymbolId::CallableContract(id) => Some(ResolvedTypeName::CallableContract(id)),
        AnySymbolId::GenericTypeParameter(id) => Some(ResolvedTypeName::GenericParameter(id)),
        AnySymbolId::InherentTypeMember(id) => Some(ResolvedTypeName::InherentMember(id)),
        AnySymbolId::TraitTypeMember(id) => Some(ResolvedTypeName::TraitMember(id)),
        AnySymbolId::TraitTypeFulfillment(id) => Some(ResolvedTypeName::TraitFulfillment(id)),
        _ => None,
    }
}

#[cfg(test)]
pub(super) fn classify_trait(name: ResolvedName) -> Option<bray_symbols::TraitSymbolId> {
    match name {
        ResolvedName::Surface(AnySymbolId::Trait(id)) => Some(id),
        ResolvedName::Local(_) | ResolvedName::Surface(_) => None,
    }
}

pub(super) fn classify_value(name: ResolvedName) -> Option<ResolvedValueName> {
    match name {
        ResolvedName::Local(id) => Some(ResolvedValueName::Local(id)),
        ResolvedName::Surface(AnySymbolId::GenericConstParameter(id)) => {
            Some(ResolvedValueName::GenericConstParameter(id))
        }
        ResolvedName::Surface(AnySymbolId::CallableParameter(id)) => {
            Some(ResolvedValueName::CallableParameter(id))
        }
        ResolvedName::Surface(AnySymbolId::PredicateParameter(id)) => {
            Some(ResolvedValueName::PredicateParameter(id))
        }
        ResolvedName::Surface(AnySymbolId::ReceiverParameter(id)) => {
            Some(ResolvedValueName::ReceiverParameter(id))
        }
        ResolvedName::Surface(AnySymbolId::Constant(id)) => Some(ResolvedValueName::Constant(id)),
        ResolvedName::Surface(AnySymbolId::Static(id)) => Some(ResolvedValueName::Static(id)),
        ResolvedName::Surface(AnySymbolId::Function(id)) => Some(ResolvedValueName::Function(id)),
        ResolvedName::Surface(AnySymbolId::Predicate(id)) => Some(ResolvedValueName::Predicate(id)),
        ResolvedName::Surface(AnySymbolId::CallableOverload(id)) => {
            Some(ResolvedValueName::CallableOverload(id))
        }
        ResolvedName::Surface(AnySymbolId::StructField(id)) => {
            Some(ResolvedValueName::StructField(id))
        }
        ResolvedName::Surface(AnySymbolId::UnionVariant(id)) => {
            Some(ResolvedValueName::UnionVariant(id))
        }
        ResolvedName::Surface(AnySymbolId::UnionPayloadField(id)) => {
            Some(ResolvedValueName::UnionPayloadField(id))
        }
        ResolvedName::Surface(AnySymbolId::TypeCallableMember(id)) => {
            Some(ResolvedValueName::TypeCallableMember(id))
        }
        ResolvedName::Surface(AnySymbolId::Constructor(id)) => {
            Some(ResolvedValueName::Constructor(id))
        }
        ResolvedName::Surface(AnySymbolId::TraitCallableMember(id)) => {
            Some(ResolvedValueName::TraitCallableMember(id))
        }
        ResolvedName::Surface(AnySymbolId::TraitConstantMember(id)) => {
            Some(ResolvedValueName::TraitConstantMember(id))
        }
        ResolvedName::Surface(AnySymbolId::TraitPredicateMember(id)) => {
            Some(ResolvedValueName::TraitPredicateMember(id))
        }
        ResolvedName::Surface(AnySymbolId::TraitCallableFulfillment(id)) => {
            Some(ResolvedValueName::TraitCallableFulfillment(id))
        }
        ResolvedName::Surface(AnySymbolId::TraitConstantFulfillment(id)) => {
            Some(ResolvedValueName::TraitConstantFulfillment(id))
        }
        ResolvedName::Surface(AnySymbolId::TraitPredicateFulfillment(id)) => {
            Some(ResolvedValueName::TraitPredicateFulfillment(id))
        }
        ResolvedName::Surface(_) => None,
    }
}

#[cfg(test)]
pub(super) fn classify_callable_overload(name: ResolvedName) -> Option<CallableOverloadSymbolId> {
    match name {
        ResolvedName::Surface(AnySymbolId::CallableOverload(id)) => Some(id),
        ResolvedName::Local(_) | ResolvedName::Surface(_) => None,
    }
}

pub(super) fn classify_member(name: ResolvedName) -> Option<ResolvedMemberName> {
    match name {
        ResolvedName::Surface(symbol) => Some(ResolvedMemberName(symbol)),
        ResolvedName::Local(_) => None,
    }
}
