use std::num::NonZeroU16;

use crate::{FormatterDecision, FormatterParameter, FormatterRule};

/// Immutable resolved formatter policy.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FormatterConfiguration {
    enabled_rules: [bool; FormatterRule::ALL.len()],
    maximum_line_width: NonZeroU16,
}

impl FormatterConfiguration {
    /// Returns whether one formatter rule is enabled.
    pub const fn is_enabled(&self, rule: FormatterRule) -> bool {
        self.enabled_rules[rule.index()]
    }

    /// Returns whether a rule and its prerequisites enable one owned decision.
    ///
    /// Dependencies constrain only the recorded decision. Disabling wrapping,
    /// for example, leaves a list rule's delimiter spacing enabled.
    pub(crate) fn applies_to(&self, rule: FormatterRule, decision: FormatterDecision) -> bool {
        let descriptor = rule.descriptor();

        assert!(
            descriptor.decisions.contains(&decision),
            "formatter rule {} does not own {decision:?}",
            descriptor.name,
        );

        self.is_enabled(rule)
            && descriptor.dependencies.iter().all(|dependency| {
                dependency.decision != decision || self.is_enabled(dependency.rule)
            })
    }

    /// Returns the selected maximum display width.
    pub const fn maximum_line_width(&self) -> u16 {
        self.maximum_line_width.get()
    }

    /// Returns a configuration with one rule explicitly enabled or disabled.
    pub fn with_rule(mut self, rule: FormatterRule, enabled: bool) -> Self {
        self.enabled_rules[rule.index()] = enabled;

        self
    }

    /// Returns a configuration with the selected positive maximum display width.
    pub fn with_maximum_line_width(mut self, maximum_line_width: NonZeroU16) -> Self {
        self.maximum_line_width = maximum_line_width;

        self
    }
}

impl Default for FormatterConfiguration {
    fn default() -> Self {
        let mut enabled_rules = [false; FormatterRule::ALL.len()];

        for rule in FormatterRule::ALL {
            enabled_rules[rule.index()] = rule.descriptor().enabled_by_default;
        }

        Self {
            enabled_rules,
            maximum_line_width: FormatterParameter::MaximumLineWidth.schema().default,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU16;

    use super::{FormatterConfiguration, FormatterDecision, FormatterParameter, FormatterRule};

    #[test]
    fn dependency_gates_only_its_owned_decision() {
        let configuration =
            FormatterConfiguration::default().with_rule(FormatterRule::LineWrapping, false);

        assert!(configuration.applies_to(
            FormatterRule::ParenthesizedListLayout,
            FormatterDecision::TokenSpacing,
        ));

        assert!(!configuration.applies_to(
            FormatterRule::ParenthesizedListLayout,
            FormatterDecision::ParenthesizedListBreaks,
        ));

        assert!(
            !FormatterConfiguration::default()
                .with_rule(FormatterRule::BlockBraces, false)
                .applies_to(
                    FormatterRule::MatchArmBodyLayout,
                    FormatterDecision::BracePlacement
                )
        );
    }

    #[test]
    fn parameter_schema_validates_the_full_width_boundary() {
        let schema = FormatterParameter::MaximumLineWidth.schema();

        assert_eq!(
            schema.default.get(),
            FormatterConfiguration::default().maximum_line_width()
        );

        assert_eq!(schema.validate(1).map(NonZeroU16::get), Some(1));
        assert_eq!(schema.validate(65535).map(NonZeroU16::get), Some(65535));
        assert_eq!(schema.validate(0), None);
        assert_eq!(schema.validate(65536), None);
        assert_eq!(schema.validate(u64::MAX), None);
    }

    #[test]
    fn defaults_and_immutable_overrides_are_explicit() {
        let defaults = FormatterConfiguration::default();

        assert_eq!(defaults.maximum_line_width(), 120);

        assert!(defaults.is_enabled(FormatterRule::Indentation));
        assert!(!defaults.is_enabled(FormatterRule::SimplifyNestedIf));

        let configured = defaults
            .with_rule(FormatterRule::Indentation, false)
            .with_rule(FormatterRule::SimplifyNestedIf, true)
            .with_maximum_line_width(NonZeroU16::new(96).unwrap_or_else(|| unreachable!()));

        assert!(defaults.is_enabled(FormatterRule::Indentation));
        assert!(!defaults.is_enabled(FormatterRule::SimplifyNestedIf));
        assert!(!configured.is_enabled(FormatterRule::Indentation));
        assert!(configured.is_enabled(FormatterRule::SimplifyNestedIf));

        assert_eq!(configured.maximum_line_width(), 96);
    }
}
