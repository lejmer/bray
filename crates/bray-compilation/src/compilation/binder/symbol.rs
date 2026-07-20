mod cache;
mod completion;
mod compute;
mod contract;
mod environment;
mod surface;
mod template;

#[cfg(test)]
mod test_support;

pub(in crate::compilation) use cache::CompilationSymbolFacts;
