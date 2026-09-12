use super::modifiers::{
    AsyncCapableLifecycleMemberModifiersSyntax, ScopeEnterMemberModifiersSyntax,
    SyncLifecycleMemberModifiersSyntax,
};
use crate::node::{child_nodes, define_source_syntax_node};
use crate::syntax::callable::{
    CallableBodyBlockExpressionSyntax, CallableResultClauseSyntax, ParameterListSyntax,
};
use crate::{
    EnsuresClauseSyntax, RequiresClauseSyntax, SyntaxKind, UsesClauseSyntax, WithClauseSyntax,
};

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
    modifiers_getter: scope_enter_member_modifiers,
    modifiers_push: push_scope_enter_member_modifiers,
    modifiers_type: ScopeEnterMemberModifiersSyntax,
    modifiers_kind: SyntaxKind::ScopeEnterMemberModifiers,
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
    modifiers_getter: scope_enter_member_modifiers,
    modifiers_push: push_scope_enter_member_modifiers,
    modifiers_type: ScopeEnterMemberModifiersSyntax,
    modifiers_kind: SyntaxKind::ScopeEnterMemberModifiers,
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
