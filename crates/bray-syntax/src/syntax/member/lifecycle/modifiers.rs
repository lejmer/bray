use crate::SyntaxKind;
use crate::node::define_source_syntax_node;

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

define_source_syntax_node! {
    /// Optional scope-enter member modifiers in source order.
    pub struct ScopeEnterMemberModifiersSyntax {
        builder: ScopeEnterMemberModifiersSyntaxBuilder,
        kind: SyntaxKind::ScopeEnterMemberModifiers,
        source_slot: "scope_enter_member_modifiers.source",
        node_name: "scope-enter member modifiers",
        range_description: "scope-enter-member-modifiers",
        debug_name: "ScopeEnterMemberModifiersSyntax",
        builder_debug_name: "ScopeEnterMemberModifiersSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [],
        optional_tokens: [
            {
                /// Returns the first optional `async` modifier token.
                async_token;
                /// Appends an `async` modifier token.
                push_async_token;
                kind: SyntaxKind::AsyncKeyword;
                slot: "scope_enter_member_modifiers.async_token";
            },
            {
                /// Returns the first optional `trusted` modifier token.
                trusted_token;
                /// Appends a `trusted` modifier token.
                push_trusted_token;
                kind: SyntaxKind::TrustedKeyword;
                slot: "scope_enter_member_modifiers.trusted_token";
            },
            {
                /// Returns the first optional `consume` receiver modifier token.
                consume_token;
                /// Appends a `consume` receiver modifier token.
                push_consume_token;
                kind: SyntaxKind::ConsumeKeyword;
                slot: "scope_enter_member_modifiers.consume_token";
            },
            {
                /// Returns the first optional `mut` receiver modifier token.
                mut_token;
                /// Appends a `mut` receiver modifier token.
                push_mut_token;
                kind: SyntaxKind::MutKeyword;
                slot: "scope_enter_member_modifiers.mut_token";
            }
        ],
        required_children: [],
    }
}
