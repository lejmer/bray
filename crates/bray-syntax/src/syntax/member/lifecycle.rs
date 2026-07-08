use super::super::callable::{
    CallableBodyBlockExpressionSyntax, CallableResultClauseSyntax, ParameterListSyntax,
};
use crate::node::{child_nodes, define_source_syntax_node};
use crate::{
    EnsuresClauseSyntax, RequiresClauseSyntax, SyntaxKind, UsesClauseSyntax, WithClauseSyntax,
};

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

define_source_syntax_node! {
    /// Optional async-capable lifecycle member modifiers in source order.
    pub struct AsyncCapableLifecycleMemberModifiersSyntax {
        builder: AsyncCapableLifecycleMemberModifiersSyntaxBuilder,
        kind: SyntaxKind::AsyncCapableLifecycleMemberModifiers,
        source_slot: "async_capable_lifecycle_member_modifiers.source",
        node_name: "async-capable lifecycle member modifiers",
        range_description: "async-capable-lifecycle-member-modifiers",
        debug_name: "AsyncCapableLifecycleMemberModifiersSyntax",
        builder_debug_name: "AsyncCapableLifecycleMemberModifiersSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [],
        optional_tokens: [
            {
                /// Returns the first optional `async` modifier token.
                async_token;
                /// Appends an `async` modifier token.
                push_async_token;
                kind: SyntaxKind::AsyncKeyword;
                slot: "async_capable_lifecycle_member_modifiers.async_token";
            },
            {
                /// Returns the first optional `trusted` modifier token.
                trusted_token;
                /// Appends a `trusted` modifier token.
                push_trusted_token;
                kind: SyntaxKind::TrustedKeyword;
                slot: "async_capable_lifecycle_member_modifiers.trusted_token";
            }
        ],
        required_children: [],
    }
}

define_source_syntax_node! {
    /// Optional synchronous lifecycle member modifiers in source order.
    pub struct SyncLifecycleMemberModifiersSyntax {
        builder: SyncLifecycleMemberModifiersSyntaxBuilder,
        kind: SyntaxKind::SyncLifecycleMemberModifiers,
        source_slot: "sync_lifecycle_member_modifiers.source",
        node_name: "sync lifecycle member modifiers",
        range_description: "sync-lifecycle-member-modifiers",
        debug_name: "SyncLifecycleMemberModifiersSyntax",
        builder_debug_name: "SyncLifecycleMemberModifiersSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [],
        optional_tokens: [
            {
                /// Returns the first optional `trusted` modifier token.
                trusted_token;
                /// Appends a `trusted` modifier token.
                push_trusted_token;
                kind: SyntaxKind::TrustedKeyword;
                slot: "sync_lifecycle_member_modifiers.trusted_token";
            }
        ],
        required_children: [],
    }
}

define_lifecycle_body_node_with_optional_result! {
    /// Finalizer lifecycle member declaration.
    FinalizerMemberDeclarationSyntax, FinalizerMemberDeclarationSyntaxBuilder,
    kind: SyntaxKind::FinalizerMemberDeclaration,
    source_slot: "finalizer_member_declaration.source",
    node_name: "finalizer member declaration",
    range_description: "finalizer-member-declaration",
    debug_name: "FinalizerMemberDeclarationSyntax",
    builder_debug_name: "FinalizerMemberDeclarationSyntaxBuilder",
    keyword_getter: finalize_keyword,
    keyword_push: push_finalize_keyword,
    keyword_kind: SyntaxKind::FinalizeKeyword,
    keyword_slot: "finalizer_member_declaration.finalize_keyword",
    modifiers_getter: async_capable_lifecycle_member_modifiers,
    modifiers_push: push_async_capable_lifecycle_member_modifiers,
    modifiers_type: AsyncCapableLifecycleMemberModifiersSyntax,
    modifiers_kind: SyntaxKind::AsyncCapableLifecycleMemberModifiers,
}

