mod bundle;
mod surface;

#[cfg(test)]
pub(in crate::compilation::export::build) use surface::build_identity_surface;
pub(in crate::compilation) use surface::external_symbol_key;
