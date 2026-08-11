use crate::{
    Diagnostic, DiagnosticArg, DiagnosticArgName, DiagnosticLabelStyle, DiagnosticNoteKind,
    DiagnosticRelatedLocationKind, DiagnosticSuggestionKind,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DiagnosticContextKind {
    Invocation,
    Source,
    ExternalInput,
    Artifact,
}

/// Goal-state structural and rendered requirements for one diagnostic category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiagnosticQualityContract {
    context: DiagnosticContextKind,
    required_args: &'static [DiagnosticArgName],
    components: &'static [DiagnosticComponentContract],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DiagnosticComponentContract {
    PrimaryMessage {
        args: &'static [DiagnosticArgName],
    },
    Note {
        kind: DiagnosticNoteKind,
        args: &'static [DiagnosticArgName],
    },
    RelatedLocation {
        kind: DiagnosticRelatedLocationKind,
        args: &'static [DiagnosticArgName],
    },
    Suggestion {
        kind: DiagnosticSuggestionKind,
        applicability: crate::DiagnosticSuggestionApplicability,
        args: &'static [DiagnosticArgName],
    },
}

impl DiagnosticQualityContract {
    #[cfg(test)]
    pub(super) fn requires_note(self) -> bool {
        self.components
            .iter()
            .any(|component| matches!(component, DiagnosticComponentContract::Note { .. }))
    }

    #[cfg(test)]
    pub(super) fn requires_suggestion(self) -> bool {
        self.components
            .iter()
            .any(|component| matches!(component, DiagnosticComponentContract::Suggestion { .. }))
    }

    pub(super) const fn invocation(
        args: &'static [DiagnosticArgName],
        components: &'static [DiagnosticComponentContract],
    ) -> Self {
        Self::new(DiagnosticContextKind::Invocation, args, components)
    }

    pub(super) const fn external(
        args: &'static [DiagnosticArgName],
        components: &'static [DiagnosticComponentContract],
    ) -> Self {
        Self::new(DiagnosticContextKind::ExternalInput, args, components)
    }

    pub(super) const fn artifact(
        args: &'static [DiagnosticArgName],
        components: &'static [DiagnosticComponentContract],
    ) -> Self {
        Self::new(DiagnosticContextKind::Artifact, args, components)
    }

    pub(super) const fn source(
        args: &'static [DiagnosticArgName],
        components: &'static [DiagnosticComponentContract],
    ) -> Self {
        Self::new(DiagnosticContextKind::Source, args, components)
    }

    const fn new(
        context: DiagnosticContextKind,
        required_args: &'static [DiagnosticArgName],
        components: &'static [DiagnosticComponentContract],
    ) -> Self {
        Self {
            context,
            required_args,
            components,
        }
    }

    /// Returns typed context that must appear in at least one rendered component.
    pub const fn required_rendered_args(self) -> &'static [DiagnosticArgName] {
        self.required_args
    }

    /// Returns typed context that the primary message itself must render.
    pub fn primary_message_args(self) -> &'static [DiagnosticArgName] {
        self.components
            .iter()
            .find_map(|action| match action {
                DiagnosticComponentContract::PrimaryMessage { args } => Some(*args),
                _ => None,
            })
            .unwrap_or(&[])
    }

    /// Returns typed context that a required note of `kind` must render.
    pub fn note_args(self, kind: DiagnosticNoteKind) -> Option<&'static [DiagnosticArgName]> {
        self.components
            .iter()
            .find_map(|component| match component {
                DiagnosticComponentContract::Note {
                    kind: action_kind,
                    args,
                } if *action_kind == kind => Some(*args),
                _ => None,
            })
    }

    /// Returns typed context that a required related location of `kind` must render.
    pub fn related_location_args(
        self,
        kind: DiagnosticRelatedLocationKind,
    ) -> Option<&'static [DiagnosticArgName]> {
        self.components
            .iter()
            .find_map(|component| match component {
                DiagnosticComponentContract::RelatedLocation {
                    kind: action_kind,
                    args,
                } if *action_kind == kind => Some(*args),
                _ => None,
            })
    }

    /// Returns typed context that a required suggestion of `kind` must render.
    pub fn suggestion_args(
        self,
        kind: DiagnosticSuggestionKind,
    ) -> Option<&'static [DiagnosticArgName]> {
        self.components
            .iter()
            .find_map(|component| match component {
                DiagnosticComponentContract::Suggestion {
                    kind: action_kind,
                    args,
                    ..
                } if *action_kind == kind => Some(*args),
                _ => None,
            })
    }

    /// Returns every unmet structural requirement in one produced diagnostic.
    pub fn unmet_requirements(self, diagnostic: &Diagnostic) -> Vec<DiagnosticQualityIssue> {
        let mut issues = Vec::new();

        if self.context == DiagnosticContextKind::Source && diagnostic.primary_span().is_none() {
            issues.push(DiagnosticQualityIssue::MissingPrimarySpan);
        }

        if !self
            .required_args
            .iter()
            .all(|name| diagnostic.args().iter().any(|arg| arg.name() == *name))
        {
            issues.push(DiagnosticQualityIssue::MissingTypedContext);
        }

        if has_duplicate_arg_names(diagnostic.args())
            || diagnostic
                .notes()
                .iter()
                .any(|note| has_duplicate_arg_names(note.args()))
            || diagnostic
                .related_locations()
                .iter()
                .any(|location| has_duplicate_arg_names(location.args()))
            || diagnostic
                .suggestions()
                .iter()
                .any(|suggestion| has_duplicate_arg_names(suggestion.args()))
        {
            issues.push(DiagnosticQualityIssue::DuplicateArgument);
        }

        if diagnostic
            .labels()
            .iter()
            .filter(|label| label.style() == DiagnosticLabelStyle::Primary)
            .count()
            > 1
        {
            issues.push(DiagnosticQualityIssue::DuplicatePrimaryLabel);
        }

        if has_duplicates(diagnostic.labels())
            || has_duplicates(diagnostic.notes())
            || has_duplicates(diagnostic.related_locations())
            || has_duplicates(diagnostic.suggestions())
        {
            issues.push(DiagnosticQualityIssue::DuplicateAction);
        }

        if diagnostic.primary_span().is_some()
            && !diagnostic.labels().iter().any(|label| {
                label.style() == DiagnosticLabelStyle::Primary
                    && Some(label.span()) == diagnostic.primary_span()
            })
        {
            issues.push(DiagnosticQualityIssue::MissingPrimaryLabel);
        }

        for component in self.components {
            validate_component(*component, diagnostic, &mut issues);
        }

        issues.sort_unstable();
        issues.dedup();

        issues
    }
}

