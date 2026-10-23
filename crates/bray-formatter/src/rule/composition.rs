use super::{FormatterDecision, FormatterRule, FormatterRuleDescriptor, FormatterStage};

impl FormatterRule {
    /// Returns the rules participating in a stage in deterministic precedence order.
    pub fn in_stage(stage: FormatterStage) -> Vec<Self> {
        let mut rules = Self::ALL
            .iter()
            .copied()
            .filter(|rule| {
                rule.descriptor()
                    .stages()
                    .any(|candidate| candidate == stage)
            })
            .collect::<Vec<_>>();

        rules.sort_unstable_by_key(|rule| rule.descriptor().order);

        rules
    }

    pub(crate) fn preferred_for(self, other: Self, decision: FormatterDecision) -> Self {
        preferred(self.descriptor(), other.descriptor(), decision)
    }
}

fn preferred(
    left: &FormatterRuleDescriptor,
    right: &FormatterRuleDescriptor,
    decision: FormatterDecision,
) -> FormatterRule {
    assert!(
        left.decisions.contains(&decision) && right.decisions.contains(&decision),
        "formatter decision {decision:?} is not owned by both {} and {}",
        left.name,
        right.name,
    );

    if left.rule == right.rule {
        return left.rule;
    }

    assert!(
        left.conflicts.contains(&right.rule) || right.conflicts.contains(&left.rule),
        "unrecorded formatter ownership conflict for {decision:?}: {} and {}",
        left.name,
        right.name,
    );

    if left.order <= right.order {
        left.rule
    } else {
        right.rule
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{FormatterDecision, FormatterRule, FormatterStage, preferred};
    use crate::FormatterConfiguration;

    #[test]
    fn catalog_is_exhaustive_and_records_every_shared_owner() {
        let mut names = BTreeSet::new();
        let mut orders = BTreeSet::new();
        let defaults = FormatterConfiguration::default();

        for (index, rule) in FormatterRule::ALL.iter().copied().enumerate() {
            let descriptor = rule.descriptor();

            assert_eq!(rule.index(), index);
            assert_eq!(descriptor.rule, rule);
            assert!(names.insert(descriptor.name));
            assert!(orders.insert(descriptor.order));
            assert!(!descriptor.decisions.is_empty());
            assert_eq!(FormatterRule::from_name(descriptor.name), Some(rule));
            assert_eq!(defaults.is_enabled(rule), descriptor.enabled_by_default);

            for dependency in descriptor.dependencies {
                assert!(descriptor.decisions.contains(&dependency.decision));
                assert_ne!(dependency.rule, rule);
            }

            for conflict in descriptor.conflicts {
                assert_ne!(*conflict, rule);

                assert!(
                    descriptor
                        .decisions
                        .iter()
                        .any(|decision| { conflict.descriptor().decisions.contains(decision) })
                );
            }

            for other in FormatterRule::ALL
                .iter()
                .copied()
                .filter(|other| *other != rule)
            {
                for decision in descriptor.decisions {
                    if other.descriptor().decisions.contains(decision) {
                        assert_eq!(
                            rule.preferred_for(other, *decision),
                            other.preferred_for(rule, *decision),
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn stages_and_conflicts_use_explicit_precedence() {
        for stage in FormatterStage::ALL {
            let rules = FormatterRule::in_stage(*stage);

            assert!(
                rules
                    .windows(2)
                    .all(|pair| { pair[0].descriptor().order < pair[1].descriptor().order })
            );
        }

        assert!(FormatterRule::in_stage(FormatterStage::RecoveryPreservation).is_empty());

        assert_eq!(
            FormatterRule::BlockBraces.preferred_for(
                FormatterRule::MatchArmBodyLayout,
                FormatterDecision::BracePlacement,
            ),
            FormatterRule::MatchArmBodyLayout,
        );
    }

    #[test]
    #[should_panic(expected = "unrecorded formatter ownership conflict")]
    fn unrecorded_ownership_is_an_invariant_failure() {
        let mut descriptor = *FormatterRule::GenericDelimiterSpacing.descriptor();

        descriptor.conflicts = &[];

        preferred(
            &descriptor,
            FormatterRule::WordSpacing.descriptor(),
            FormatterDecision::TokenSpacing,
        );
    }
}
