macro_rules! define_lifecycle_body_node_with_required_result {
    (
        $(#[$node_meta:meta])*
        $node_syntax:ident, $builder_syntax:ident,
        kind: $node_kind:path,
        source_slot: $source_slot:literal,
        node_name: $node_name:literal,
        range_description: $range_description:literal,
        debug_name: $debug_name:literal,
        builder_debug_name: $builder_debug_name:literal,
        keyword_getter: $keyword_getter:ident,
        keyword_push: $keyword_push:ident,
        keyword_kind: $keyword_kind:path,
        keyword_slot: $keyword_slot:literal,
        modifiers_getter: $modifiers_getter:ident,
        modifiers_push: $modifiers_push:ident,
        modifiers_type: $modifiers_type:ty,
        modifiers_kind: $modifiers_kind:path $(,)?
    ) => {
        define_source_syntax_node! {
            $(#[$node_meta])*
            pub struct $node_syntax {
                builder: $builder_syntax,
                kind: $node_kind,
                source_slot: $source_slot,
                node_name: $node_name,
                range_description: $range_description,
                debug_name: $debug_name,
                builder_debug_name: $builder_debug_name,
                skipped_syntax: true,
                required_tokens: [
                    {
                        /// Returns the required lifecycle keyword token.
                        $keyword_getter;
                        /// Appends the lifecycle keyword token.
                        $keyword_push;
                        kind: $keyword_kind;
                        slot: $keyword_slot;
                    }
                ],
                optional_tokens: [],
                required_children: [
                    {
                        /// Returns the lifecycle modifiers child.
                        $modifiers_getter;
                        /// Appends the lifecycle modifiers child.
                        $modifiers_push;
                        ty: $modifiers_type;
                        kind: $modifiers_kind;
                    },
                    {
                        /// Returns the parameter-list child.
                        parameter_list;
                        /// Appends the parameter-list child.
                        push_parameter_list;
                        ty: ParameterListSyntax;
                        kind: SyntaxKind::ParameterList;
                    },
                    {
                        /// Returns the required callable result clause.
                        callable_result_clause;
                        /// Appends the callable result clause child.
                        push_callable_result_clause;
                        ty: CallableResultClauseSyntax;
                        kind: SyntaxKind::CallableResultClause;
                    },
                    {
                        /// Returns the required callable body block expression.
                        callable_body_block_expression;
                        /// Appends the callable body block expression child.
                        push_callable_body_block_expression;
                        ty: CallableBodyBlockExpressionSyntax;
                        kind: SyntaxKind::CallableBodyBlockExpression;
                    }
                ],
                repeated_children: [
                    {
                        /// Returns `requires(...)` clauses in source order.
                        requires_clauses;
                        /// Appends a `requires(...)` clause.
                        push_requires_clause;
                        ty: RequiresClauseSyntax;
                        kind: SyntaxKind::RequiresClause;
                    },
                    {
                        /// Returns `ensures(...)` clauses in source order.
                        ensures_clauses;
                        /// Appends an `ensures(...)` clause.
                        push_ensures_clause;
                        ty: EnsuresClauseSyntax;
                        kind: SyntaxKind::EnsuresClause;
                    },
                    {
                        /// Returns `executes(...)` clauses in source order.
                        executes_clauses;
                        /// Appends an `executes(...)` clause.
                        push_executes_clause;
                        ty: crate::ExecutesClauseSyntax;
                        kind: SyntaxKind::ExecutesClause;
                    },
                    {
                        /// Returns `when(...)` clauses in source order.
                        when_clauses;
                        /// Appends a `when(...)` clause.
                        push_when_clause;
                        ty: crate::WhenClauseSyntax;
                        kind: SyntaxKind::WhenClause;
                    },
                    {
                        /// Returns `with(...)` clauses in source order.
                        with_clauses;
                        /// Appends a `with(...)` clause.
                        push_with_clause;
                        ty: WithClauseSyntax;
                        kind: SyntaxKind::WithClause;
                    },
                    {
                        /// Returns `uses(...)` clauses in source order.
                        uses_clauses;
                        /// Appends a `uses(...)` clause.
                        push_uses_clause;
                        ty: UsesClauseSyntax;
                        kind: SyntaxKind::UsesClause;
                    }
                ],
            }
        }
    };
}

macro_rules! define_lifecycle_body_node_with_optional_result {
    (
        $(#[$node_meta:meta])*
        $node_syntax:ident, $builder_syntax:ident,
        kind: $node_kind:path,
        source_slot: $source_slot:literal,
        node_name: $node_name:literal,
        range_description: $range_description:literal,
        debug_name: $debug_name:literal,
        builder_debug_name: $builder_debug_name:literal,
        keyword_getter: $keyword_getter:ident,
        keyword_push: $keyword_push:ident,
        keyword_kind: $keyword_kind:path,
        keyword_slot: $keyword_slot:literal,
        modifiers_getter: $modifiers_getter:ident,
        modifiers_push: $modifiers_push:ident,
        modifiers_type: $modifiers_type:ty,
        modifiers_kind: $modifiers_kind:path $(,)?
    ) => {
        define_source_syntax_node! {
            $(#[$node_meta])*
            pub struct $node_syntax {
                builder: $builder_syntax,
                kind: $node_kind,
                source_slot: $source_slot,
                node_name: $node_name,
                range_description: $range_description,
                debug_name: $debug_name,
                builder_debug_name: $builder_debug_name,
                skipped_syntax: true,
                required_tokens: [
                    {
                        /// Returns the required lifecycle keyword token.
                        $keyword_getter;
                        /// Appends the lifecycle keyword token.
                        $keyword_push;
                        kind: $keyword_kind;
                        slot: $keyword_slot;
                    }
                ],
                optional_tokens: [],
                required_children: [
                    {
                        /// Returns the lifecycle modifiers child.
                        $modifiers_getter;
                        /// Appends the lifecycle modifiers child.
                        $modifiers_push;
                        ty: $modifiers_type;
                        kind: $modifiers_kind;
                    },
                    {
                        /// Returns the parameter-list child.
                        parameter_list;
                        /// Appends the parameter-list child.
                        push_parameter_list;
                        ty: ParameterListSyntax;
                        kind: SyntaxKind::ParameterList;
                    },
                    {
                        /// Returns the required callable body block expression.
                        callable_body_block_expression;
                        /// Appends the callable body block expression child.
                        push_callable_body_block_expression;
                        ty: CallableBodyBlockExpressionSyntax;
                        kind: SyntaxKind::CallableBodyBlockExpression;
                    }
                ],
                repeated_children: [
                    {
                        /// Returns callable result clauses in source order.
                        callable_result_clauses;
                        /// Appends a callable result clause child.
                        push_callable_result_clause;
                        ty: CallableResultClauseSyntax;
                        kind: SyntaxKind::CallableResultClause;
                    },
                    {
                        /// Returns `requires(...)` clauses in source order.
                        requires_clauses;
                        /// Appends a `requires(...)` clause.
                        push_requires_clause;
                        ty: RequiresClauseSyntax;
                        kind: SyntaxKind::RequiresClause;
                    },
                    {
                        /// Returns `ensures(...)` clauses in source order.
                        ensures_clauses;
                        /// Appends an `ensures(...)` clause.
                        push_ensures_clause;
                        ty: EnsuresClauseSyntax;
                        kind: SyntaxKind::EnsuresClause;
                    },
                    {
                        /// Returns `executes(...)` clauses in source order.
                        executes_clauses;
                        /// Appends an `executes(...)` clause.
                        push_executes_clause;
                        ty: crate::ExecutesClauseSyntax;
                        kind: SyntaxKind::ExecutesClause;
                    },
                    {
                        /// Returns `when(...)` clauses in source order.
                        when_clauses;
                        /// Appends a `when(...)` clause.
                        push_when_clause;
                        ty: crate::WhenClauseSyntax;
                        kind: SyntaxKind::WhenClause;
                    },
                    {
                        /// Returns `with(...)` clauses in source order.
                        with_clauses;
                        /// Appends a `with(...)` clause.
                        push_with_clause;
                        ty: WithClauseSyntax;
                        kind: SyntaxKind::WithClause;
                    },
                    {
                        /// Returns `uses(...)` clauses in source order.
                        uses_clauses;
                        /// Appends a `uses(...)` clause.
                        push_uses_clause;
                        ty: UsesClauseSyntax;
                        kind: SyntaxKind::UsesClause;
                    }
                ],
            }
        }

        impl $node_syntax {
            /// Returns the callable result clause child when present.
            pub fn callable_result_clause(&self) -> Option<CallableResultClauseSyntax> {
                child_nodes(
                    &self.source,
                    &self.node,
                    self.start,
                    SyntaxKind::CallableResultClause,
                    CallableResultClauseSyntax::from_green,
                )
                .next()
            }
        }
    };
}

macro_rules! define_lifecycle_requirement_node_with_required_result {
    (
        $(#[$node_meta:meta])*
        $node_syntax:ident, $builder_syntax:ident,
        kind: $node_kind:path,
        source_slot: $source_slot:literal,
        node_name: $node_name:literal,
        range_description: $range_description:literal,
        debug_name: $debug_name:literal,
        builder_debug_name: $builder_debug_name:literal,
        keyword_getter: $keyword_getter:ident,
        keyword_push: $keyword_push:ident,
        keyword_kind: $keyword_kind:path,
        keyword_slot: $keyword_slot:literal,
        modifiers_getter: $modifiers_getter:ident,
        modifiers_push: $modifiers_push:ident,
        modifiers_type: $modifiers_type:ty,
        modifiers_kind: $modifiers_kind:path,
        semicolon_slot: $semicolon_slot:literal $(,)?
    ) => {
        define_source_syntax_node! {
            $(#[$node_meta])*
            pub struct $node_syntax {
                builder: $builder_syntax,
                kind: $node_kind,
                source_slot: $source_slot,
                node_name: $node_name,
                range_description: $range_description,
                debug_name: $debug_name,
                builder_debug_name: $builder_debug_name,
                skipped_syntax: true,
                required_tokens: [
                    {
                        /// Returns the required lifecycle keyword token.
                        $keyword_getter;
                        /// Appends the lifecycle keyword token.
                        $keyword_push;
                        kind: $keyword_kind;
                        slot: $keyword_slot;
                    },
                    {
                        /// Returns the required semicolon token.
                        semicolon_token;
                        /// Appends the semicolon token.
                        push_semicolon_token;
                        kind: SyntaxKind::SemicolonToken;
                        slot: $semicolon_slot;
                    }
                ],
                optional_tokens: [],
                required_children: [
                    {
                        /// Returns the lifecycle modifiers child.
                        $modifiers_getter;
                        /// Appends the lifecycle modifiers child.
                        $modifiers_push;
                        ty: $modifiers_type;
                        kind: $modifiers_kind;
                    },
                    {
                        /// Returns the parameter-list child.
                        parameter_list;
                        /// Appends the parameter-list child.
                        push_parameter_list;
                        ty: ParameterListSyntax;
                        kind: SyntaxKind::ParameterList;
                    },
                    {
                        /// Returns the required callable result clause.
                        callable_result_clause;
                        /// Appends the callable result clause child.
                        push_callable_result_clause;
                        ty: CallableResultClauseSyntax;
                        kind: SyntaxKind::CallableResultClause;
                    }
                ],
                repeated_children: [
                    {
                        /// Returns `requires(...)` clauses in source order.
                        requires_clauses;
                        /// Appends a `requires(...)` clause.
                        push_requires_clause;
                        ty: RequiresClauseSyntax;
                        kind: SyntaxKind::RequiresClause;
                    },
                    {
                        /// Returns `ensures(...)` clauses in source order.
                        ensures_clauses;
                        /// Appends an `ensures(...)` clause.
                        push_ensures_clause;
                        ty: EnsuresClauseSyntax;
                        kind: SyntaxKind::EnsuresClause;
                    },
                    {
                        /// Returns `executes(...)` clauses in source order.
                        executes_clauses;
                        /// Appends an `executes(...)` clause.
                        push_executes_clause;
                        ty: crate::ExecutesClauseSyntax;
                        kind: SyntaxKind::ExecutesClause;
                    },
                    {
                        /// Returns `when(...)` clauses in source order.
                        when_clauses;
                        /// Appends a `when(...)` clause.
                        push_when_clause;
                        ty: crate::WhenClauseSyntax;
                        kind: SyntaxKind::WhenClause;
                    },
                    {
                        /// Returns `with(...)` clauses in source order.
                        with_clauses;
                        /// Appends a `with(...)` clause.
                        push_with_clause;
                        ty: WithClauseSyntax;
                        kind: SyntaxKind::WithClause;
                    },
                    {
                        /// Returns `uses(...)` clauses in source order.
                        uses_clauses;
                        /// Appends a `uses(...)` clause.
                        push_uses_clause;
                        ty: UsesClauseSyntax;
                        kind: SyntaxKind::UsesClause;
                    }
                ],
            }
        }
    };
}

macro_rules! define_lifecycle_requirement_node_with_optional_result {
    (
        $(#[$node_meta:meta])*
        $node_syntax:ident, $builder_syntax:ident,
        kind: $node_kind:path,
        source_slot: $source_slot:literal,
        node_name: $node_name:literal,
        range_description: $range_description:literal,
        debug_name: $debug_name:literal,
        builder_debug_name: $builder_debug_name:literal,
        keyword_getter: $keyword_getter:ident,
        keyword_push: $keyword_push:ident,
        keyword_kind: $keyword_kind:path,
        keyword_slot: $keyword_slot:literal,
        modifiers_getter: $modifiers_getter:ident,
        modifiers_push: $modifiers_push:ident,
        modifiers_type: $modifiers_type:ty,
        modifiers_kind: $modifiers_kind:path,
        semicolon_slot: $semicolon_slot:literal $(,)?
    ) => {
        define_source_syntax_node! {
            $(#[$node_meta])*
            pub struct $node_syntax {
                builder: $builder_syntax,
                kind: $node_kind,
                source_slot: $source_slot,
                node_name: $node_name,
                range_description: $range_description,
                debug_name: $debug_name,
                builder_debug_name: $builder_debug_name,
                skipped_syntax: true,
                required_tokens: [
                    {
                        /// Returns the required lifecycle keyword token.
                        $keyword_getter;
                        /// Appends the lifecycle keyword token.
                        $keyword_push;
                        kind: $keyword_kind;
                        slot: $keyword_slot;
                    },
                    {
                        /// Returns the required semicolon token.
                        semicolon_token;
                        /// Appends the semicolon token.
                        push_semicolon_token;
                        kind: SyntaxKind::SemicolonToken;
                        slot: $semicolon_slot;
                    }
                ],
                optional_tokens: [],
                required_children: [
                    {
                        /// Returns the lifecycle modifiers child.
                        $modifiers_getter;
                        /// Appends the lifecycle modifiers child.
                        $modifiers_push;
                        ty: $modifiers_type;
                        kind: $modifiers_kind;
                    },
                    {
                        /// Returns the parameter-list child.
                        parameter_list;
                        /// Appends the parameter-list child.
                        push_parameter_list;
                        ty: ParameterListSyntax;
                        kind: SyntaxKind::ParameterList;
                    }
                ],
                repeated_children: [
                    {
                        /// Returns callable result clauses in source order.
                        callable_result_clauses;
                        /// Appends a callable result clause child.
                        push_callable_result_clause;
                        ty: CallableResultClauseSyntax;
                        kind: SyntaxKind::CallableResultClause;
                    },
                    {
                        /// Returns `requires(...)` clauses in source order.
                        requires_clauses;
                        /// Appends a `requires(...)` clause.
                        push_requires_clause;
                        ty: RequiresClauseSyntax;
                        kind: SyntaxKind::RequiresClause;
                    },
                    {
                        /// Returns `ensures(...)` clauses in source order.
                        ensures_clauses;
                        /// Appends an `ensures(...)` clause.
                        push_ensures_clause;
                        ty: EnsuresClauseSyntax;
                        kind: SyntaxKind::EnsuresClause;
                    },
                    {
                        /// Returns `executes(...)` clauses in source order.
                        executes_clauses;
                        /// Appends an `executes(...)` clause.
                        push_executes_clause;
                        ty: crate::ExecutesClauseSyntax;
                        kind: SyntaxKind::ExecutesClause;
                    },
                    {
                        /// Returns `when(...)` clauses in source order.
                        when_clauses;
                        /// Appends a `when(...)` clause.
                        push_when_clause;
                        ty: crate::WhenClauseSyntax;
                        kind: SyntaxKind::WhenClause;
                    },
                    {
                        /// Returns `with(...)` clauses in source order.
                        with_clauses;
                        /// Appends a `with(...)` clause.
                        push_with_clause;
                        ty: WithClauseSyntax;
                        kind: SyntaxKind::WithClause;
                    },
                    {
                        /// Returns `uses(...)` clauses in source order.
                        uses_clauses;
                        /// Appends a `uses(...)` clause.
                        push_uses_clause;
                        ty: UsesClauseSyntax;
                        kind: SyntaxKind::UsesClause;
                    }
                ],
            }
        }

        impl $node_syntax {
            /// Returns the callable result clause child when present.
            pub fn callable_result_clause(&self) -> Option<CallableResultClauseSyntax> {
                child_nodes(
                    &self.source,
                    &self.node,
                    self.start,
                    SyntaxKind::CallableResultClause,
                    CallableResultClauseSyntax::from_green,
                )
                .next()
            }
        }
    };
}
