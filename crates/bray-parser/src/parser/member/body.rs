use bray_syntax::{
    InherentImplementationBodySyntaxBuilder, StructBodySyntaxBuilder, SyntaxKind,
    TraitBodySyntaxBuilder, TraitImplementationBodySyntaxBuilder, UnionBodySyntaxBuilder,
};

use crate::cursor::RecoverySet;

use super::callable::TraitCallableTailPolicy;
use crate::parser::recovery::RecoverySyntaxSink;
use crate::parser::state::Parser;

const STRUCT_BODY_ITEM_RECOVERY_KINDS: [SyntaxKind; 9] = [
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::AsyncKeyword,
    SyntaxKind::TrustedKeyword,
    SyntaxKind::StaticKeyword,
    SyntaxKind::ConsumeKeyword,
    SyntaxKind::MutKeyword,
    SyntaxKind::ConstKeyword,
    SyntaxKind::FuncKeyword,
];

pub(super) const UNION_BODY_ITEM_RECOVERY_KINDS: [SyntaxKind; 11] = [
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
    SyntaxKind::IdentifierToken,
];

const TRAIT_BODY_ITEM_RECOVERY_KINDS: [SyntaxKind; 7] = [
    SyntaxKind::AsyncKeyword,
    SyntaxKind::TrustedKeyword,
    SyntaxKind::StaticKeyword,
    SyntaxKind::ConsumeKeyword,
    SyntaxKind::MutKeyword,
    SyntaxKind::ConstKeyword,
    SyntaxKind::FuncKeyword,
];

const IMPLEMENTATION_BODY_ITEM_RECOVERY_KINDS: [SyntaxKind; 9] = [
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::AsyncKeyword,
    SyntaxKind::TrustedKeyword,
    SyntaxKind::StaticKeyword,
    SyntaxKind::ConsumeKeyword,
    SyntaxKind::MutKeyword,
    SyntaxKind::ConstKeyword,
    SyntaxKind::FuncKeyword,
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

    pub(in crate::parser) fn parse_inherent_implementation_body_items(
        &mut self,
        builder: &mut InherentImplementationBodySyntaxBuilder,
    ) {
        self.parse_member_body_items(builder, Parser::parse_inherent_implementation_body_item);
    }

    pub(in crate::parser) fn parse_trait_implementation_body_items(
        &mut self,
        builder: &mut TraitImplementationBodySyntaxBuilder,
    ) {
        self.parse_member_body_items(builder, Parser::parse_trait_implementation_body_item);
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

        if self.should_parse_constant_declaration() {
            builder.push_constant_declaration(self.parse_constant_declaration());
            return;
        }

        if self.should_parse_struct_field_declaration() {
            builder.push_struct_field_declaration(self.parse_struct_field_declaration());
            return;
        }

        // TODO(parser): Parse remaining struct body items as they are implemented.
        self.recover_body_item(builder, &STRUCT_BODY_ITEM_RECOVERY_KINDS);
    }

    fn parse_union_body_item(&mut self, builder: &mut UnionBodySyntaxBuilder) {
        if self.should_parse_type_callable_member_declaration() {
            builder.push_type_callable_member_declaration(
                self.parse_type_callable_member_declaration(),
            );
            return;
        }

        if self.should_parse_constant_declaration() {
            builder.push_constant_declaration(self.parse_constant_declaration());
            return;
        }

        if self.should_parse_union_variant_declaration() {
            builder.push_union_variant_declaration(self.parse_union_variant_declaration());
            return;
        }

        // TODO(parser): Parse remaining union body items as they are implemented.
        self.recover_body_item(builder, &UNION_BODY_ITEM_RECOVERY_KINDS);
    }

    fn parse_trait_body_item(&mut self, builder: &mut TraitBodySyntaxBuilder) {
        if self.should_parse_trait_callable_member_declaration() {
            builder.push_trait_callable_member_declaration(
                self.parse_trait_callable_member_declaration(
                    TraitCallableTailPolicy::AllowSemicolon,
                ),
            );
            return;
        }

        if self.should_parse_trait_constant_member_declaration() {
            builder.push_trait_constant_member_declaration(
                self.parse_trait_constant_member_declaration(),
            );
            return;
        }

        // TODO(parser): Parse remaining trait member declarations as they are implemented.
        self.recover_body_item(builder, &TRAIT_BODY_ITEM_RECOVERY_KINDS);
    }

    fn parse_inherent_implementation_body_item(
        &mut self,
        builder: &mut InherentImplementationBodySyntaxBuilder,
    ) {
        if self.should_parse_type_callable_member_declaration() {
            builder.push_type_callable_member_declaration(
                self.parse_type_callable_member_declaration(),
            );
            return;
        }

        if self.should_parse_constant_declaration() {
            builder.push_constant_declaration(self.parse_constant_declaration());
            return;
        }

        // TODO(parser): Parse remaining inherent implementation members as they are implemented.
        self.recover_body_item(builder, &IMPLEMENTATION_BODY_ITEM_RECOVERY_KINDS);
    }

    fn parse_trait_implementation_body_item(
        &mut self,
        builder: &mut TraitImplementationBodySyntaxBuilder,
    ) {
        if self.should_parse_trait_callable_member_declaration() {
            builder.push_trait_callable_member_declaration(
                self.parse_trait_callable_member_declaration(TraitCallableTailPolicy::RequireBody),
            );
            return;
        }

        if self.should_parse_trait_implementation_constant_member_definition() {
            builder.push_trait_implementation_constant_member_definition(
                self.parse_trait_implementation_constant_member_definition(),
            );
            return;
        }

        // TODO(parser): Parse remaining trait implementation members as they are implemented.
        self.recover_body_item(builder, &IMPLEMENTATION_BODY_ITEM_RECOVERY_KINDS);
    }

    fn recover_body_item(
        &mut self,
        builder: &mut impl RecoverySyntaxSink,
        recovery_kinds: &[SyntaxKind],
    ) {
        self.recover_current_and_until_balanced_close_brace_or_recovery_set(
            builder,
            RecoverySet::new(recovery_kinds),
        );
    }
}
