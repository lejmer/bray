use crate::green::{GreenElement, GreenNode};
use crate::{SyntaxKind, SyntaxToken, SyntaxTokenPresence};

#[derive(Debug)]
pub(crate) struct RequiredSyntaxSlot<T> {
    name: &'static str,
    value: Option<T>,
}

impl<T> RequiredSyntaxSlot<T> {
    pub(crate) const fn new(name: &'static str) -> Self {
        Self { name, value: None }
    }

    pub(crate) fn set(&mut self, value: T) {
        if self.value.is_some() {
            panic!("required syntax slot was set more than once: {}", self.name);
        }

        self.value = Some(value);
    }

    pub(crate) fn into_value(self) -> T {
        match self.value {
            Some(value) => value,
            None => panic!("required syntax slot was not set: {}", self.name),
        }
    }
}

#[derive(Debug)]
pub(crate) struct SyntaxListSlot<T> {
    values: Vec<T>,
}

impl<T> SyntaxListSlot<T> {
    pub(crate) const fn new() -> Self {
        Self { values: Vec::new() }
    }

    pub(crate) fn push(&mut self, value: T) {
        self.values.push(value);
    }

    pub(crate) fn extend(&mut self, values: impl IntoIterator<Item = T>) {
        self.values.extend(values);
    }

    pub(crate) fn into_vec(self) -> Vec<T> {
        self.values
    }
}

impl<T> Default for SyntaxListSlot<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Default)]
pub(crate) struct GreenNodeBuilder {
    children: Vec<GreenElement>,
}

impl GreenNodeBuilder {
    pub(crate) const fn new() -> Self {
        Self {
            children: Vec::new(),
        }
    }

    pub(crate) fn push_element(&mut self, element: GreenElement) {
        self.children.push(element);
    }

    pub(crate) fn push_node(&mut self, node: GreenNode) {
        self.push_element(GreenElement::from(node));
    }

    pub(crate) fn push_elements(&mut self, elements: impl IntoIterator<Item = GreenElement>) {
        self.children.extend(elements);
    }

    pub(crate) fn push_token(&mut self, token: SyntaxToken) {
        self.push_element(GreenElement::from(token));
    }

    pub(crate) fn push_tokens(&mut self, tokens: impl IntoIterator<Item = SyntaxToken>) {
        self.push_elements(tokens.into_iter().map(GreenElement::from));
    }

    pub(crate) fn push_skipped_tokens(&mut self, tokens: impl IntoIterator<Item = SyntaxToken>) {
        if let Some(node) = skipped_syntax_node(tokens) {
            self.push_node(node);
        }
    }

    pub(crate) fn last_token(&self) -> Option<(SyntaxKind, SyntaxTokenPresence)> {
        match self.children.last() {
            Some(GreenElement::Token(token)) => Some((token.kind(), token.presence())),
            Some(GreenElement::Node(node)) => node.last_token(),
            None => None,
        }
    }

    pub(crate) fn build(self, kind: SyntaxKind) -> GreenNode {
        GreenNode::new(kind, self.children)
    }
}

fn skipped_syntax_node(tokens: impl IntoIterator<Item = SyntaxToken>) -> Option<GreenNode> {
    let mut builder = GreenNodeBuilder::new();
    let mut has_tokens = false;

    for token in tokens {
        assert!(
            token.is_present(),
            "skipped syntax can only contain present source tokens"
        );

        has_tokens = true;

        builder.push_token(token);
    }

    if has_tokens {
        Some(builder.build(SyntaxKind::SkippedSyntax))
    } else {
        None
    }
}

pub(crate) fn require_token_kind(
    actual: SyntaxKind,
    expected: SyntaxKind,
    slot_name: &'static str,
) {
    assert_eq!(
        actual,
        expected,
        "syntax slot `{}` expected token kind `{}`",
        slot_name,
        expected.as_str()
    );
}

#[cfg(test)]
mod tests {
    use bray_source::{TextRange, TextSize};

