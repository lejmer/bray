use bray_symbols::{DirectiveKind, DirectiveSurface, DirectiveTemplate};

pub(super) fn first_directive(
    directives: &DirectiveSurface,
    kind: DirectiveKind,
) -> Option<&DirectiveTemplate> {
    directives
        .directives()
        .iter()
        .find(|directive| directive.kind() == kind)
}

pub(super) fn directives_of_kind(
    directives: &DirectiveSurface,
    kind: DirectiveKind,
) -> Vec<DirectiveTemplate> {
    directives
        .directives()
        .iter()
        .filter(|directive| directive.kind() == kind)
        .cloned()
        .collect()
}
