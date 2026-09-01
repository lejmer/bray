mod coherence;
mod conformance;
mod fulfillment;
mod index;
mod matching;
mod participation;
mod query;
mod selection;

pub(super) use fulfillment::{
    TypeValuedMemberResolution, callable_instance, implementation_callable_instance,
    implementation_fulfillments, implementation_instance_requirement, implementation_requirement,
    selected_type_valued_member,
};
pub(super) use index::ImplementationHeader;
pub(in crate::compilation) use index::ImplementationHeaderIndex;
pub(crate) use matching::ImplementationMatchError;
pub(super) use matching::{implementation_match_query_error, match_implementation_subject};
