use std::collections::HashSet;

use super::super::{
    ConstantTermData, ConstantValueKind, GenericArgument, GenericSubstitutionId,
    SemanticValueStoreId, TraitApplicationId, TypeData, TypeId,
};
use super::table::SemanticTables;

pub(super) fn substitution_is_concrete(
    tables: &SemanticTables,
    store: SemanticValueStoreId,
    root: GenericSubstitutionId,
) -> bool {
    let mut pending = vec![ConcreteWork::Substitution(root)];

    let mut types = HashSet::new();
    let mut substitutions = HashSet::new();
    let mut trait_applications = HashSet::new();

    while let Some(work) = pending.pop() {
        match work {
            ConcreteWork::Type(id) if types.insert(id) => {
                let data = tables.types.get(store, id);

                match data {
                    TypeData::Error
                    | TypeData::TypeParameter(_)
                    | TypeData::ContextualSelf(_)
                    | TypeData::TypeValuedMemberProjection { .. } => {
                        return false;
                    }
                    TypeData::Named { substitution, .. } => {
                        pending.push(ConcreteWork::Substitution(*substitution));
                    }
                    TypeData::Tuple(elements) => {
                        pending.extend(elements.iter().copied().map(ConcreteWork::Type));
                    }
                    TypeData::Array { element, length } => {
                        pending.push(ConcreteWork::Type(*element));

                        if !append_closed_term_type(tables, store, *length, &mut pending) {
                            return false;
                        }
                    }
                    TypeData::FlexibleArray(target)
                    | TypeData::Slice(target)
                    | TypeData::Generator(target)
                    | TypeData::Nullable(target)
                    | TypeData::Borrow { target, .. } => {
                        pending.push(ConcreteWork::Type(*target));
                    }
                    TypeData::TraitView(application) => {
                        pending.push(ConcreteWork::TraitApplication(*application));
                    }
                    TypeData::OwnedIndirection { storage, target } => {
                        pending.push(ConcreteWork::Type(*storage));
                        pending.push(ConcreteWork::Type(*target));
                    }
                    TypeData::Callable(callable) => {
                        pending.extend(
                            callable
                                .parameters()
                                .iter()
                                .map(|parameter| ConcreteWork::Type(parameter.ty())),
                        );

                        pending.push(ConcreteWork::Type(callable.result()));
                    }
                }
            }
            ConcreteWork::Substitution(id) if substitutions.insert(id) => {
                let substitution = tables.substitutions.get(store, id);

                for binding in substitution.bindings() {
                    match binding.argument() {
                        GenericArgument::Type(ty) => pending.push(ConcreteWork::Type(ty)),
                        GenericArgument::Constant(term) => {
                            if !append_closed_term_type(tables, store, term, &mut pending) {
                                return false;
                            }
                        }
                    }
                }
            }
            ConcreteWork::TraitApplication(id) if trait_applications.insert(id) => {
                let application = tables.trait_applications.get(store, id);

                pending.push(ConcreteWork::Substitution(application.substitution()));
            }
            ConcreteWork::Type(_)
            | ConcreteWork::Substitution(_)
            | ConcreteWork::TraitApplication(_) => {}
        }
    }

    true
}

fn append_closed_term_type(
    tables: &SemanticTables,
    store: SemanticValueStoreId,
    term: super::super::ConstantTermId,
    pending: &mut Vec<ConcreteWork>,
) -> bool {
    let ConstantTermData::Value(value) = tables.constant_terms.get(store, term) else {
        return false;
    };

    let value = tables.constant_values.get(store, *value);

    if matches!(value.kind(), ConstantValueKind::Error) {
        return false;
    }

    pending.push(ConcreteWork::Type(value.ty()));

    true
}

enum ConcreteWork {
    Type(TypeId),
    Substitution(GenericSubstitutionId),
    TraitApplication(TraitApplicationId),
}
