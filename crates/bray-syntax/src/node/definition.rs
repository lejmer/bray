macro_rules! define_source_syntax_node {
    (
        $(#[$node_meta:meta])*
        $visibility:vis struct $node_syntax:ident {
            builder: $builder_syntax:ident,
            kind: $node_kind:path,
            source_slot: $source_slot:literal,
            node_name: $node_name:literal,
            range_description: $range_description:literal,
            debug_name: $debug_name:literal,
            builder_debug_name: $builder_debug_name:literal,
            skipped_syntax: true,
            required_tokens: [$($required_tokens:tt)*],
            optional_tokens: [$($optional_tokens:tt)*],
            required_children: [$($required_children:tt)*] $(,)?
        }
    ) => {
        define_source_syntax_node! {
            @definition
            $(#[$node_meta])*
            $visibility struct $node_syntax {
                builder: $builder_syntax,
                kind: $node_kind,
                source_slot: $source_slot,
                node_name: $node_name,
                range_description: $range_description,
                debug_name: $debug_name,
                builder_debug_name: $builder_debug_name,
                required_tokens: [$($required_tokens)*],
                optional_tokens: [$($optional_tokens)*],
                required_children: [$($required_children)*],
                repeated_children: [],
                node_recovery_methods: [
                    /// Returns descendant skipped-syntax recovery nodes in source order.
                    pub fn skipped_syntax(&self) -> impl Iterator<Item = $crate::SkippedSyntax> + '_ {
                        $crate::node::skipped_syntax(&self.source, &self.node, self.start)
                    }
                ],
                builder_recovery_methods: [
                    /// Appends present source tokens under a skipped-syntax recovery node.
                    pub fn push_skipped_tokens(
                        &mut self,
                        tokens: impl IntoIterator<Item = $crate::SyntaxToken>,
                    ) {
                        self.node.push_skipped_tokens(tokens);
                    }
                ],
            }
        }
    };

    (
        $(#[$node_meta:meta])*
        $visibility:vis struct $node_syntax:ident {
            builder: $builder_syntax:ident,
            kind: $node_kind:path,
            source_slot: $source_slot:literal,
            node_name: $node_name:literal,
            range_description: $range_description:literal,
            debug_name: $debug_name:literal,
            builder_debug_name: $builder_debug_name:literal,
            skipped_syntax: false,
            required_tokens: [$($required_tokens:tt)*],
            optional_tokens: [$($optional_tokens:tt)*],
            required_children: [$($required_children:tt)*] $(,)?
        }
    ) => {
        define_source_syntax_node! {
            @definition
            $(#[$node_meta])*
            $visibility struct $node_syntax {
                builder: $builder_syntax,
                kind: $node_kind,
                source_slot: $source_slot,
                node_name: $node_name,
                range_description: $range_description,
                debug_name: $debug_name,
                builder_debug_name: $builder_debug_name,
                required_tokens: [$($required_tokens)*],
                optional_tokens: [$($optional_tokens)*],
                required_children: [$($required_children)*],
                repeated_children: [],
                node_recovery_methods: [],
                builder_recovery_methods: [],
            }
        }
    };

    (
        $(#[$node_meta:meta])*
        $visibility:vis struct $node_syntax:ident {
            builder: $builder_syntax:ident,
            kind: $node_kind:path,
            source_slot: $source_slot:literal,
            node_name: $node_name:literal,
            range_description: $range_description:literal,
            debug_name: $debug_name:literal,
            builder_debug_name: $builder_debug_name:literal,
            skipped_syntax: true,
            required_tokens: [$($required_tokens:tt)*],
            optional_tokens: [$($optional_tokens:tt)*],
            required_children: [$($required_children:tt)*],
            repeated_children: [$($repeated_children:tt)*] $(,)?
        }
    ) => {
        define_source_syntax_node! {
            @definition
            $(#[$node_meta])*
            $visibility struct $node_syntax {
                builder: $builder_syntax,
                kind: $node_kind,
                source_slot: $source_slot,
                node_name: $node_name,
                range_description: $range_description,
                debug_name: $debug_name,
                builder_debug_name: $builder_debug_name,
                required_tokens: [$($required_tokens)*],
                optional_tokens: [$($optional_tokens)*],
                required_children: [$($required_children)*],
                repeated_children: [$($repeated_children)*],
                node_recovery_methods: [
                    /// Returns descendant skipped-syntax recovery nodes in source order.
                    pub fn skipped_syntax(&self) -> impl Iterator<Item = $crate::SkippedSyntax> + '_ {
                        $crate::node::skipped_syntax(&self.source, &self.node, self.start)
                    }
                ],
                builder_recovery_methods: [
                    /// Appends present source tokens under a skipped-syntax recovery node.
                    pub fn push_skipped_tokens(
                        &mut self,
                        tokens: impl IntoIterator<Item = $crate::SyntaxToken>,
                    ) {
                        self.node.push_skipped_tokens(tokens);
                    }
                ],
            }
        }
    };

    (
        $(#[$node_meta:meta])*
        $visibility:vis struct $node_syntax:ident {
            builder: $builder_syntax:ident,
            kind: $node_kind:path,
            source_slot: $source_slot:literal,
            node_name: $node_name:literal,
            range_description: $range_description:literal,
            debug_name: $debug_name:literal,
            builder_debug_name: $builder_debug_name:literal,
            skipped_syntax: false,
            required_tokens: [$($required_tokens:tt)*],
            optional_tokens: [$($optional_tokens:tt)*],
            required_children: [$($required_children:tt)*],
            repeated_children: [$($repeated_children:tt)*] $(,)?
        }
    ) => {
        define_source_syntax_node! {
            @definition
            $(#[$node_meta])*
            $visibility struct $node_syntax {
                builder: $builder_syntax,
                kind: $node_kind,
                source_slot: $source_slot,
                node_name: $node_name,
                range_description: $range_description,
                debug_name: $debug_name,
                builder_debug_name: $builder_debug_name,
                required_tokens: [$($required_tokens)*],
                optional_tokens: [$($optional_tokens)*],
                required_children: [$($required_children)*],
                repeated_children: [$($repeated_children)*],
                node_recovery_methods: [],
                builder_recovery_methods: [],
            }
        }
    };

    (
        @definition
        $(#[$node_meta:meta])*
        $visibility:vis struct $node_syntax:ident {
            builder: $builder_syntax:ident,
            kind: $node_kind:path,
            source_slot: $source_slot:literal,
            node_name: $node_name:literal,
            range_description: $range_description:literal,
            debug_name: $debug_name:literal,
            builder_debug_name: $builder_debug_name:literal,
            required_tokens: [
                $(
                    {
                        $(#[$required_token_getter_meta:meta])*
                        $required_token_getter:ident;
                        $(#[$required_token_push_meta:meta])*
                        $required_token_push:ident;
                        kind: $required_token_kind:path;
                        slot: $required_token_slot:literal;
                    }
                ),* $(,)?
            ],
            optional_tokens: [
                $(
                    {
                        $(#[$optional_token_getter_meta:meta])*
                        $optional_token_getter:ident;
                        $(#[$optional_token_push_meta:meta])*
                        $optional_token_push:ident;
                        kind: $optional_token_kind:path;
                        slot: $optional_token_slot:literal;
                    }
                ),* $(,)?
            ],
            required_children: [
                $(
                    {
                        $(#[$required_child_getter_meta:meta])*
                        $required_child_getter:ident;
                        $(#[$required_child_push_meta:meta])*
                        $required_child_push:ident;
                        ty: $required_child_type:ty;
                        kind: $required_child_kind:path;
                    }
                ),* $(,)?
            ],
            repeated_children: [
                $(
                    {
                        $(#[$repeated_child_getter_meta:meta])*
                        $repeated_child_getter:ident;
                        $(#[$repeated_child_push_meta:meta])*
                        $repeated_child_push:ident;
                        ty: $repeated_child_type:ty;
                        kind: $repeated_child_kind:path;
                    }
                ),* $(,)?
            ],
            node_recovery_methods: [$($node_recovery_methods:item)*],
            builder_recovery_methods: [$($builder_recovery_methods:item)*],
        }
    ) => {
        $(#[$node_meta])*
        #[derive(Clone, Eq, Hash, PartialEq)]
        $visibility struct $node_syntax {
            source: bray_source::SourceSnapshot,
            node: $crate::green::GreenNode,
            start: bray_source::TextSize,
        }

        impl $node_syntax {
            /// Creates a builder for this syntax node at `start`.
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
                assert_eq!(node.kind(), $node_kind);

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
                Self {
                    source: builder.source.into_value(),
                    node: builder.node.build($node_kind),
                    start: builder.start,
                }
            }

            /// Returns the full source text range covered by this node.
            pub fn full_range(&self) -> bray_source::TextRange {
                $crate::SyntaxNode::full_range(self)
            }

            /// Returns whether this node contains parser recovery.
            pub fn is_recovered(&self) -> bool {
                $crate::node::contains_recovery(&self.node, self.start)
            }

            $(
                $(#[$required_token_getter_meta])*
                pub fn $required_token_getter(&self) -> $crate::SyntaxToken {
                    $crate::node::required_token(
                        &self.node,
                        self.start,
                        $required_token_kind,
                        $node_name,
                    )
                }
            )*

            $(
                $(#[$optional_token_getter_meta])*
                pub fn $optional_token_getter(&self) -> Option<$crate::SyntaxToken> {
                    $crate::node::first_token(
                        &self.node,
                        self.start,
                        $optional_token_kind,
                    )
                }
            )*

            $(
                $(#[$required_child_getter_meta])*
                pub fn $required_child_getter(&self) -> $required_child_type {
                    $crate::node::required_child_node(
                        &self.source,
                        &self.node,
                        self.start,
                        $required_child_kind,
                        <$required_child_type>::from_green,
                        $node_name,
                    )
                }
            )*

            $(
                $(#[$repeated_child_getter_meta])*
                pub fn $repeated_child_getter(&self) -> impl Iterator<Item = $repeated_child_type> + '_ {
                    $crate::node::child_nodes(
                        &self.source,
                        &self.node,
                        self.start,
                        $repeated_child_kind,
                        <$repeated_child_type>::from_green,
                    )
                }
            )*

            /// Returns descendant syntax tokens in source order.
            pub fn tokens(&self) -> impl Iterator<Item = $crate::SyntaxToken> + '_ {
                self.node.syntax_tokens(self.start)
            }

            $($node_recovery_methods)*
        }

        impl std::fmt::Debug for $node_syntax {
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

        impl $crate::node::GreenSyntaxNode for $node_syntax {
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
                $node_syntax::is_recovered(self)
            }
        }

        impl $crate::node::GreenSourceSyntaxNode for $node_syntax {
            fn source(&self) -> &bray_source::SourceSnapshot {
                &self.source
            }
        }

        /// Builder for this syntax node.
        $visibility struct $builder_syntax {
            source: $crate::builder::RequiredSyntaxSlot<bray_source::SourceSnapshot>,
            node: $crate::builder::GreenNodeBuilder,
            start: bray_source::TextSize,
        }

        impl $builder_syntax {
            /// Creates an empty syntax-node builder at `start`.
            pub fn new(source: bray_source::SourceSnapshot, start: bray_source::TextSize) -> Self {
                let mut source_slot = $crate::builder::RequiredSyntaxSlot::new($source_slot);

                source_slot.set(source);

                Self {
                    source: source_slot,
                    node: $crate::builder::GreenNodeBuilder::new(),
                    start,
                }
            }

            $(
                $(#[$required_token_push_meta])*
                pub fn $required_token_push(&mut self, token: $crate::SyntaxToken) {
                    $crate::builder::require_token_kind(
                        token.kind(),
                        $required_token_kind,
                        $required_token_slot,
                    );

                    self.node.push_token(token);
                }
            )*

            $(
                $(#[$optional_token_push_meta])*
                pub fn $optional_token_push(&mut self, token: $crate::SyntaxToken) {
                    $crate::builder::require_token_kind(
                        token.kind(),
                        $optional_token_kind,
                        $optional_token_slot,
                    );

                    self.node.push_token(token);
                }
            )*

            $(
                $(#[$required_child_push_meta])*
                pub fn $required_child_push(&mut self, child: $required_child_type) {
                    self.node.push_node(child.into_green());
                }
            )*

            $(
                $(#[$repeated_child_push_meta])*
                pub fn $repeated_child_push(&mut self, child: $repeated_child_type) {
                    self.node.push_node(child.into_green());
                }
            )*

            $($builder_recovery_methods)*

            /// Builds the syntax node.
            pub fn build(self) -> $node_syntax {
                $node_syntax::from_builder(self)
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

pub(crate) use define_source_syntax_node;
