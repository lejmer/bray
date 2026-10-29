#[cfg(test)]
mod operations;
mod resident;

pub(crate) use resident::drain_product_thread_statics;

#[cfg(test)]
pub(super) use operations::control;