define_lifecycle_body_node_with_optional_result! {
    /// Destructor lifecycle member declaration.
    DestructorMemberDeclarationSyntax, DestructorMemberDeclarationSyntaxBuilder,
    kind: SyntaxKind::DestructorMemberDeclaration,
    source_slot: "destructor_member_declaration.source",
    node_name: "destructor member declaration",
    range_description: "destructor-member-declaration",
    debug_name: "DestructorMemberDeclarationSyntax",
    builder_debug_name: "DestructorMemberDeclarationSyntaxBuilder",
    keyword_getter: destruct_keyword,
    keyword_push: push_destruct_keyword,
    keyword_kind: SyntaxKind::DestructKeyword,
    keyword_slot: "destructor_member_declaration.destruct_keyword",
    modifiers_getter: sync_lifecycle_member_modifiers,
    modifiers_push: push_sync_lifecycle_member_modifiers,
    modifiers_type: SyncLifecycleMemberModifiersSyntax,
    modifiers_kind: SyntaxKind::SyncLifecycleMemberModifiers,
}

define_lifecycle_body_node_with_required_result! {
    /// Scope-enter lifecycle member declaration.
    ScopeEnterMemberDeclarationSyntax, ScopeEnterMemberDeclarationSyntaxBuilder,
    kind: SyntaxKind::ScopeEnterMemberDeclaration,
    source_slot: "scope_enter_member_declaration.source",
    node_name: "scope-enter member declaration",
    range_description: "scope-enter-member-declaration",
    debug_name: "ScopeEnterMemberDeclarationSyntax",
    builder_debug_name: "ScopeEnterMemberDeclarationSyntaxBuilder",
    keyword_getter: enter_keyword,
    keyword_push: push_enter_keyword,
    keyword_kind: SyntaxKind::EnterKeyword,
    keyword_slot: "scope_enter_member_declaration.enter_keyword",
    modifiers_getter: async_capable_lifecycle_member_modifiers,
    modifiers_push: push_async_capable_lifecycle_member_modifiers,
    modifiers_type: AsyncCapableLifecycleMemberModifiersSyntax,
    modifiers_kind: SyntaxKind::AsyncCapableLifecycleMemberModifiers,
}

define_lifecycle_body_node_with_optional_result! {
    /// Scope-exit lifecycle member declaration.
    ScopeExitMemberDeclarationSyntax, ScopeExitMemberDeclarationSyntaxBuilder,
    kind: SyntaxKind::ScopeExitMemberDeclaration,
    source_slot: "scope_exit_member_declaration.source",
    node_name: "scope-exit member declaration",
    range_description: "scope-exit-member-declaration",
    debug_name: "ScopeExitMemberDeclarationSyntax",
    builder_debug_name: "ScopeExitMemberDeclarationSyntaxBuilder",
    keyword_getter: exit_keyword,
    keyword_push: push_exit_keyword,
    keyword_kind: SyntaxKind::ExitKeyword,
    keyword_slot: "scope_exit_member_declaration.exit_keyword",
    modifiers_getter: async_capable_lifecycle_member_modifiers,
    modifiers_push: push_async_capable_lifecycle_member_modifiers,
    modifiers_type: AsyncCapableLifecycleMemberModifiersSyntax,
    modifiers_kind: SyntaxKind::AsyncCapableLifecycleMemberModifiers,
}

define_lifecycle_requirement_node_with_optional_result! {
    /// Trait finalizer requirement declaration.
    TraitFinalizerRequirementDeclarationSyntax, TraitFinalizerRequirementDeclarationSyntaxBuilder,
    kind: SyntaxKind::TraitFinalizerRequirementDeclaration,
    source_slot: "trait_finalizer_requirement_declaration.source",
    node_name: "trait finalizer requirement declaration",
    range_description: "trait-finalizer-requirement-declaration",
    debug_name: "TraitFinalizerRequirementDeclarationSyntax",
    builder_debug_name: "TraitFinalizerRequirementDeclarationSyntaxBuilder",
    keyword_getter: finalize_keyword,
    keyword_push: push_finalize_keyword,
    keyword_kind: SyntaxKind::FinalizeKeyword,
    keyword_slot: "trait_finalizer_requirement_declaration.finalize_keyword",
    modifiers_getter: async_capable_lifecycle_member_modifiers,
    modifiers_push: push_async_capable_lifecycle_member_modifiers,
    modifiers_type: AsyncCapableLifecycleMemberModifiersSyntax,
    modifiers_kind: SyntaxKind::AsyncCapableLifecycleMemberModifiers,
    semicolon_slot: "trait_finalizer_requirement_declaration.semicolon_token",
}

