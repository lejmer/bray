use bray_syntax::{
    ImplementationBodySyntaxBuilder, StructBodySyntaxBuilder, SyntaxKind, TraitBodySyntaxBuilder,
    UnionBodySyntaxBuilder,
};

use crate::cursor::RecoverySet;

use crate::parser::recovery::RecoverySyntaxSink;
use crate::parser::state::Parser;

pub(in crate::parser) const MEMBER_KEYWORD_RECOVERY_KINDS: [SyntaxKind; 19] = [
    SyntaxKind::AtToken,
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::AsyncKeyword,
    SyntaxKind::TrustedKeyword,
    SyntaxKind::StaticKeyword,
    SyntaxKind::ConsumeKeyword,
    SyntaxKind::MutKeyword,
    SyntaxKind::ConstKeyword,
    SyntaxKind::FuncKeyword,
    SyntaxKind::ConstructKeyword,
    SyntaxKind::FinalizeKeyword,
    SyntaxKind::DestructKeyword,
    SyntaxKind::EnterKeyword,
    SyntaxKind::ExitKeyword,
    SyntaxKind::TypeKeyword,
    SyntaxKind::PredicateKeyword,
    SyntaxKind::OverloadKeyword,
    SyntaxKind::EndOfFileToken,
];

pub(super) const MEMBER_ITEM_RECOVERY_KINDS: [SyntaxKind; 19] = [
    SyntaxKind::AtToken,
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::AsyncKeyword,
    SyntaxKind::TrustedKeyword,
    SyntaxKind::StaticKeyword,
    SyntaxKind::ConsumeKeyword,
    SyntaxKind::MutKeyword,
    SyntaxKind::ConstKeyword,
    SyntaxKind::FuncKeyword,
    SyntaxKind::ConstructKeyword,
    SyntaxKind::FinalizeKeyword,
    SyntaxKind::DestructKeyword,
    SyntaxKind::EnterKeyword,
    SyntaxKind::ExitKeyword,
    SyntaxKind::TypeKeyword,
    SyntaxKind::PredicateKeyword,
    SyntaxKind::OverloadKeyword,
    SyntaxKind::IdentifierToken,
];

impl Parser {
    pub(in crate::parser) fn parse_struct_body_items(
        &mut self,
        builder: &mut StructBodySyntaxBuilder,
    ) {
        self.parse_member_body_items(builder, Parser::parse_struct_body_item);
    }

    pub(in crate::parser) fn parse_union_body_items(
        &mut self,
        builder: &mut UnionBodySyntaxBuilder,
    ) {
        self.parse_member_body_items(builder, Parser::parse_union_body_item);
    }

    pub(in crate::parser) fn parse_trait_body_items(
        &mut self,
        builder: &mut TraitBodySyntaxBuilder,
    ) {
        self.parse_member_body_items(builder, Parser::parse_trait_body_item);
    }

    pub(in crate::parser) fn parse_implementation_body_items(
        &mut self,
        builder: &mut ImplementationBodySyntaxBuilder,
    ) {
        self.parse_member_body_items(builder, Parser::parse_implementation_body_item);
    }

    fn parse_member_body_items<Builder>(
        &mut self,
        builder: &mut Builder,
        mut parse_item: impl FnMut(&mut Parser, &mut Builder),
    ) {
        while !self.at(SyntaxKind::CloseBraceToken) && !self.at(SyntaxKind::EndOfFileToken) {
            parse_item(self, builder);
        }
    }

    fn parse_struct_body_item(&mut self, builder: &mut StructBodySyntaxBuilder) {
        if self.should_parse_type_callable_member_declaration() {
            builder.push_type_callable_member_declaration(
                self.parse_type_callable_member_declaration(),
            );
            return;
        }

        if self.should_parse_type_constructor_member_declaration() {
            builder.push_type_constructor_member_declaration(
                self.parse_type_constructor_member_declaration(),
            );
            return;
        }

        if self.should_parse_type_lifecycle_member_declaration() {
            self.parse_type_lifecycle_member_declaration(builder);
            return;
        }

        if self.should_parse_constant_declaration() {
            builder.push_constant_declaration(self.parse_constant_declaration());
            return;
        }

        if self.should_parse_predicate_declaration() {
            builder.push_predicate_declaration(self.parse_predicate_declaration());
            return;
        }

        if self.should_parse_callable_overload_declaration() {
            builder.push_callable_overload_declaration(self.parse_callable_overload_declaration());
            return;
        }

        if self.should_parse_struct_field_declaration() {
            builder.push_struct_field_declaration(self.parse_struct_field_declaration());
            return;
        }

        // Recover syntax that is not valid in a struct body item position.
        self.recover_body_item(builder);
    }

