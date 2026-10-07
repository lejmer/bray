#[macro_use]
mod macros;
mod callable_type;
mod catalog;
mod default;
mod dependency;
mod lookup;
mod predicate;
mod shared;

pub(in crate::compilation) use callable_type::checked_callable_type_contracts;
pub(super) use catalog::checked_catalog_predicates;
pub(super) use dependency::extend_dependency_contract_with_statics;
pub(super) use lookup::runtime_default_provider;
pub(super) use shared::{
    CheckedSourcePredicateSequence, checked_source_expression, checked_source_predicate_sequence,
};
