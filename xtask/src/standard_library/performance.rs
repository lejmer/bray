mod command;
mod comparison;
mod corpus;
mod model;
mod observation;
mod report;
mod retention;
mod statistics;
mod validation;

#[cfg(test)]
mod tests;

pub(in crate::standard_library) use command::run;
