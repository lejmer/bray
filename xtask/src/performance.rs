mod command;
mod comparison;
mod compilation;
mod compiler_timing;
mod corpus;
mod execution;
mod format;
mod html;
mod model;
mod observation;
mod peer;
mod presentation;
mod ranking;
mod report;
mod retention;
mod source;
mod statistics;
mod validation;

#[cfg(test)]
mod tests;

pub(crate) use command::run;
pub(in crate::performance) use command::validate_output;
