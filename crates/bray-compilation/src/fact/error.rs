use bray_binder::SemanticUnitContextError;
use bray_checker::CheckerInfrastructureError;

use super::CompilationFactKey;

/// One detected cycle in the compilation fact dependency graph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactCycle {
    facts: Box<[CompilationFactKey]>,
}

impl FactCycle {
    pub(crate) fn new(facts: impl Into<Box<[CompilationFactKey]>>) -> Self {
        Self {
            facts: canonical_cycle(facts.into()),
        }
    }

    pub(crate) fn facts(&self) -> &[CompilationFactKey] {
        &self.facts
    }
}

fn canonical_cycle(facts: Box<[CompilationFactKey]>) -> Box<[CompilationFactKey]> {
    let mut facts = facts.into_vec();

    let Some(closing) = facts.last().cloned() else {
        return facts.into_boxed_slice();
    };

    let Some(start) = facts[..facts.len().saturating_sub(1)]
        .iter()
        .position(|fact| fact == &closing)
    else {
        return facts.into_boxed_slice();
    };

    facts.drain(..start);

    let cycle_len = facts.len().saturating_sub(1);

    let Some((canonical_start, _)) = facts[..cycle_len]
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| left.cmp(right))
    else {
        return facts.into_boxed_slice();
    };

    let mut canonical = facts[..cycle_len]
        .iter()
        .cycle()
        .skip(canonical_start)
        .take(cycle_len)
        .cloned()
        .collect::<Vec<_>>();

    if let Some(first) = canonical.first().cloned() {
        canonical.push(first);
    }

    canonical.into_boxed_slice()
}

#[cfg(test)]
mod tests {
    use bray_source::SourceId;

    use super::{CompilationFactKey, FactCycle};

    #[test]
    fn cycle_paths_remove_prefixes_and_use_a_canonical_start() {
        let syntax = CompilationFactKey::SyntaxTree;
        let declaration = CompilationFactKey::DeclarationTable;
        let prefix = CompilationFactKey::SourceUnitSyntax(SourceId::new(0));

        let cycle = FactCycle::new([prefix, syntax.clone(), declaration.clone(), syntax.clone()]);

        assert_eq!(cycle.facts(), &[declaration.clone(), syntax, declaration]);
    }
}

/// An outer compiler-query outcome that must not be represented as a source diagnostic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FactQueryError {
    /// The requesting operation was cancelled before completion.
    Cancelled,
    /// Evaluation encountered a same-worker or cross-worker dependency cycle.
    Cycle(FactCycle),
    /// The fact request could not complete because compiler coordination failed.
    InfrastructureFailure,
    /// Semantic-context construction found an inconsistent bound unit.
    SemanticUnitContext(SemanticUnitContextError),
    /// Semantic checking could not complete because a typed dependency was unavailable.
    CheckerInfrastructure(CheckerInfrastructureError),
}

impl std::fmt::Display for FactQueryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("fact evaluation was cancelled"),
            Self::Cycle(cycle) => write!(
                formatter,
                "fact evaluation encountered a dependency cycle: {:?}",
                cycle.facts()
            ),
            Self::InfrastructureFailure => {
                formatter.write_str("fact evaluation encountered an infrastructure failure")
            }
            Self::SemanticUnitContext(error) => {
                write!(formatter, "semantic unit context failed: {error:?}")
            }
            Self::CheckerInfrastructure(error) => {
                write!(
                    formatter,
                    "semantic checking infrastructure failed: {error:?}"
                )
            }
        }
    }
}

impl std::error::Error for FactQueryError {}
