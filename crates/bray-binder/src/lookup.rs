mod binding;
mod category;
mod diagnostic;
mod path;

#[cfg(test)]
pub(crate) use binding::lookup_unqualified_name;
#[cfg(test)]
pub(crate) use path::NameAccess;
