use bray_syntax::{
    CallableOverloadDeclarationSyntax, ConstantDeclarationSyntax, ImplementationBodySyntaxBuilder,
    ImplementationTypeMemberBindingSyntax, PredicateDeclarationSyntax, StructBodySyntaxBuilder,
    StructFieldDeclarationSyntax, SyntaxKind, TraitBodySyntaxBuilder,
    TraitCallableMemberDeclarationSyntax, TraitConstantMemberDeclarationSyntax,
    TraitPredicateMemberDeclarationSyntax, TraitTypeMemberDeclarationSyntax,
    TypeCallableMemberDeclarationSyntax, TypeConstructorMemberDeclarationSyntax,
    UnionBodySyntaxBuilder, UnionVariantDeclarationSyntax,
};

use crate::cursor::RecoverySet;

use crate::parser::recovery::RecoverySyntaxSink;
use crate::parser::state::Parser;

use super::lifecycle::{TraitLifecycleRequirementSyntaxSink, TypeLifecycleMemberSyntaxSink};

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

    pub(in crate::parser) fn parse_struct_body_item(
        &mut self,
        builder: &mut impl StructBodyItemSyntaxSink,
    ) {
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

    pub(in crate::parser) fn parse_union_body_item(
        &mut self,
        builder: &mut impl UnionBodyItemSyntaxSink,
    ) {
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

    pub(in crate::parser) fn parse_trait_body_item(
        &mut self,
        builder: &mut impl TraitBodyItemSyntaxSink,
    ) {
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

        if self.should_parse_trait_predicate_member_declaration() {
            builder.push_trait_predicate_member_declaration(
                self.parse_trait_predicate_member_declaration(),
            );
            return;
        }

        // Recover syntax that is not valid in a trait member position.
        self.recover_body_item(builder);
    }

    pub(in crate::parser) fn parse_implementation_body_item(
        &mut self,
        builder: &mut impl ImplementationBodyItemSyntaxSink,
    ) {
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

pub(in crate::parser) trait SharedTypeMemberSyntaxSink:
    TypeLifecycleMemberSyntaxSink
{
    fn push_type_callable_member_declaration(
        &mut self,
        declaration: TypeCallableMemberDeclarationSyntax,
    );

    fn push_type_constructor_member_declaration(
        &mut self,
        declaration: TypeConstructorMemberDeclarationSyntax,
    );

    fn push_constant_declaration(&mut self, declaration: ConstantDeclarationSyntax);

    fn push_predicate_declaration(&mut self, declaration: PredicateDeclarationSyntax);

    fn push_callable_overload_declaration(
        &mut self,
        declaration: CallableOverloadDeclarationSyntax,
    );
}

pub(in crate::parser) trait StructBodyItemSyntaxSink:
    SharedTypeMemberSyntaxSink
{
    fn push_struct_field_declaration(&mut self, declaration: StructFieldDeclarationSyntax);
}

pub(in crate::parser) trait UnionBodyItemSyntaxSink:
    SharedTypeMemberSyntaxSink
{
    fn push_union_variant_declaration(&mut self, declaration: UnionVariantDeclarationSyntax);
}

pub(in crate::parser) trait TraitBodyItemSyntaxSink:
    TraitLifecycleRequirementSyntaxSink
{
    fn push_trait_callable_member_declaration(
        &mut self,
        declaration: TraitCallableMemberDeclarationSyntax,
    );

    fn push_trait_constant_member_declaration(
        &mut self,
        declaration: TraitConstantMemberDeclarationSyntax,
    );

    fn push_trait_type_member_declaration(&mut self, declaration: TraitTypeMemberDeclarationSyntax);

    fn push_trait_predicate_member_declaration(
        &mut self,
        declaration: TraitPredicateMemberDeclarationSyntax,
    );
}

pub(in crate::parser) trait ImplementationBodyItemSyntaxSink:
    SharedTypeMemberSyntaxSink
{
    fn push_implementation_type_member_binding(
        &mut self,
        binding: ImplementationTypeMemberBindingSyntax,
    );
}

macro_rules! impl_shared_type_member_syntax_sink {
    ($builder:ty) => {
        impl SharedTypeMemberSyntaxSink for $builder {
            fn push_type_callable_member_declaration(
                &mut self,
                declaration: TypeCallableMemberDeclarationSyntax,
            ) {
                <$builder>::push_type_callable_member_declaration(self, declaration);
            }

            fn push_type_constructor_member_declaration(
                &mut self,
                declaration: TypeConstructorMemberDeclarationSyntax,
            ) {
                <$builder>::push_type_constructor_member_declaration(self, declaration);
            }

            fn push_constant_declaration(&mut self, declaration: ConstantDeclarationSyntax) {
                <$builder>::push_constant_declaration(self, declaration);
            }

            fn push_predicate_declaration(&mut self, declaration: PredicateDeclarationSyntax) {
                <$builder>::push_predicate_declaration(self, declaration);
            }

            fn push_callable_overload_declaration(
                &mut self,
                declaration: CallableOverloadDeclarationSyntax,
            ) {
                <$builder>::push_callable_overload_declaration(self, declaration);
            }
        }
    };
}

impl_shared_type_member_syntax_sink!(StructBodySyntaxBuilder);
impl_shared_type_member_syntax_sink!(UnionBodySyntaxBuilder);
impl_shared_type_member_syntax_sink!(ImplementationBodySyntaxBuilder);

impl StructBodyItemSyntaxSink for StructBodySyntaxBuilder {
    fn push_struct_field_declaration(&mut self, declaration: StructFieldDeclarationSyntax) {
        StructBodySyntaxBuilder::push_struct_field_declaration(self, declaration);
    }
}

impl UnionBodyItemSyntaxSink for UnionBodySyntaxBuilder {
    fn push_union_variant_declaration(&mut self, declaration: UnionVariantDeclarationSyntax) {
        UnionBodySyntaxBuilder::push_union_variant_declaration(self, declaration);
    }
}

impl TraitBodyItemSyntaxSink for TraitBodySyntaxBuilder {
    fn push_trait_callable_member_declaration(
        &mut self,
        declaration: TraitCallableMemberDeclarationSyntax,
    ) {
        TraitBodySyntaxBuilder::push_trait_callable_member_declaration(self, declaration);
    }

    fn push_trait_constant_member_declaration(
        &mut self,
        declaration: TraitConstantMemberDeclarationSyntax,
    ) {
        TraitBodySyntaxBuilder::push_trait_constant_member_declaration(self, declaration);
    }

    fn push_trait_type_member_declaration(
        &mut self,
        declaration: TraitTypeMemberDeclarationSyntax,
    ) {
        TraitBodySyntaxBuilder::push_trait_type_member_declaration(self, declaration);
    }

    fn push_trait_predicate_member_declaration(
        &mut self,
        declaration: TraitPredicateMemberDeclarationSyntax,
    ) {
        TraitBodySyntaxBuilder::push_trait_predicate_member_declaration(self, declaration);
    }
}

impl ImplementationBodyItemSyntaxSink for ImplementationBodySyntaxBuilder {
    fn push_implementation_type_member_binding(
        &mut self,
        binding: ImplementationTypeMemberBindingSyntax,
    ) {
        ImplementationBodySyntaxBuilder::push_implementation_type_member_binding(self, binding);
    }
}
