use std::collections::BTreeMap;

use crate::{GenericArgument, GenericParameterSymbolId, GenericSubstitutionData, TypeId};

pub(in crate::value) enum Substitution<'a> {
    Generic(&'a GenericSubstitutionData),
    Types(&'a BTreeMap<TypeId, TypeId>),
}

impl Substitution<'_> {
    pub(super) fn argument_for(
        &self,
        parameter: GenericParameterSymbolId,
    ) -> Option<GenericArgument> {
        match self {
            Self::Generic(generic) => generic.argument_for(parameter),
            Self::Types(_) => None,
        }
    }

    pub(super) fn type_replacement(&self, ty: TypeId) -> Option<TypeId> {
        match self {
            Self::Generic(_) => None,
            Self::Types(types) => types.get(&ty).copied(),
        }
    }

    pub(super) fn generic(&self) -> Option<&GenericSubstitutionData> {
        match self {
            Self::Generic(generic) => Some(generic),
            Self::Types(_) => None,
        }
    }
}
