#[macro_use]
mod macros;
mod callable_type;
mod catalog;
mod default;
mod dependency;
mod lookup;
mod predicate;
mod shared;

pub(in crate::compilation) use callable_type::{
    bind_callable_clause_unit, callable_clause_header, callable_requirement_source,
    checked_callable_type_contracts,
};
pub(super) use catalog::checked_catalog_predicates;
pub(in crate::compilation) use dependency::extend_dependency_contract_with_statics;
pub(in crate::compilation) use dependency::static_dependency_root;
pub(super) use lookup::runtime_default_provider;
pub(super) use shared::{CheckedSourcePredicateSequence, checked_source_predicate_sequence};

pub(in crate::compilation) use shared::checked_source_expression;
