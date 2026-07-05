use super::separated::define_token_separated_list_syntax;
use crate::SyntaxKind;

#[cfg(test)]
use super::separated::test_token_separated_list_syntax;

define_token_separated_list_syntax! {
    list {
        /// Concrete comma-separated identifier list syntax node.
        ///
        /// The node stores item nodes, comma tokens, missing comma tokens, and recovery
        /// nodes in source order.
        pub struct IdentifierListSyntax {
            builder: IdentifierListSyntaxBuilder,
            kind: SyntaxKind::IdentifierList,
            items: items,
            separators: separator_tokens,
            source_slot: "identifier_list.source",
            range_description: "identifier-list",
            debug_name: "IdentifierListSyntax",
            builder_debug_name: "IdentifierListSyntaxBuilder",
        }
    }
    item {
        /// Concrete identifier item syntax node.
        pub struct IdentifierListItemSyntax {
            builder: IdentifierListItemSyntaxBuilder,
            kind: SyntaxKind::IdentifierListItem,
            token_kind: SyntaxKind::IdentifierToken,
            token: identifier_token,
            push_token: push_identifier_token,
            source_slot: "identifier_list_item.source",
            token_slot: "identifier_list_item.identifier_token",
            range_description: "identifier-list item",
            debug_name: "IdentifierListItemSyntax",
            builder_debug_name: "IdentifierListItemSyntaxBuilder",
            missing_token_panic: "identifier-list item must contain an identifier token",
        }
    }
    separator_kind: SyntaxKind::CommaToken,
}

#[cfg(test)]
test_token_separated_list_syntax! {
    mod tests {
        list: IdentifierListSyntax,
        item: IdentifierListItemSyntax,
        list_kind: crate::SyntaxKind::IdentifierList,
        item_kind: crate::SyntaxKind::IdentifierListItem,
        token_kind: crate::SyntaxKind::IdentifierToken,
        token: identifier_token,
        separator_kind: crate::SyntaxKind::CommaToken,
        separators: separator_tokens,
        items: items,
        source_name: "syntax-test",
    }
}
