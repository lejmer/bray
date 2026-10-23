use std::collections::HashSet;

use super::super::{
    ConstantTermData, ConstantTermId, ConstantValueId, ConstantValueKind, GenericArgument,
    GenericSubstitutionId, SemanticValueStoreId, TraitApplicationId, TypeData, TypeId,
};
use super::table::SemanticTables;

pub(super) fn substitution_is_concrete(
    tables: &SemanticTables,
    store: SemanticValueStoreId,
    root: GenericSubstitutionId,
) -> bool {
    is_concrete(tables, store, ConcreteWork::Substitution(root))
}

pub(super) fn constant_term_is_closed(
    tables: &SemanticTables,
    store: SemanticValueStoreId,
    root: ConstantTermId,
) -> bool {
    is_concrete(tables, store, ConcreteWork::Term(root))
}

fn is_concrete(tables: &SemanticTables, store: SemanticValueStoreId, root: ConcreteWork) -> bool {
    let closed_terms = matches!(root, ConcreteWork::Term(_));
    let mut pending = vec![root];

    let mut terms = HashSet::new();
    let mut types = HashSet::new();
    let mut substitutions = HashSet::new();
    let mut trait_applications = HashSet::new();

    while let Some(work) = pending.pop() {
        match work {
            ConcreteWork::Term(id) if terms.insert(id) => {
                match tables.constant_terms.get(store, id) {
                    ConstantTermData::Parameter(_)
                    | ConstantTermData::CallableArgument(_)
                    | ConstantTermData::TargetProperty(_) => return false,
                    ConstantTermData::Value(value) => {
                        if !append_closed_value_type(tables, store, *value, &mut pending) {
                            return false;
                        }
                    }
                    ConstantTermData::IntegerLiteral { .. } => {}
                    ConstantTermData::Typed { term, ty } => {
                        pending.push(ConcreteWork::Type(*ty));
                        pending.push(ConcreteWork::Term(*term));
                    }
                    ConstantTermData::Unary { operand, .. }
                    | ConstantTermData::NullablePresent(operand)
                    | ConstantTermData::VariantTest {
                        subject: operand, ..
                    } => {
                        pending.push(ConcreteWork::Term(*operand));
                    }
                    ConstantTermData::Conversion { operand, target } => {
                        pending.push(ConcreteWork::Type(*target));
                        pending.push(ConcreteWork::Term(*operand));
                    }
                    ConstantTermData::Binary { left, right, .. } => {
                        pending.push(ConcreteWork::Term(*left));
                        pending.push(ConcreteWork::Term(*right));
                    }
                    ConstantTermData::Tuple(elements) | ConstantTermData::Array(elements) => {
                        pending.extend(elements.iter().copied().map(ConcreteWork::Term));
                    }
                    ConstantTermData::Product(fields) => {
                        pending.extend(
                            fields
                                .iter()
                                .map(|field| ConcreteWork::Term(*field.value())),
                        );
                    }
                    ConstantTermData::Union { fields, .. } => {
                        pending.extend(
                            fields
                                .iter()
                                .map(|field| ConcreteWork::Term(*field.value())),
                        );
                    }
                    ConstantTermData::DefinitionApplication {
                        substitution,
                        selected_implementation,
                        ..
                    } => {
                        pending.push(ConcreteWork::Substitution(*substitution));

                        if let Some(implementation) = selected_implementation {
                            let implementation =
                                tables.implementation_instances.get(store, *implementation);

                            pending.push(ConcreteWork::Substitution(implementation.substitution()));
                        }
                    }
                    ConstantTermData::Call {
                        callable,
                        arguments,
                        selected_implementation,
                    } => {
                        let callable = tables.callable_instances.get(store, *callable);

                        pending.push(ConcreteWork::Substitution(callable.substitution()));
                        pending.extend(arguments.iter().copied().map(ConcreteWork::Term));

                        if let Some(implementation) = selected_implementation {
                            let implementation =
                                tables.implementation_instances.get(store, *implementation);

                            pending.push(ConcreteWork::Substitution(implementation.substitution()));
                        }
                    }
                    ConstantTermData::PredicateCall {
                        predicate,
                        arguments,
                    } => {
                        pending.push(ConcreteWork::Substitution(predicate.substitution()));
                        pending.extend(arguments.iter().copied().map(ConcreteWork::Term));
                    }
                    ConstantTermData::Projection(projection) => {
                        pending.push(ConcreteWork::Term(projection.subject()));
                        pending.extend(projection.kind().term_references().map(ConcreteWork::Term));
                    }
                }
            }
            ConcreteWork::Type(id) if types.insert(id) => {
                let data = tables.types.get(store, id);

                match data {
                    TypeData::Error
                    | TypeData::TypeParameter(_)
                    | TypeData::ContextualSelf(_)
                    | TypeData::TypeValuedMemberProjection { .. } => {
                        return false;
                    }
                    TypeData::ExactInteger { .. } => {}
                    TypeData::Named { substitution, .. } => {
                        pending.push(ConcreteWork::Substitution(*substitution));
                    }
                    TypeData::Tuple(elements) => {
                        pending.extend(elements.iter().copied().map(ConcreteWork::Type));
                    }
                    TypeData::Array { element, length } => {
                        pending.push(ConcreteWork::Type(*element));

                        if closed_terms {
                            pending.push(ConcreteWork::Term(*length));
                        } else if !append_closed_array_length_type(
                            tables,
                            store,
                            *length,
                            &mut pending,
                        ) {
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
                        GenericArgument::Constant(term) if closed_terms => {
                            pending.push(ConcreteWork::Term(term));
                        }
                        GenericArgument::Constant(term) => {
                            let ConstantTermData::Value(value) =
                                tables.constant_terms.get(store, term)
                            else {
                                return false;
                            };

                            if !append_closed_value_type(tables, store, *value, &mut pending) {
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
            ConcreteWork::Term(_)
            | ConcreteWork::Type(_)
            | ConcreteWork::Substitution(_)
            | ConcreteWork::TraitApplication(_) => {}
        }
    }

    true
}

fn append_closed_array_length_type(
    tables: &SemanticTables,
    store: SemanticValueStoreId,
    mut term: ConstantTermId,
    pending: &mut Vec<ConcreteWork>,
) -> bool {
    loop {
        match tables.constant_terms.get(store, term) {
            ConstantTermData::IntegerLiteral { .. } => return true,
            ConstantTermData::Typed { term: inner, ty } => {
                pending.push(ConcreteWork::Type(*ty));
                term = *inner;
            }
            ConstantTermData::Value(value) => {
                return append_closed_value_type(tables, store, *value, pending);
            }
            _ => return false,
        }
    }
}

fn append_closed_value_type(
    tables: &SemanticTables,
    store: SemanticValueStoreId,
    value: ConstantValueId,
    pending: &mut Vec<ConcreteWork>,
) -> bool {
    let value = tables.constant_values.get(store, value);

    if matches!(value.kind(), ConstantValueKind::Error) {
        return false;
    }

    pending.push(ConcreteWork::Type(value.ty()));

    true
}

enum ConcreteWork {
    Term(ConstantTermId),
    Type(TypeId),
    Substitution(GenericSubstitutionId),
    TraitApplication(TraitApplicationId),
}
