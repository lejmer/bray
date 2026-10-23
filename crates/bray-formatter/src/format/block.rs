use bray_syntax::{BlockItemSyntax, SyntaxKind, SyntaxNodeView};

use crate::{FormatterConfiguration, FormatterDecision, FormatterRule};

pub(super) struct BlockParagraphs {
    enabled: bool,
    paragraphs: Vec<BlockParagraph>,
    next_item: usize,
}

struct BlockParagraph {
    depth: usize,
    previous_item: Option<usize>,
    previous_category: Option<BlockItemCategory>,
    current_item: Option<usize>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum BlockItemCategory {
    Declaration,
    Expression,
}

impl BlockParagraphs {
    pub(super) const fn new(enabled: bool) -> Self {
        Self {
            enabled,
            paragraphs: Vec::new(),
            next_item: 0,
        }
    }

    pub(super) fn preserves_source_separation(
        &self,
        current: SyntaxKind,
        previous: Option<SyntaxKind>,
        nodes: &[SyntaxKind],
        line_breaks: u8,
        configuration: &FormatterConfiguration,
    ) -> bool {
        self.enabled
            && line_breaks >= 2
            && !matches!(current, SyntaxKind::OpenBraceToken | SyntaxKind::CloseBraceToken)
            && previous != Some(SyntaxKind::OpenBraceToken)
            && nodes.iter().copied().any(is_block_expression)
            && !(nodes.last() == Some(&SyntaxKind::MatchBody)
                && configuration.is_enabled(FormatterRule::MatchCaseSpacing)
                && FormatterRule::MatchCaseSpacing.preferred_for(
                    FormatterRule::BlockParagraphSpacing,
                    FormatterDecision::BlockParagraphSeparation,
                ) == FormatterRule::MatchCaseSpacing)
    }

    pub(super) fn enter_node(
        &mut self,
        node: SyntaxNodeView<'_>,
        depth: usize,
    ) -> Option<(usize, Option<usize>, bool)> {
        if !self.enabled {
            return None;
        }

        let kind = node.kind();

        if is_block_expression(kind) {
            self.paragraphs.push(BlockParagraph {
                depth,
                previous_item: None,
                previous_category: None,
                current_item: None,
            });

            return None;
        }

        let paragraph = self.paragraphs.last_mut()?;

        if kind != SyntaxKind::BlockItem || paragraph.depth + 1 != depth {
            return None;
        }

        let item = self.next_item;
        let category = block_item_category(node);

        let separate = paragraph
            .previous_category
            .is_some_and(|previous| previous != category);

        self.next_item = self.next_item.saturating_add(1);
        paragraph.current_item = Some(item);
        paragraph.previous_category = Some(category);

        Some((item, paragraph.previous_item, separate))
    }

    pub(super) fn leave_node(&mut self, kind: SyntaxKind, depth: usize) -> Option<usize> {
        if !self.enabled {
            return None;
        }

        if is_block_expression(kind) {
            let paragraph = self.paragraphs.pop();

            assert_eq!(
                paragraph.map(|paragraph| paragraph.depth + 1),
                Some(depth),
                "formatter block traversal must remain balanced"
            );

            return None;
        }

        let paragraph = self.paragraphs.last_mut()?;

        if kind != SyntaxKind::BlockItem || paragraph.depth + 2 != depth {
            return None;
        }

        let item = paragraph.current_item.take()?;

        paragraph.previous_item = Some(item);

        Some(item)
    }
}

fn block_item_category(node: SyntaxNodeView<'_>) -> BlockItemCategory {
    let item = node
        .cast::<BlockItemSyntax>()
        .unwrap_or_else(|| unreachable!("a block item view must cast to block item syntax"));

    if item.local_binding_declaration().is_some() || item.constant_declaration().is_some() {
        BlockItemCategory::Declaration
    } else {
        BlockItemCategory::Expression
    }
}

const fn is_block_expression(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::BlockExpression | SyntaxKind::CallableBodyBlockExpression
    )
}