    use super::{GreenNodeBuilder, RequiredSyntaxSlot, require_token_kind};
    use crate::{SyntaxKind, SyntaxToken};

    #[test]
    fn required_slots_return_their_value() {
        let mut slot = RequiredSyntaxSlot::new("test.slot");

        slot.set(10);

        assert_eq!(slot.into_value(), 10);
    }

    #[test]
    #[should_panic]
    fn required_slots_reject_missing_values() {
        let slot = RequiredSyntaxSlot::<u8>::new("test.slot");

        let _ = slot.into_value();
    }

    #[test]
    #[should_panic]
    fn required_slots_reject_duplicate_values() {
        let mut slot = RequiredSyntaxSlot::new("test.slot");

        slot.set(1);
        slot.set(2);
    }

    #[test]
    fn green_node_builders_accept_token_elements() {
        let mut child_builder = GreenNodeBuilder::new();

        child_builder.push_token(SyntaxToken::new(
            SyntaxKind::FuncKeyword,
            TextRange::new(TextSize::ZERO, TextSize::new(4)),
        ));

        let child = child_builder.build(SyntaxKind::SourceUnit);

        let mut parent_builder = GreenNodeBuilder::new();

        parent_builder.push_element(child.into());
        parent_builder.push_tokens([SyntaxToken::end_of_file(TextSize::new(4))]);

        assert_eq!(
            parent_builder.last_token().map(|(kind, _presence)| kind),
            Some(SyntaxKind::EndOfFileToken)
        );
    }

    #[test]
    fn green_node_builders_accept_element_lists() {
        let mut first_builder = GreenNodeBuilder::new();
        let mut second_builder = GreenNodeBuilder::new();

        first_builder.push_token(SyntaxToken::new(
            SyntaxKind::FuncKeyword,
            TextRange::new(TextSize::ZERO, TextSize::new(4)),
        ));

        second_builder.push_token(SyntaxToken::end_of_file(TextSize::new(4)));

        let first = first_builder.build(SyntaxKind::SourceUnit);
        let second = second_builder.build(SyntaxKind::SourceUnit);

        let mut parent_builder = GreenNodeBuilder::new();

        parent_builder.push_elements([first.into(), second.into()]);

        assert_eq!(
            parent_builder.last_token().map(|(kind, _presence)| kind),
            Some(SyntaxKind::EndOfFileToken)
        );
    }

    #[test]
    fn green_node_builders_attach_skipped_syntax_nodes() {
        let mut builder = GreenNodeBuilder::new();

        builder.push_token(SyntaxToken::new(
            SyntaxKind::FuncKeyword,
            TextRange::new(TextSize::ZERO, TextSize::new(4)),
        ));

        builder.push_skipped_tokens([SyntaxToken::invalid(TextRange::new(
            TextSize::new(4),
            TextSize::new(5),
        ))]);

        builder.push_token(SyntaxToken::end_of_file(TextSize::new(5)));

        let node = builder.build(SyntaxKind::SourceUnit);

        let skipped_nodes = node
            .skipped_syntax_nodes(TextSize::ZERO)
            .collect::<Vec<_>>();

        assert_eq!(skipped_nodes.len(), 1);
        assert_eq!(skipped_nodes[0].0.kind(), SyntaxKind::SkippedSyntax);
        assert_eq!(skipped_nodes[0].1, TextSize::new(4));
    }

    #[test]
    fn required_token_slots_accept_missing_tokens_with_expected_kind() {
        let mut slot = RequiredSyntaxSlot::new("statement.semicolon_token");

        let missing = SyntaxToken::missing(SyntaxKind::SemicolonToken, TextSize::new(4));

        require_token_kind(
            missing.kind(),
            SyntaxKind::SemicolonToken,
            "statement.semicolon_token",
        );

        slot.set(missing.clone());

        let token = slot.into_value();

        assert_eq!(token, missing);
        assert!(token.is_missing());
    }
}
