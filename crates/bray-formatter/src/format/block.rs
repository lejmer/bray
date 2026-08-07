use bray_syntax::SyntaxKind;

pub(super) struct BlockParagraphs {
    enabled: bool,
    paragraphs: Vec<BlockParagraph>,
    next_item: usize,
}

struct BlockParagraph {
    depth: usize,
    previous_item: Option<usize>,
    current_item: Option<usize>,
}

impl BlockParagraphs {
    pub(super) const fn new(enabled: bool) -> Self {
        Self {
            enabled,
            paragraphs: Vec::new(),
            next_item: 0,
        }
    }

    pub(super) fn enter_node(
        &mut self,
        kind: SyntaxKind,
        depth: usize,
    ) -> Option<(usize, Option<usize>)> {
        if !self.enabled {
            return None;
        }

        if is_block_expression(kind) {
            self.paragraphs.push(BlockParagraph {
                depth,
                previous_item: None,
                current_item: None,
            });

            return None;
        }

        let paragraph = self.paragraphs.last_mut()?;

        if kind != SyntaxKind::BlockItem || paragraph.depth + 1 != depth {
            return None;
        }

        let item = self.next_item;

        self.next_item = self.next_item.saturating_add(1);
        paragraph.current_item = Some(item);

        Some((item, paragraph.previous_item))
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

const fn is_block_expression(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::BlockExpression | SyntaxKind::CallableBodyBlockExpression
    )
}