    fn parse_union_body_item(&mut self, builder: &mut UnionBodySyntaxBuilder) {
        if self.should_parse_type_callable_member_declaration() {
            builder.push_type_callable_member_declaration(
                self.parse_type_callable_member_declaration(),
            );
            return;
        }

        if self.should_parse_type_constructor_member_declaration() {
            builder.push_type_constructor_member_declaration(
                self.parse_type_constructor_member_declaration(),
            );
            return;
        }

        if self.should_parse_type_lifecycle_member_declaration() {
            self.parse_type_lifecycle_member_declaration(builder);
            return;
        }

        if self.should_parse_constant_declaration() {
            builder.push_constant_declaration(self.parse_constant_declaration());
            return;
        }

        if self.should_parse_predicate_declaration() {
            builder.push_predicate_declaration(self.parse_predicate_declaration());
            return;
        }

        if self.should_parse_callable_overload_declaration() {
            builder.push_callable_overload_declaration(self.parse_callable_overload_declaration());
            return;
        }

        if self.should_parse_union_variant_declaration() {
            builder.push_union_variant_declaration(self.parse_union_variant_declaration());
            return;
        }

        // Recover syntax that is not valid in a union body item position.
        self.recover_body_item(builder);
    }

    fn parse_trait_body_item(&mut self, builder: &mut TraitBodySyntaxBuilder) {
        if self.should_parse_trait_callable_member_declaration() {
            builder.push_trait_callable_member_declaration(
                self.parse_trait_callable_member_declaration(),
            );
            return;
        }

        if self.should_parse_trait_lifecycle_requirement_declaration() {
            self.parse_trait_lifecycle_requirement_declaration(builder);
            return;
        }

        if self.should_parse_trait_constant_member_declaration() {
            builder.push_trait_constant_member_declaration(
                self.parse_trait_constant_member_declaration(),
            );
            return;
        }

        if self.should_parse_trait_type_member_declaration() {
            builder.push_trait_type_member_declaration(self.parse_trait_type_member_declaration());
            return;
        }

        // TODO(parser): Parse remaining trait member declarations as they are implemented.
        self.recover_body_item(builder);
    }

    fn parse_implementation_body_item(&mut self, builder: &mut ImplementationBodySyntaxBuilder) {
        if self.should_parse_type_callable_member_declaration() {
            builder.push_type_callable_member_declaration(
                self.parse_type_callable_member_declaration(),
            );
            return;
        }

        if self.should_parse_type_constructor_member_declaration() {
            builder.push_type_constructor_member_declaration(
                self.parse_type_constructor_member_declaration(),
            );
            return;
        }

        if self.should_parse_type_lifecycle_member_declaration() {
            self.parse_type_lifecycle_member_declaration(builder);
            return;
        }

        if self.should_parse_implementation_type_member_binding() {
            builder.push_implementation_type_member_binding(
                self.parse_implementation_type_member_binding(),
            );
            return;
        }

        if self.should_parse_constant_declaration() {
            builder.push_constant_declaration(self.parse_constant_declaration());
            return;
        }

        if self.should_parse_predicate_declaration() {
            builder.push_predicate_declaration(self.parse_predicate_declaration());
            return;
        }

        if self.should_parse_callable_overload_declaration() {
            builder.push_callable_overload_declaration(self.parse_callable_overload_declaration());
            return;
        }

        // Recover syntax that is not valid in an implementation member position.
        self.recover_body_item(builder);
    }

    fn recover_body_item(&mut self, builder: &mut impl RecoverySyntaxSink) {
        self.recover_current_and_until_balanced_close_brace_or_recovery_set(
            builder,
            RecoverySet::new(&MEMBER_ITEM_RECOVERY_KINDS),
        );
    }
}