define_lifecycle_requirement_node_with_optional_result! {
    /// Trait destructor requirement declaration.
    TraitDestructorRequirementDeclarationSyntax, TraitDestructorRequirementDeclarationSyntaxBuilder,
    kind: SyntaxKind::TraitDestructorRequirementDeclaration,
    source_slot: "trait_destructor_requirement_declaration.source",
    node_name: "trait destructor requirement declaration",
    range_description: "trait-destructor-requirement-declaration",
    debug_name: "TraitDestructorRequirementDeclarationSyntax",
    builder_debug_name: "TraitDestructorRequirementDeclarationSyntaxBuilder",
    keyword_getter: destruct_keyword,
    keyword_push: push_destruct_keyword,
    keyword_kind: SyntaxKind::DestructKeyword,
    keyword_slot: "trait_destructor_requirement_declaration.destruct_keyword",
    modifiers_getter: sync_lifecycle_member_modifiers,
    modifiers_push: push_sync_lifecycle_member_modifiers,
    modifiers_type: SyncLifecycleMemberModifiersSyntax,
    modifiers_kind: SyntaxKind::SyncLifecycleMemberModifiers,
    semicolon_slot: "trait_destructor_requirement_declaration.semicolon_token",
}

define_lifecycle_requirement_node_with_required_result! {
    /// Trait scope-enter requirement declaration.
    TraitScopeEnterRequirementDeclarationSyntax, TraitScopeEnterRequirementDeclarationSyntaxBuilder,
    kind: SyntaxKind::TraitScopeEnterRequirementDeclaration,
    source_slot: "trait_scope_enter_requirement_declaration.source",
    node_name: "trait scope-enter requirement declaration",
    range_description: "trait-scope-enter-requirement-declaration",
    debug_name: "TraitScopeEnterRequirementDeclarationSyntax",
    builder_debug_name: "TraitScopeEnterRequirementDeclarationSyntaxBuilder",
    keyword_getter: enter_keyword,
    keyword_push: push_enter_keyword,
    keyword_kind: SyntaxKind::EnterKeyword,
    keyword_slot: "trait_scope_enter_requirement_declaration.enter_keyword",
    modifiers_getter: async_capable_lifecycle_member_modifiers,
    modifiers_push: push_async_capable_lifecycle_member_modifiers,
    modifiers_type: AsyncCapableLifecycleMemberModifiersSyntax,
    modifiers_kind: SyntaxKind::AsyncCapableLifecycleMemberModifiers,
    semicolon_slot: "trait_scope_enter_requirement_declaration.semicolon_token",
}

define_lifecycle_requirement_node_with_optional_result! {
    /// Trait scope-exit requirement declaration.
    TraitScopeExitRequirementDeclarationSyntax, TraitScopeExitRequirementDeclarationSyntaxBuilder,
    kind: SyntaxKind::TraitScopeExitRequirementDeclaration,
    source_slot: "trait_scope_exit_requirement_declaration.source",
    node_name: "trait scope-exit requirement declaration",
    range_description: "trait-scope-exit-requirement-declaration",
    debug_name: "TraitScopeExitRequirementDeclarationSyntax",
    builder_debug_name: "TraitScopeExitRequirementDeclarationSyntaxBuilder",
    keyword_getter: exit_keyword,
    keyword_push: push_exit_keyword,
    keyword_kind: SyntaxKind::ExitKeyword,
    keyword_slot: "trait_scope_exit_requirement_declaration.exit_keyword",
    modifiers_getter: async_capable_lifecycle_member_modifiers,
    modifiers_push: push_async_capable_lifecycle_member_modifiers,
    modifiers_type: AsyncCapableLifecycleMemberModifiersSyntax,
    modifiers_kind: SyntaxKind::AsyncCapableLifecycleMemberModifiers,
    semicolon_slot: "trait_scope_exit_requirement_declaration.semicolon_token",
}
