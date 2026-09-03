macro_rules! test_token_list_syntax {
    (
        mod $module:ident {
            list: $list_syntax:ident,
            item: $item_syntax:ident,
            list_kind: $list_kind:path,
            item_kind: $item_kind:path,
            token_kind: $token_kind:path,
            token: $token_method:ident,
            separator_kind: $separator_kind:path,
            separators: $separator_tokens_method:ident,
            items: $items_method:ident,
            source_name: $source_name:literal $(,)?
        }
    ) => {
        mod $module {
            use bray_source::{
                SourceId, SourceIdentity, SourceOrigin, SourceSnapshot, SourceVersion, TextRange,
                TextSize,
            };

            use super::{$item_syntax, $list_syntax};
            use $crate::{
                SeparatedSyntaxNode, SourceSyntaxNode, SyntaxNode, SyntaxText, SyntaxToken,
                SyntaxTrivia,
            };

            #[test]
            fn stores_items_separators_and_source_text() {
                let snapshot = snapshot("a, b");
                let first_token = token(0, 1);

                let separator = SyntaxToken::new(
                    $separator_kind,
                    TextRange::new(TextSize::new(1), TextSize::new(2)),
                )
                .with_trailing_trivia([SyntaxTrivia::whitespace(TextRange::new(
                    TextSize::new(2),
                    TextSize::new(3),
                ))]);

                let second_token = token(3, 4);
                let first = item(&snapshot, first_token.clone());
                let second = item(&snapshot, second_token.clone());

                let list = $list_syntax::builder(snapshot.clone(), TextSize::ZERO)
                    .item(first.clone())
                    .separator_token(separator.clone())
                    .item(second.clone())
                    .build();

                assert_eq!(list.kind(), $list_kind);

                assert_eq!(
                    list.full_range(),
                    TextRange::new(TextSize::ZERO, TextSize::new(4))
                );

                assert_eq!(list.full_text(), "a, b");
                assert!(!list.is_recovered());

                assert_eq!(
                    list.tokens().collect::<Vec<_>>(),
                    [first_token, separator.clone(), second_token]
                );

                assert_eq!(
                    list.$separator_tokens_method().collect::<Vec<_>>(),
                    [separator]
                );

                assert_eq!(list.$items_method().collect::<Vec<_>>(), [first, second]);
            }

            #[test]
            fn preserves_missing_separators() {
                let snapshot = snapshot("a b");

                let first_token = token(0, 1).with_trailing_trivia([SyntaxTrivia::whitespace(
                    TextRange::new(TextSize::new(1), TextSize::new(2)),
                )]);

                let missing_separator = SyntaxToken::missing($separator_kind, TextSize::new(2));
                let second_token = token(2, 3);

                let list = $list_syntax::builder(snapshot.clone(), TextSize::ZERO)
                    .item(item(&snapshot, first_token))
                    .separator_token(missing_separator.clone())
                    .item(item(&snapshot, second_token))
                    .build();

                let separators = list.$separator_tokens_method().collect::<Vec<_>>();

                assert_eq!(list.full_text(), "a b");
                assert!(list.is_recovered());
                assert_eq!(separators, [missing_separator]);
                assert!(separators[0].is_missing());
            }

            #[test]
            fn attaches_skipped_syntax_without_losing_source_text() {
                let snapshot = snapshot("a$ b");

                let skipped_token =
                    SyntaxToken::invalid(TextRange::new(TextSize::new(1), TextSize::new(2)))
                        .with_trailing_trivia([SyntaxTrivia::whitespace(TextRange::new(
                            TextSize::new(2),
                            TextSize::new(3),
                        ))]);

                let list = $list_syntax::builder(snapshot.clone(), TextSize::ZERO)
                    .item(item(&snapshot, token(0, 1)))
                    .skipped_tokens([skipped_token.clone()])
                    .item(item(&snapshot, token(3, 4)))
                    .build();

                let skipped_syntax = list.skipped_syntax().collect::<Vec<_>>();

                let [skipped] = skipped_syntax.as_slice() else {
                    panic!("expected one skipped-syntax node: {skipped_syntax:?}");
                };

                assert_eq!(list.full_text(), "a$ b");
                assert!(list.is_recovered());
                assert_eq!(skipped.full_text(), "$ ");
                assert_eq!(skipped.tokens().collect::<Vec<_>>(), [skipped_token]);
            }

            #[test]
            fn items_expose_their_named_token_slot() {
                let snapshot = snapshot("main");
                let token = token(0, 4);
                let item = item(&snapshot, token.clone());

                assert_eq!(item.kind(), $item_kind);
                assert_eq!(item.full_text(), "main");
                assert!(!item.is_recovered());
                assert_eq!(item.$token_method(), token);
                assert_eq!(item.tokens().collect::<Vec<_>>(), [token]);
            }

            #[test]
            fn items_detect_missing_token_slots() {
                let snapshot = snapshot("");
                let token = SyntaxToken::missing($token_kind, TextSize::ZERO);
                let item = item(&snapshot, token.clone());

                assert_eq!(item.$token_method(), token);
                assert!(item.is_recovered());
                assert_eq!(item.full_text(), "");
            }

            #[test]
            fn typed_nodes_implement_syntax_node_contract() {
                let snapshot = snapshot("a");
                let item = item(&snapshot, token(0, 1));

                let list = $list_syntax::builder(snapshot, TextSize::ZERO)
                    .item(item.clone())
                    .build();

                assert_node_kind(&item, $item_kind);
                assert_node_kind(&list, $list_kind);

                assert_source_node_source(&item, "a");
                assert_source_node_source(&list, "a");

                assert_source_node_text(&item, "a");
                assert_source_node_text(&list, "a");
            }

            fn item(snapshot: &SourceSnapshot, token: SyntaxToken) -> $item_syntax {
                $item_syntax::builder(snapshot.clone())
                    .$token_method(token)
                    .build()
            }

            fn token(start: u32, end: u32) -> SyntaxToken {
                SyntaxToken::new(
                    $token_kind,
                    TextRange::new(TextSize::new(start), TextSize::new(end)),
                )
            }

            fn assert_node_kind(node: &impl SyntaxNode, kind: $crate::SyntaxKind) {
                assert_eq!(node.kind(), kind);
            }

            fn assert_source_node_text(node: &impl SourceSyntaxNode, text: &str) {
                assert_eq!(node.full_text_from(text), text);
            }

            fn assert_source_node_source(node: &impl SourceSyntaxNode, text: &str) {
                assert_eq!(node.source().text(), text);
            }

            fn snapshot(text: &str) -> SourceSnapshot {
                match SourceSnapshot::new(
                    SourceId::new(0),
                    SourceIdentity::new(0),
                    SourceOrigin::virtual_source($source_name),
                    SourceVersion::new(0),
                    text,
                ) {
                    Ok(snapshot) => snapshot,
                    Err(error) => panic!("test source should fit in TextSize: {error:?}"),
                }
            }
        }
    };

    (
        mod $module:ident {
            list: $list_syntax:ident,
            item: $item_syntax:ident,
            list_kind: $list_kind:path,
            item_kind: $item_kind:path,
            token_kind: $token_kind:path,
            token: $token_method:ident,
            separator: none,
            items: $items_method:ident,
            source_name: $source_name:literal $(,)?
        }
    ) => {
        mod $module {
            use bray_source::{
                SourceId, SourceIdentity, SourceOrigin, SourceSnapshot, SourceVersion, TextRange,
                TextSize,
            };

            use super::{$item_syntax, $list_syntax};
            use $crate::{SourceSyntaxNode, SyntaxNode, SyntaxText, SyntaxToken, SyntaxTrivia};

            #[test]
            fn stores_items_and_source_text() {
                let snapshot = snapshot("@test @target");

                let first_token = token(0, 5).with_trailing_trivia([SyntaxTrivia::whitespace(
                    TextRange::new(TextSize::new(5), TextSize::new(6)),
                )]);

                let second_token = token(6, 13);
                let first = item(&snapshot, first_token.clone());
                let second = item(&snapshot, second_token.clone());

                let list = $list_syntax::builder(snapshot.clone(), TextSize::ZERO)
                    .item(first.clone())
                    .item(second.clone())
                    .build();

                assert_eq!(list.kind(), $list_kind);

                assert_eq!(
                    list.full_range(),
                    TextRange::new(TextSize::ZERO, TextSize::new(13))
                );

                assert_eq!(list.full_text(), "@test @target");
                assert!(!list.is_recovered());

                assert_eq!(
                    list.tokens().collect::<Vec<_>>(),
                    [first_token, second_token]
                );

                assert_eq!(list.$items_method().collect::<Vec<_>>(), [first, second]);
            }

            #[test]
            fn attaches_skipped_syntax_without_losing_source_text() {
                let snapshot = snapshot("@test $ @target");

                let skipped_token =
                    SyntaxToken::invalid(TextRange::new(TextSize::new(6), TextSize::new(7)))
                        .with_trailing_trivia([SyntaxTrivia::whitespace(TextRange::new(
                            TextSize::new(7),
                            TextSize::new(8),
                        ))]);

                let list = $list_syntax::builder(snapshot.clone(), TextSize::ZERO)
                    .item(item(
                        &snapshot,
                        token(0, 5).with_trailing_trivia([SyntaxTrivia::whitespace(
                            TextRange::new(TextSize::new(5), TextSize::new(6)),
                        )]),
                    ))
                    .skipped_tokens([skipped_token.clone()])
                    .item(item(&snapshot, token(8, 15)))
                    .build();

                let skipped_syntax = list.skipped_syntax().collect::<Vec<_>>();

                let [skipped] = skipped_syntax.as_slice() else {
                    panic!("expected one skipped-syntax node: {skipped_syntax:?}");
                };

                assert_eq!(list.full_text(), "@test $ @target");
                assert!(list.is_recovered());
                assert_eq!(skipped.full_text(), "$ ");
                assert_eq!(skipped.tokens().collect::<Vec<_>>(), [skipped_token]);
            }

            #[test]
            fn items_expose_their_named_token_slot() {
                let snapshot = snapshot("@test");
                let token = token(0, 5);
                let item = item(&snapshot, token.clone());

                assert_eq!(item.kind(), $item_kind);
                assert_eq!(item.full_text(), "@test");
                assert!(!item.is_recovered());
                assert_eq!(item.$token_method(), token);
                assert_eq!(item.tokens().collect::<Vec<_>>(), [token]);
            }

            #[test]
            fn items_detect_missing_token_slots() {
                let snapshot = snapshot("");
                let token = SyntaxToken::missing($token_kind, TextSize::ZERO);
                let item = item(&snapshot, token.clone());

                assert_eq!(item.$token_method(), token);
                assert!(item.is_recovered());
                assert_eq!(item.full_text(), "");
            }

            #[test]
            fn typed_nodes_implement_syntax_node_contract() {
                let snapshot = snapshot("@test");
                let item = item(&snapshot, token(0, 5));

                let list = $list_syntax::builder(snapshot.clone(), TextSize::ZERO)
                    .item(item.clone())
                    .build();
                let roundtrip =
                    $list_syntax::from_green(snapshot, list.clone().into_green(), TextSize::ZERO);

                assert_node_kind(&item, $item_kind);
                assert_node_kind(&list, $list_kind);
                assert_node_kind(&roundtrip, $list_kind);

                assert_source_node_source(&item, "@test");
                assert_source_node_source(&list, "@test");
                assert_source_node_source(&roundtrip, "@test");

                assert_source_node_text(&item, "@test");
                assert_source_node_text(&list, "@test");
                assert_source_node_text(&roundtrip, "@test");
            }

            fn item(snapshot: &SourceSnapshot, token: SyntaxToken) -> $item_syntax {
                $item_syntax::builder(snapshot.clone())
                    .$token_method(token)
                    .build()
            }

            fn token(start: u32, end: u32) -> SyntaxToken {
                SyntaxToken::new(
                    $token_kind,
                    TextRange::new(TextSize::new(start), TextSize::new(end)),
                )
            }

            fn assert_node_kind(node: &impl SyntaxNode, kind: $crate::SyntaxKind) {
                assert_eq!(node.kind(), kind);
            }

            fn assert_source_node_text(node: &impl SourceSyntaxNode, text: &str) {
                assert_eq!(node.full_text_from(text), text);
            }

            fn assert_source_node_source(node: &impl SourceSyntaxNode, text: &str) {
                assert_eq!(node.source().text(), text);
            }

            fn snapshot(text: &str) -> SourceSnapshot {
                match SourceSnapshot::new(
                    SourceId::new(0),
                    SourceIdentity::new(0),
                    SourceOrigin::virtual_source($source_name),
                    SourceVersion::new(0),
                    text,
                ) {
                    Ok(snapshot) => snapshot,
                    Err(error) => panic!("test source should fit in TextSize: {error:?}"),
                }
            }
        }
    };
}

pub(crate) use test_token_list_syntax;
