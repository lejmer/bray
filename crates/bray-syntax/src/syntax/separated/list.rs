macro_rules! define_separated_list_syntax {
    (
        $(#[$list_meta:meta])*
        $visibility:vis struct $list_syntax:ident {
            builder: $builder_syntax:ident,
            item: $item_syntax:ident,
            kind: $list_kind:path,
            item_kind: $item_kind:path,
            separator_kind: $separator_kind:path,
            items: $items_method:ident,
            separators: $separator_tokens_method:ident,
            source_slot: $source_slot:literal,
            range_description: $range_description:literal,
            debug_name: $debug_name:literal,
            builder_debug_name: $builder_debug_name:literal $(,)?
        }
    ) => {
        $(#[$list_meta])*
        #[derive(Clone, Eq, Hash, PartialEq)]
        $visibility struct $list_syntax {
            list: $crate::syntax::separated::SeparatedSyntaxList,
        }

        impl $list_syntax {
            /// Creates a builder for this separated-list node at `start`.
            ///
            /// `start` is the source offset where the list begins, including leading
            /// trivia for its first child when present.
            pub fn builder(
                source: bray_source::SourceSnapshot,
                start: bray_source::TextSize,
            ) -> $builder_syntax {
                $builder_syntax::new(source, start)
            }

            pub(crate) fn from_green(
                source: bray_source::SourceSnapshot,
                node: $crate::green::GreenNode,
                start: bray_source::TextSize,
            ) -> Self {
                Self {
                    list: $crate::syntax::separated::SeparatedSyntaxList::from_green(
                        source,
                        node,
                        start,
                        $list_kind,
                    ),
                }
            }

            pub(crate) fn into_green(self) -> $crate::green::GreenNode {
                self.list.into_green()
            }

            fn from_builder(builder: $builder_syntax) -> Self {
                Self {
                    list: $crate::syntax::separated::SeparatedSyntaxList::from_green(
                        builder.source.into_value(),
                        builder.list.build(),
                        builder.start,
                        $list_kind,
                    ),
                }
            }

            /// Returns the full source text range covered by this list.
            pub fn full_range(&self) -> bray_source::TextRange {
                $crate::SyntaxNode::full_range(self)
            }

            /// Returns whether this list contains parser recovery.
            pub fn is_recovered(&self) -> bool {
                self.list.is_recovered()
            }

            /// Returns direct item nodes in source order.
            pub fn $items_method(&self) -> impl Iterator<Item = $item_syntax> + '_ {
                self.list.child_nodes($item_kind).map(|(node, start)| {
                    // SourceSnapshot clones share immutable source text with typed child nodes.
                    $item_syntax::from_green(self.list.source().clone(), node, start)
                })
            }

            /// Returns separator tokens in source order.
            pub fn $separator_tokens_method(
                &self,
            ) -> impl Iterator<Item = $crate::SyntaxToken> + '_ {
                self.list.separator_tokens($separator_kind)
            }

            /// Returns descendant syntax tokens in source order.
            pub fn tokens(&self) -> impl Iterator<Item = $crate::SyntaxToken> + '_ {
                self.list.tokens()
            }

            /// Returns skipped-syntax recovery nodes in source order.
            pub fn skipped_syntax(&self) -> impl Iterator<Item = $crate::SkippedSyntax> + '_ {
                self.list.skipped_syntax()
            }
        }

        impl std::fmt::Debug for $list_syntax {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter
                    .debug_struct($debug_name)
                    .field("source", &$crate::SourceSyntaxNode::source(self))
                    .field("kind", &$crate::SyntaxNode::kind(self))
                    .field("full_range", &self.full_range())
                    .field("is_recovered", &self.is_recovered())
                    .finish()
            }
        }

        impl $crate::syntax::node::GreenSyntaxNode for $list_syntax {
            fn green_node(&self) -> &$crate::green::GreenNode {
                self.list.green_node()
            }

            fn start(&self) -> bray_source::TextSize {
                self.list.start()
            }

            fn range_description(&self) -> &'static str {
                $range_description
            }

            fn is_recovered(&self) -> bool {
                self.list.is_recovered()
            }
        }

        impl $crate::syntax::node::GreenSourceSyntaxNode for $list_syntax {
            fn source(&self) -> &bray_source::SourceSnapshot {
                self.list.source()
            }
        }

        /// Builder for this separated-list syntax node.
        $visibility struct $builder_syntax {
            source: $crate::builder::RequiredSyntaxSlot<bray_source::SourceSnapshot>,
            start: bray_source::TextSize,
            list: $crate::syntax::separated::SeparatedSyntaxListBuilder,
        }

        impl $builder_syntax {
            /// Creates an empty separated-list builder starting at `start`.
            pub fn new(source: bray_source::SourceSnapshot, start: bray_source::TextSize) -> Self {
                let mut source_slot = $crate::builder::RequiredSyntaxSlot::new($source_slot);

                source_slot.set(source);

                Self {
                    source: source_slot,
                    start,
                    list: $crate::syntax::separated::SeparatedSyntaxListBuilder::new(
                        $list_kind,
                        $separator_kind,
                    ),
                }
            }

            /// Appends an item node in source order.
            pub fn push_item(&mut self, item: $item_syntax) {
                self.list.push_item_node(item.into_green());
            }

            /// Appends a separator token in source order.
            pub fn push_separator_token(&mut self, token: $crate::SyntaxToken) {
                self.list.push_separator_token(token);
            }

            /// Appends present source tokens under a skipped-syntax recovery node.
            ///
            /// Empty token lists do not add a recovery node.
            ///
            /// Panics when any skipped token is missing.
            pub fn push_skipped_tokens(
                &mut self,
                tokens: impl IntoIterator<Item = $crate::SyntaxToken>,
            ) {
                self.list.push_skipped_tokens(tokens);
            }

            /// Appends an item node in source order.
            pub fn item(mut self, item: $item_syntax) -> Self {
                self.push_item(item);

                self
            }

            /// Appends a separator token in source order.
            pub fn separator_token(mut self, token: $crate::SyntaxToken) -> Self {
                self.push_separator_token(token);

                self
            }

            /// Appends present source tokens under a skipped-syntax recovery node.
            ///
            /// Empty token lists do not add a recovery node.
            ///
            /// Panics when any skipped token is missing.
            pub fn skipped_tokens(
                mut self,
                tokens: impl IntoIterator<Item = $crate::SyntaxToken>,
            ) -> Self {
                self.push_skipped_tokens(tokens);

                self
            }

            /// Builds the separated-list node.
            pub fn build(self) -> $list_syntax {
                $list_syntax::from_builder(self)
            }
        }

        impl std::fmt::Debug for $builder_syntax {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter
                    .debug_struct($builder_debug_name)
                    .field("start", &self.start)
                    .finish_non_exhaustive()
            }
        }
    };
}

pub(in crate::syntax) use define_separated_list_syntax;
