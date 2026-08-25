mod core;
mod error;
mod options;
mod platform;
mod profile;
mod reuse;
mod source;
mod verification;

pub(super) use core::{
    build, compare_bundles, read_manifest, standard_library_product, standard_library_version,
    write_bundle_artifact,
};
pub(crate) use core::{build_target_bundle, run};
pub(super) use error::BuildError;
pub(crate) use reuse::current_target_bundle;
pub(super) use source::request as standard_library_source_request;
