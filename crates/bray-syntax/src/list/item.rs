macro_rules! define_token_item_syntax {
    (
        $(#[$item_meta:meta])*
        $visibility:vis struct $item_syntax:ident {
            builder: $builder_syntax:ident,
            kind: $item_kind:path,
            token_kind: $token_kind:path,
            token: $token_method:ident,
            push_token: $push_token_method:ident,
            source_slot: $source_slot:literal,
            token_slot: $token_slot:literal,
            range_description: $range_description:literal,
            debug_name: $debug_name:literal,
            builder_debug_name: $builder_debug_name:literal,
            missing_token_panic: $missing_token_panic:literal $(,)?
        }
    ) => {
        $(#[$item_meta])*
        #[derive(Clone, Eq, Hash, PartialEq)]
        $visibility struct $item_syntax {
            source: bray_source::SourceSnapshot,
            node: $crate::green::GreenNode,
            start: bray_source::TextSize,
        }

        impl $item_syntax {
            /// Creates a builder for this item node.
            pub fn builder(source: bray_source::SourceSnapshot) -> $builder_syntax {
                $builder_syntax::new(source)
            }

            pub(crate) fn from_green(
                source: bray_source::SourceSnapshot,
                node: $crate::green::GreenNode,
                start: bray_source::TextSize,
            ) -> Self {
                assert_eq!(node.kind(), $item_kind);

                Self {
                    source,
                    node,
                    start,
                }
            }

            pub(crate) fn into_green(self) -> $crate::green::GreenNode {
                self.node
            }

            fn from_builder(builder: $builder_syntax) -> Self {
                let source = builder.source.into_value();
                let token = builder.token.into_value();

                $crate::builder::require_token_kind(token.kind(), $token_kind, $token_slot);

                let start = token.full_range().start();
                let mut node = $crate::builder::GreenNodeBuilder::new();

                node.push_token(token);

                Self {
                    source,
                    node: node.build($item_kind),
                    start,
                }
            }

            /// Returns the full source text range covered by this item.
            pub fn full_range(&self) -> bray_source::TextRange {
                $crate::SyntaxNode::full_range(self)
            }

            /// Returns whether this item contains parser recovery.
            pub fn is_recovered(&self) -> bool {
                self.$token_method().is_missing()
            }

            /// Returns the required token slot.
            pub fn $token_method(&self) -> $crate::SyntaxToken {
                match self.node.syntax_tokens(self.start).next() {
                    Some(token) => token,
                    None => panic!($missing_token_panic),
                }
            }

            /// Returns descendant syntax tokens in source order.
            pub fn tokens(&self) -> impl Iterator<Item = $crate::SyntaxToken> + '_ {
                self.node.syntax_tokens(self.start)
            }
        }

        impl std::fmt::Debug for $item_syntax {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter
                    .debug_struct($debug_name)
                    .field("source", &self.source)
                    .field("kind", &$crate::SyntaxNode::kind(self))
                    .field("full_range", &self.full_range())
                    .field("is_recovered", &self.is_recovered())
                    .finish()
            }
        }

        impl $crate::node::GreenSyntaxNode for $item_syntax {
            fn green_node(&self) -> &$crate::green::GreenNode {
                &self.node
            }

            fn start(&self) -> bray_source::TextSize {
                self.start
            }

            fn range_description(&self) -> &'static str {
                $range_description
            }

            fn is_recovered(&self) -> bool {
                $item_syntax::is_recovered(self)
            }
        }

        impl $crate::node::GreenSourceSyntaxNode for $item_syntax {
            fn source(&self) -> &bray_source::SourceSnapshot {
                &self.source
            }
        }

        /// Builder for this required-token item syntax node.
        $visibility struct $builder_syntax {
            source: $crate::builder::RequiredSyntaxSlot<bray_source::SourceSnapshot>,
            token: $crate::builder::RequiredSyntaxSlot<$crate::SyntaxToken>,
        }

        impl $builder_syntax {
            /// Creates an empty item builder.
            pub fn new(source: bray_source::SourceSnapshot) -> Self {
                let mut source_slot = $crate::builder::RequiredSyntaxSlot::new($source_slot);

                source_slot.set(source);

                Self {
                    source: source_slot,
                    token: $crate::builder::RequiredSyntaxSlot::new($token_slot),
                }
            }

            /// Sets the required token slot.
            pub fn $push_token_method(&mut self, token: $crate::SyntaxToken) {
                self.token.set(token);
            }

            /// Sets the required token slot.
            pub fn $token_method(mut self, token: $crate::SyntaxToken) -> Self {
                self.$push_token_method(token);

                self
            }

            /// Builds the item node.
            pub fn build(self) -> $item_syntax {
                $item_syntax::from_builder(self)
            }
        }

        impl std::fmt::Debug for $builder_syntax {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter
                    .debug_struct($builder_debug_name)
                    .finish_non_exhaustive()
            }
        }
    };
}

pub(crate) use define_token_item_syntax;
