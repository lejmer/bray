mod candidate;
mod evaluation;
mod query;

#[cfg(test)]
mod tests;

pub(in crate::compilation) use evaluation::constraint_expression;