fn has_duplicate_arg_names(args: &[DiagnosticArg]) -> bool {
    args.iter().enumerate().any(|(index, arg)| {
        args[index + 1..]
            .iter()
            .any(|other| other.name() == arg.name())
    })
}

fn has_duplicates<T: Eq>(values: &[T]) -> bool {
    values
        .iter()
        .enumerate()
        .any(|(index, value)| values[index + 1..].contains(value))
}

fn validate_component(
    component: DiagnosticComponentContract,
    diagnostic: &Diagnostic,
    issues: &mut Vec<DiagnosticQualityIssue>,
) {
    match component {
        DiagnosticComponentContract::PrimaryMessage { args } => {
            if !args
                .iter()
                .all(|name| diagnostic.args().iter().any(|arg| arg.name() == *name))
            {
                issues.push(DiagnosticQualityIssue::MissingTypedContext);
            }
        }
        DiagnosticComponentContract::Note { kind, args } => {
            let note = diagnostic.notes().iter().find(|note| note.kind() == kind);

            if note.is_none() {
                issues.push(DiagnosticQualityIssue::MissingComponent);
            } else if note.is_some_and(|note| {
                !args
                    .iter()
                    .all(|name| note.args().iter().any(|arg| arg.name() == *name))
            }) {
                issues.push(DiagnosticQualityIssue::MissingTypedContext);
            }
        }
        DiagnosticComponentContract::RelatedLocation { kind, args } => {
            let related = diagnostic
                .related_locations()
                .iter()
                .find(|location| location.kind() == kind);

            if related.is_none() {
                issues.push(DiagnosticQualityIssue::MissingComponent);
            } else if related
                .is_some_and(|location| diagnostic.primary_span() == Some(location.span()))
            {
                issues.push(DiagnosticQualityIssue::InvalidRelatedLocation);
            } else if related.is_some_and(|location| {
                !args
                    .iter()
                    .all(|name| location.args().iter().any(|arg| arg.name() == *name))
            }) {
                issues.push(DiagnosticQualityIssue::MissingTypedContext);
            }
        }
        DiagnosticComponentContract::Suggestion {
            kind,
            applicability,
            args,
        } => {
            let suggestion = diagnostic
                .suggestions()
                .iter()
                .find(|suggestion| suggestion.kind() == kind);

            if suggestion.is_none() {
                issues.push(DiagnosticQualityIssue::MissingComponent);
            } else if suggestion.is_some_and(|suggestion| {
                suggestion.applicability() != applicability
                    || match applicability {
                        crate::DiagnosticSuggestionApplicability::Manual => {
                            !suggestion.edits().is_empty()
                        }
                        crate::DiagnosticSuggestionApplicability::MachineApplicable
                        | crate::DiagnosticSuggestionApplicability::MaybeApplicable => {
                            suggestion.edits().is_empty()
                        }
                    }
                    || !args
                        .iter()
                        .all(|name| suggestion.args().iter().any(|arg| arg.name() == *name))
            }) {
                issues.push(DiagnosticQualityIssue::MissingTypedContext);
            }
        }
    }
}

/// An unmet goal-state diagnostic quality requirement.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticQualityIssue {
    /// A source diagnostic has no primary source span.
    MissingPrimarySpan,
    /// A diagnostic lacks one required named typed context set.
    MissingTypedContext,
    /// A source diagnostic lacks a primary label on its primary span.
    MissingPrimaryLabel,
    /// The diagnostic lacks its exact required note, relation, or suggestion.
    MissingComponent,
    /// A related location repeats the primary location instead of providing context.
    InvalidRelatedLocation,
    /// One rendered component carries more than one value for the same argument name.
    DuplicateArgument,
    /// More than one primary label competes to explain the diagnostic's primary span.
    DuplicatePrimaryLabel,
    /// The diagnostic repeats an identical label, note, related location, or suggestion.
    DuplicateAction,
}
