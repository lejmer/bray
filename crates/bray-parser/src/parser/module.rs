use bray_syntax::{
    BlockModuleDeclarationSyntax, BlockModuleDeclarationSyntaxBuilder,
    CallableContractDeclarationSyntax, CallableOverloadDeclarationSyntax,
    ConstantDeclarationSyntax, ExportDeclarationSyntax, FunctionDeclarationSyntax,
    ImplementationOverloadDeclarationSyntax, InherentImplementationDeclarationSyntax,
    ModuleBodySyntax, ModuleBodySyntaxBuilder, ModuleDirectivesSyntax,
    ModuleDirectivesSyntaxBuilder, ModuleModifiersSyntax,
    NamedTraitImplementationDeclarationSyntax, PathSyntax, PredicateDeclarationSyntax,
    SourceUnitModuleDeclarationSyntax, SourceUnitModuleDeclarationSyntaxBuilder,
    SourceUnitSyntaxBuilder, StaticDeclarationSyntax, StructDeclarationSyntax, SyntaxKind,
    SyntaxToken, TraitDeclarationSyntax, UnionDeclarationSyntax,
    UnnamedTraitImplementationDeclarationSyntax, UsingDeclarationSyntax,
};

use crate::cursor::RecoverySet;

use super::directive::DirectiveScanKind;
use super::entry::DeclarationFragmentSyntax;
use super::recovery::RecoverySyntaxSink;
use super::state::Parser;

const MODULE_DECLARATION_START_KINDS: [SyntaxKind; 5] = [
    SyntaxKind::AtToken,
    SyntaxKind::TrustedKeyword,
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::ModuleKeyword,
];

const MODULE_HEADER_START_KINDS: [SyntaxKind; 4] = [
    SyntaxKind::TrustedKeyword,
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::ModuleKeyword,
];

const TOP_LEVEL_BLOCK_MODULE_RECOVERY_KINDS: [SyntaxKind; 6] = [
    SyntaxKind::AtToken,
    SyntaxKind::TrustedKeyword,
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::ModuleKeyword,
    SyntaxKind::EndOfFileToken,
];

const DIRECTIVE_ARGUMENT_RECOVERY_KINDS: [SyntaxKind; 5] = [
    SyntaxKind::AtToken,
    SyntaxKind::TrustedKeyword,
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::ModuleKeyword,
];

const PARSED_MODULE_ITEM_BOUNDARY_KINDS: [SyntaxKind; 21] = [
    SyntaxKind::UsingKeyword,
    SyntaxKind::ExportKeyword,
    SyntaxKind::AtToken,
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::TrustedKeyword,
    SyntaxKind::ExternKeyword,
    SyntaxKind::AsyncKeyword,
    SyntaxKind::ConstKeyword,
    SyntaxKind::StaticKeyword,
    SyntaxKind::CallableKeyword,
    SyntaxKind::FuncKeyword,
    SyntaxKind::StructKeyword,
    SyntaxKind::UnionKeyword,
    SyntaxKind::TraitKeyword,
    SyntaxKind::ImplKeyword,
    SyntaxKind::OverloadKeyword,
    SyntaxKind::PredicateKeyword,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

pub(super) const MODULE_ITEM_DECLARATION_TERMINATOR_KINDS: [SyntaxKind; 3] = [
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

pub(super) const MODULE_ITEM_START_KINDS: [SyntaxKind; 19] = [
    SyntaxKind::UsingKeyword,
    SyntaxKind::ExportKeyword,
    SyntaxKind::AtToken,
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::TrustedKeyword,
    SyntaxKind::ExternKeyword,
    SyntaxKind::AsyncKeyword,
    SyntaxKind::ConstKeyword,
    SyntaxKind::StaticKeyword,
    SyntaxKind::CallableKeyword,
    SyntaxKind::FuncKeyword,
    SyntaxKind::StructKeyword,
    SyntaxKind::UnionKeyword,
    SyntaxKind::TraitKeyword,
    SyntaxKind::ImplKeyword,
    SyntaxKind::OverloadKeyword,
    SyntaxKind::PredicateKeyword,
    SyntaxKind::ModuleKeyword,
];

impl Parser {
    pub(super) fn parse_source_unit_module_declaration(
        &mut self,
    ) -> SourceUnitModuleDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = SourceUnitModuleDeclarationSyntax::builder(self.syntax_source(), start);

        self.recover_until(&mut builder, &MODULE_DECLARATION_START_KINDS);
        builder.push_module_directives(self.parse_module_directives());
        self.recover_until(&mut builder, &MODULE_HEADER_START_KINDS);
        self.parse_module_declaration_header_after_directives(&mut builder);
        self.recover_until_module_item_declaration_end(&mut builder);

        builder.push_semicolon_token(self.expect(SyntaxKind::SemicolonToken));

        builder.build()
    }

    pub(super) fn parse_block_module_declarations(
        &mut self,
        builder: &mut SourceUnitSyntaxBuilder,
    ) {
        loop {
            if self.at(SyntaxKind::EndOfFileToken) {
                return;
            }

            if self.should_parse_block_module_declaration() {
                let declaration = self.parse_block_module_declaration();

                builder.push_block_module_declaration(declaration);
                continue;
            }

            if self.recover_until(builder, &TOP_LEVEL_BLOCK_MODULE_RECOVERY_KINDS) {
                continue;
            }

            self.recover_current_token(builder);
        }
    }

    fn parse_block_module_declaration(&mut self) -> BlockModuleDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = BlockModuleDeclarationSyntax::builder(self.syntax_source(), start);

        self.parse_module_declaration_header(&mut builder);
        builder.push_module_body(self.parse_module_body());

        builder.build()
    }

    fn parse_module_declaration_header(&mut self, builder: &mut impl ModuleDeclarationSyntaxSink) {
        builder.push_module_directives(self.parse_module_directives());
        self.parse_module_declaration_header_after_directives(builder);
    }

    fn parse_module_declaration_header_after_directives(
        &mut self,
        builder: &mut impl ModuleDeclarationSyntaxSink,
    ) {
        builder.push_module_modifiers(self.parse_module_modifiers());
        builder.push_module_keyword(self.expect(SyntaxKind::ModuleKeyword));
        builder.push_module_path(self.parse_module_path());
    }

    fn parse_module_directives(&mut self) -> ModuleDirectivesSyntax {
        let start = self.peek().full_range().start();
        let mut builder = ModuleDirectivesSyntax::builder(self.syntax_source(), start);

        while self.at(SyntaxKind::AtToken) {
            if self.at_directive_kind(SyntaxKind::TargetDirective) {
                builder.push_target_directive(
                    self.parse_target_directive(&DIRECTIVE_ARGUMENT_RECOVERY_KINDS),
                );

                continue;
            }

            if self.at_directive_kind(SyntaxKind::TestDirective) {
                builder.push_test_directive(
                    self.parse_test_directive(&DIRECTIVE_ARGUMENT_RECOVERY_KINDS),
                );

                continue;
            }

            if self.at_directive_kind(SyntaxKind::LinkDirective) {
                builder.push_link_directive(
                    self.parse_link_directive(&DIRECTIVE_ARGUMENT_RECOVERY_KINDS),
                );

                continue;
            }

            self.recover_unknown_module_directive(&mut builder);
        }

        builder.build()
    }

    fn recover_unknown_module_directive(&mut self, builder: &mut ModuleDirectivesSyntaxBuilder) {
        self.recover_unsupported_directive(builder, &MODULE_DECLARATION_START_KINDS);
    }

    fn parse_module_modifiers(&mut self) -> ModuleModifiersSyntax {
        let start = self.peek().full_range().start();
        let mut builder = ModuleModifiersSyntax::builder(self.syntax_source(), start);

        if self.at(SyntaxKind::TrustedKeyword) {
            builder.push_trusted_token(self.parse_trusted_modifier());
        }

        if self.at_visibility_modifier() {
            builder.push_visibility_token(self.parse_visibility_modifier());
        }

        builder.build()
    }

    fn parse_trusted_modifier(&mut self) -> SyntaxToken {
        self.expect(SyntaxKind::TrustedKeyword)
    }

    fn parse_module_path(&mut self) -> PathSyntax {
        self.parse_path()
    }

    fn parse_module_body(&mut self) -> ModuleBodySyntax {
        let start = self.peek().full_range().start();
        let mut builder = ModuleBodySyntax::builder(self.syntax_source(), start);

        builder.push_open_brace_token(self.expect(SyntaxKind::OpenBraceToken));
        self.parse_module_items(&mut builder, &[SyntaxKind::CloseBraceToken]);
        builder.push_close_brace_token(self.expect(SyntaxKind::CloseBraceToken));

        builder.build()
    }

    pub(super) fn parse_module_items(
        &mut self,
        builder: &mut impl ModuleItemSyntaxSink,
        terminators: &[SyntaxKind],
    ) {
        while !self.at_any(terminators) && !self.at(SyntaxKind::EndOfFileToken) {
            self.parse_module_item(builder, terminators);
        }
    }

    pub(super) fn parse_module_item(
        &mut self,
        builder: &mut impl ModuleItemSyntaxSink,
        terminators: &[SyntaxKind],
    ) {
        let start = self.peek().start();

        if self.at(SyntaxKind::UsingKeyword) {
            builder.push_using_declaration(self.parse_using_declaration());
            return;
        }

        if self.at(SyntaxKind::ExportKeyword) {
            builder.push_export_declaration(self.parse_export_declaration());
            return;
        }

        if let Some(declaration) = self.parse_module_declaration() {
            builder.push_declaration(declaration);
            return;
        }

        // Recover syntax that is not valid in a module item position.
        if self.recover_until_module_item_boundary(builder, terminators)
            || self.peek().start() != start
        {
            return;
        }

        self.recover_current_token(builder);
    }

    pub(in crate::parser) fn parse_module_declaration(
        &mut self,
    ) -> Option<DeclarationFragmentSyntax> {
        if self.should_parse_function_declaration() {
            return Some(DeclarationFragmentSyntax::Function(
                self.parse_function_declaration(),
            ));
        }

        if self.should_parse_constant_declaration() {
            return Some(DeclarationFragmentSyntax::Constant(
                self.parse_constant_declaration(),
            ));
        }

        if self.should_parse_static_declaration() {
            return Some(DeclarationFragmentSyntax::Static(
                self.parse_static_declaration(),
            ));
        }

        if self.should_parse_predicate_declaration() {
            return Some(DeclarationFragmentSyntax::Predicate(
                self.parse_predicate_declaration(),
            ));
        }

        if self.should_parse_callable_contract_declaration() {
            return Some(DeclarationFragmentSyntax::CallableContract(
                self.parse_callable_contract_declaration(),
            ));
        }

        if self.should_parse_struct_declaration() {
            return Some(DeclarationFragmentSyntax::Struct(
                self.parse_struct_declaration(),
            ));
        }

        if self.should_parse_union_declaration() {
            return Some(DeclarationFragmentSyntax::Union(
                self.parse_union_declaration(),
            ));
        }

        if self.should_parse_trait_declaration() {
            return Some(DeclarationFragmentSyntax::Trait(
                self.parse_trait_declaration(),
            ));
        }

        if self.should_parse_named_trait_implementation_declaration() {
            return Some(DeclarationFragmentSyntax::NamedTraitImplementation(
                self.parse_named_trait_implementation_declaration(),
            ));
        }

        if self.should_parse_unnamed_trait_implementation_declaration() {
            return Some(DeclarationFragmentSyntax::UnnamedTraitImplementation(
                self.parse_unnamed_trait_implementation_declaration(),
            ));
        }

        if self.should_parse_inherent_implementation_declaration() {
            return Some(DeclarationFragmentSyntax::InherentImplementation(
                self.parse_inherent_implementation_declaration(),
            ));
        }

        if self.should_parse_implementation_overload_declaration() {
            return Some(DeclarationFragmentSyntax::ImplementationOverload(
                self.parse_implementation_overload_declaration(),
            ));
        }

        if self.should_parse_callable_overload_declaration() {
            return Some(DeclarationFragmentSyntax::CallableOverload(
                self.parse_callable_overload_declaration(),
            ));
        }

        None
    }

    fn parse_using_declaration(&mut self) -> UsingDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = UsingDeclarationSyntax::builder(self.syntax_source(), start);

        builder.push_using_keyword(self.expect(SyntaxKind::UsingKeyword));

        if self.at(SyntaxKind::InternalKeyword) {
            builder.push_internal_keyword(self.expect(SyntaxKind::InternalKeyword));
        }

        builder.push_path(self.parse_path());
        self.recover_until_module_item_declaration_end(&mut builder);

        builder.push_semicolon_token(self.expect(SyntaxKind::SemicolonToken));

        builder.build()
    }

    fn parse_export_declaration(&mut self) -> ExportDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = ExportDeclarationSyntax::builder(self.syntax_source(), start);

        builder.push_export_keyword(self.expect(SyntaxKind::ExportKeyword));

        builder.push_path(self.parse_path());
        self.recover_until_module_item_declaration_end(&mut builder);

        builder.push_semicolon_token(self.expect(SyntaxKind::SemicolonToken));

        builder.build()
    }

    fn recover_until_module_item_boundary(
        &mut self,
        builder: &mut impl RecoverySyntaxSink,
        terminators: &[SyntaxKind],
    ) -> bool {
        let recovery_set =
            RecoverySet::new(&PARSED_MODULE_ITEM_BOUNDARY_KINDS).with_additional(terminators);

        self.recover_until_balanced_close_brace_or_recovery_set(builder, recovery_set)
    }

    pub(super) fn recover_until_module_item_declaration_end(
        &mut self,
        builder: &mut impl RecoverySyntaxSink,
    ) -> bool {
        let recovery_set = RecoverySet::new(&MODULE_ITEM_DECLARATION_TERMINATOR_KINDS)
            .with_additional(&MODULE_ITEM_START_KINDS);

        self.recover_until_set(builder, recovery_set)
    }

    pub(super) fn should_parse_block_module_declaration(&mut self) -> bool {
        if !self.at_any(&MODULE_DECLARATION_START_KINDS) {
            return false;
        }

        self.scan_ahead(|scan| {
            scan.consume_module_directives_for_scan();
            scan.consume_module_modifiers_for_scan();

            if !scan.at(SyntaxKind::ModuleKeyword) {
                return false;
            }

            scan.consume();

            if !scan.consume_path_for_scan() {
                return false;
            }

            scan.at(SyntaxKind::OpenBraceToken)
        })
    }

    fn consume_module_directives_for_scan(&mut self) {
        self.consume_directives_for_scan(&MODULE_ITEM_START_KINDS, |directive_name| {
            match SyntaxKind::directive_from_name(directive_name) {
                Some(
                    SyntaxKind::TargetDirective
                    | SyntaxKind::TestDirective
                    | SyntaxKind::LinkDirective,
                ) => DirectiveScanKind::ArgumentList,
                _ => DirectiveScanKind::Unknown,
            }
        });
    }

    fn consume_module_modifiers_for_scan(&mut self) {
        if self.at(SyntaxKind::TrustedKeyword) {
            self.consume();
        }

        if self.at_visibility_modifier() {
            self.consume();
        }
    }

    pub(super) fn consume_path_for_scan(&mut self) -> bool {
        if !self.at(SyntaxKind::IdentifierToken) {
            return false;
        }

        self.consume();

        while self.at(SyntaxKind::DotToken) {
            self.consume();

            if !self.at(SyntaxKind::IdentifierToken) {
                break;
            }

            self.consume();
        }

        true
    }
}

trait ModuleDeclarationSyntaxSink: RecoverySyntaxSink {
    fn push_module_directives(&mut self, directives: ModuleDirectivesSyntax);

    fn push_module_modifiers(&mut self, modifiers: ModuleModifiersSyntax);

    fn push_module_keyword(&mut self, token: SyntaxToken);

    fn push_module_path(&mut self, path: PathSyntax);
}

pub(super) trait ModuleItemSyntaxSink: RecoverySyntaxSink {
    fn push_declaration(&mut self, declaration: DeclarationFragmentSyntax) {
        match declaration {
            DeclarationFragmentSyntax::Constant(declaration) => {
                self.push_constant_declaration(declaration);
            }
            DeclarationFragmentSyntax::Static(declaration) => {
                self.push_static_declaration(declaration);
            }
            DeclarationFragmentSyntax::Function(declaration) => {
                self.push_function_declaration(declaration);
            }
            DeclarationFragmentSyntax::Predicate(declaration) => {
                self.push_predicate_declaration(declaration);
            }
            DeclarationFragmentSyntax::CallableContract(declaration) => {
                self.push_callable_contract_declaration(declaration);
            }
            DeclarationFragmentSyntax::CallableOverload(declaration) => {
                self.push_callable_overload_declaration(declaration);
            }
            DeclarationFragmentSyntax::ImplementationOverload(declaration) => {
                self.push_implementation_overload_declaration(declaration);
            }
            DeclarationFragmentSyntax::Struct(declaration) => {
                self.push_struct_declaration(declaration);
            }
            DeclarationFragmentSyntax::Union(declaration) => {
                self.push_union_declaration(declaration);
            }
            DeclarationFragmentSyntax::Trait(declaration) => {
                self.push_trait_declaration(declaration);
            }
            DeclarationFragmentSyntax::InherentImplementation(declaration) => {
                self.push_inherent_implementation_declaration(declaration);
            }
            DeclarationFragmentSyntax::UnnamedTraitImplementation(declaration) => {
                self.push_unnamed_trait_implementation_declaration(declaration);
            }
            DeclarationFragmentSyntax::NamedTraitImplementation(declaration) => {
                self.push_named_trait_implementation_declaration(declaration);
            }
            _ => panic!("parser produced a non-module declaration for a module item"),
        }
    }

    fn push_using_declaration(&mut self, declaration: UsingDeclarationSyntax);

    fn push_export_declaration(&mut self, declaration: ExportDeclarationSyntax);

    fn push_constant_declaration(&mut self, declaration: ConstantDeclarationSyntax);

    fn push_static_declaration(&mut self, declaration: StaticDeclarationSyntax);

    fn push_function_declaration(&mut self, declaration: FunctionDeclarationSyntax);

    fn push_predicate_declaration(&mut self, declaration: PredicateDeclarationSyntax);

    fn push_callable_contract_declaration(
        &mut self,
        declaration: CallableContractDeclarationSyntax,
    );

    fn push_callable_overload_declaration(
        &mut self,
        declaration: CallableOverloadDeclarationSyntax,
    );

    fn push_implementation_overload_declaration(
        &mut self,
        declaration: ImplementationOverloadDeclarationSyntax,
    );

    fn push_struct_declaration(&mut self, declaration: StructDeclarationSyntax);

    fn push_union_declaration(&mut self, declaration: UnionDeclarationSyntax);

    fn push_trait_declaration(&mut self, declaration: TraitDeclarationSyntax);

    fn push_inherent_implementation_declaration(
        &mut self,
        declaration: InherentImplementationDeclarationSyntax,
    );

    fn push_unnamed_trait_implementation_declaration(
        &mut self,
        declaration: UnnamedTraitImplementationDeclarationSyntax,
    );

    fn push_named_trait_implementation_declaration(
        &mut self,
        declaration: NamedTraitImplementationDeclarationSyntax,
    );
}

impl ModuleDeclarationSyntaxSink for SourceUnitModuleDeclarationSyntaxBuilder {
    fn push_module_directives(&mut self, directives: ModuleDirectivesSyntax) {
        SourceUnitModuleDeclarationSyntaxBuilder::push_module_directives(self, directives);
    }

    fn push_module_modifiers(&mut self, modifiers: ModuleModifiersSyntax) {
        SourceUnitModuleDeclarationSyntaxBuilder::push_module_modifiers(self, modifiers);
    }

    fn push_module_keyword(&mut self, token: SyntaxToken) {
        SourceUnitModuleDeclarationSyntaxBuilder::push_module_keyword(self, token);
    }

    fn push_module_path(&mut self, path: PathSyntax) {
        SourceUnitModuleDeclarationSyntaxBuilder::push_module_path(self, path);
    }
}

impl ModuleDeclarationSyntaxSink for BlockModuleDeclarationSyntaxBuilder {
    fn push_module_directives(&mut self, directives: ModuleDirectivesSyntax) {
        BlockModuleDeclarationSyntaxBuilder::push_module_directives(self, directives);
    }

    fn push_module_modifiers(&mut self, modifiers: ModuleModifiersSyntax) {
        BlockModuleDeclarationSyntaxBuilder::push_module_modifiers(self, modifiers);
    }

    fn push_module_keyword(&mut self, token: SyntaxToken) {
        BlockModuleDeclarationSyntaxBuilder::push_module_keyword(self, token);
    }

    fn push_module_path(&mut self, path: PathSyntax) {
        BlockModuleDeclarationSyntaxBuilder::push_module_path(self, path);
    }
}

impl ModuleItemSyntaxSink for SourceUnitSyntaxBuilder {
    fn push_using_declaration(&mut self, declaration: UsingDeclarationSyntax) {
        SourceUnitSyntaxBuilder::push_using_declaration(self, declaration);
    }

    fn push_export_declaration(&mut self, declaration: ExportDeclarationSyntax) {
        SourceUnitSyntaxBuilder::push_export_declaration(self, declaration);
    }

    fn push_constant_declaration(&mut self, declaration: ConstantDeclarationSyntax) {
        SourceUnitSyntaxBuilder::push_constant_declaration(self, declaration);
    }

    fn push_static_declaration(&mut self, declaration: StaticDeclarationSyntax) {
        SourceUnitSyntaxBuilder::push_static_declaration(self, declaration);
    }

    fn push_function_declaration(&mut self, declaration: FunctionDeclarationSyntax) {
        SourceUnitSyntaxBuilder::push_function_declaration(self, declaration);
    }

    fn push_predicate_declaration(&mut self, declaration: PredicateDeclarationSyntax) {
        SourceUnitSyntaxBuilder::push_predicate_declaration(self, declaration);
    }

    fn push_callable_contract_declaration(
        &mut self,
        declaration: CallableContractDeclarationSyntax,
    ) {
        SourceUnitSyntaxBuilder::push_callable_contract_declaration(self, declaration);
    }

    fn push_callable_overload_declaration(
        &mut self,
        declaration: CallableOverloadDeclarationSyntax,
    ) {
        SourceUnitSyntaxBuilder::push_callable_overload_declaration(self, declaration);
    }

    fn push_implementation_overload_declaration(
        &mut self,
        declaration: ImplementationOverloadDeclarationSyntax,
    ) {
        SourceUnitSyntaxBuilder::push_implementation_overload_declaration(self, declaration);
    }

    fn push_struct_declaration(&mut self, declaration: StructDeclarationSyntax) {
        SourceUnitSyntaxBuilder::push_struct_declaration(self, declaration);
    }

    fn push_union_declaration(&mut self, declaration: UnionDeclarationSyntax) {
        SourceUnitSyntaxBuilder::push_union_declaration(self, declaration);
    }

    fn push_trait_declaration(&mut self, declaration: TraitDeclarationSyntax) {
        SourceUnitSyntaxBuilder::push_trait_declaration(self, declaration);
    }

    fn push_inherent_implementation_declaration(
        &mut self,
        declaration: InherentImplementationDeclarationSyntax,
    ) {
        SourceUnitSyntaxBuilder::push_inherent_implementation_declaration(self, declaration);
    }

    fn push_unnamed_trait_implementation_declaration(
        &mut self,
        declaration: UnnamedTraitImplementationDeclarationSyntax,
    ) {
        SourceUnitSyntaxBuilder::push_unnamed_trait_implementation_declaration(self, declaration);
    }

    fn push_named_trait_implementation_declaration(
        &mut self,
        declaration: NamedTraitImplementationDeclarationSyntax,
    ) {
        SourceUnitSyntaxBuilder::push_named_trait_implementation_declaration(self, declaration);
    }
}

impl ModuleItemSyntaxSink for ModuleBodySyntaxBuilder {
    fn push_using_declaration(&mut self, declaration: UsingDeclarationSyntax) {
        ModuleBodySyntaxBuilder::push_using_declaration(self, declaration);
    }

    fn push_export_declaration(&mut self, declaration: ExportDeclarationSyntax) {
        ModuleBodySyntaxBuilder::push_export_declaration(self, declaration);
    }

    fn push_constant_declaration(&mut self, declaration: ConstantDeclarationSyntax) {
        ModuleBodySyntaxBuilder::push_constant_declaration(self, declaration);
    }

    fn push_static_declaration(&mut self, declaration: StaticDeclarationSyntax) {
        ModuleBodySyntaxBuilder::push_static_declaration(self, declaration);
    }

    fn push_function_declaration(&mut self, declaration: FunctionDeclarationSyntax) {
        ModuleBodySyntaxBuilder::push_function_declaration(self, declaration);
    }

    fn push_predicate_declaration(&mut self, declaration: PredicateDeclarationSyntax) {
        ModuleBodySyntaxBuilder::push_predicate_declaration(self, declaration);
    }

    fn push_callable_contract_declaration(
        &mut self,
        declaration: CallableContractDeclarationSyntax,
    ) {
        ModuleBodySyntaxBuilder::push_callable_contract_declaration(self, declaration);
    }

    fn push_callable_overload_declaration(
        &mut self,
        declaration: CallableOverloadDeclarationSyntax,
    ) {
        ModuleBodySyntaxBuilder::push_callable_overload_declaration(self, declaration);
    }

    fn push_implementation_overload_declaration(
        &mut self,
        declaration: ImplementationOverloadDeclarationSyntax,
    ) {
        ModuleBodySyntaxBuilder::push_implementation_overload_declaration(self, declaration);
    }

    fn push_struct_declaration(&mut self, declaration: StructDeclarationSyntax) {
        ModuleBodySyntaxBuilder::push_struct_declaration(self, declaration);
    }

    fn push_union_declaration(&mut self, declaration: UnionDeclarationSyntax) {
        ModuleBodySyntaxBuilder::push_union_declaration(self, declaration);
    }

    fn push_trait_declaration(&mut self, declaration: TraitDeclarationSyntax) {
        ModuleBodySyntaxBuilder::push_trait_declaration(self, declaration);
    }

    fn push_inherent_implementation_declaration(
        &mut self,
        declaration: InherentImplementationDeclarationSyntax,
    ) {
        ModuleBodySyntaxBuilder::push_inherent_implementation_declaration(self, declaration);
    }

    fn push_unnamed_trait_implementation_declaration(
        &mut self,
        declaration: UnnamedTraitImplementationDeclarationSyntax,
    ) {
        ModuleBodySyntaxBuilder::push_unnamed_trait_implementation_declaration(self, declaration);
    }

    fn push_named_trait_implementation_declaration(
        &mut self,
        declaration: NamedTraitImplementationDeclarationSyntax,
    ) {
        ModuleBodySyntaxBuilder::push_named_trait_implementation_declaration(self, declaration);
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_source::TextRange;
    use bray_syntax::{SyntaxKind, SyntaxText};
    use bray_testing::test_source_store as source_store;

    use crate::parser::parse_compilation_unit;
    use crate::test_support::{
        assert_missing_semicolon_diagnostic, marker_offset, parse_diagnostic_kinds,
    };

    #[test]
    fn parser_parses_source_unit_module_declarations() {
        let sources = source_store(["trusted public module main.core;"]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];

        let declaration = match source_unit.source_unit_module_declaration() {
            Some(declaration) => declaration,
            None => panic!("expected source-unit module declaration"),
        };

        let modifiers = declaration.module_modifiers();
        let path = declaration.module_path();

        assert_eq!(source_unit.full_text(), "trusted public module main.core;");
        assert_eq!(declaration.full_text(), "trusted public module main.core;");

        assert_eq!(
            modifiers.trusted_token().map(|token| token.kind()),
            Some(SyntaxKind::TrustedKeyword)
        );

        assert_eq!(
            modifiers.visibility_token().map(|token| token.kind()),
            Some(SyntaxKind::PublicKeyword)
        );

        assert_eq!(path.full_text(), "main.core");
        assert_eq!(path.identifier_tokens().count(), 2);
        assert_eq!(path.dot_tokens().count(), 1);
        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_reports_missing_source_unit_module_semicolon_before_module_item_start() {
        let source = "module main\nusing std.io;";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];

        let declaration = match source_unit.source_unit_module_declaration() {
            Some(declaration) => declaration,
            None => panic!("expected source-unit module declaration"),
        };

        let using_declarations = source_unit.using_declarations().collect::<Vec<_>>();

        let insertion = marker_offset(source, "using");

        let [using_declaration] = using_declarations.as_slice() else {
            panic!("expected one using declaration: {using_declarations:?}");
        };

        let semicolon_token = declaration.semicolon_token();

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(declaration.full_text(), "module main\n");
        assert_eq!(using_declaration.full_text(), "using std.io;");
        assert!(source_unit.skipped_syntax().next().is_none());

        assert!(semicolon_token.is_missing());
        assert_eq!(semicolon_token.kind(), SyntaxKind::SemicolonToken);
        assert_eq!(semicolon_token.range(), TextRange::empty(insertion));

        assert_missing_semicolon_diagnostic(
            &result,
            insertion,
            SyntaxKind::UsingKeyword,
            "using",
            &[DiagnosticKind::SyntaxExpectedToken],
        );
    }

    #[test]
    fn parser_recovers_missing_source_unit_module_semicolon_before_block_module_start() {
        let source = "module main\nmodule extra {}";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];

        let declaration = match source_unit.source_unit_module_declaration() {
            Some(declaration) => declaration,
            None => panic!("expected source-unit module declaration"),
        };

        let block_declarations = source_unit.block_module_declarations().collect::<Vec<_>>();

        let insertion = marker_offset(source, "module extra");

        let [block_declaration] = block_declarations.as_slice() else {
            panic!("expected one block module declaration: {block_declarations:?}");
        };

        let semicolon_token = declaration.semicolon_token();

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(declaration.full_text(), "module main\n");
        assert_eq!(block_declaration.full_text(), "module extra {}");
        assert!(source_unit.skipped_syntax().next().is_none());

        assert!(semicolon_token.is_missing());
        assert_eq!(semicolon_token.kind(), SyntaxKind::SemicolonToken);
        assert_eq!(semicolon_token.range(), TextRange::empty(insertion));

        assert_missing_semicolon_diagnostic(
            &result,
            insertion,
            SyntaxKind::ModuleKeyword,
            "module",
            &[DiagnosticKind::SyntaxExpectedToken],
        );
    }

    #[test]
    fn parser_parses_block_module_declarations_after_source_unit_items() {
        let source = concat!(
            "module net;\n",
            "func parse_packet(pos bytes: &[u8]) -> Packet { return parse_packet_bytes(bytes); }\n",
            "@test module net.tests { @test func parses_minimal_packet() {} }",
        );

        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];

        let source_declaration = match source_unit.source_unit_module_declaration() {
            Some(declaration) => declaration,
            None => panic!("expected source-unit module declaration"),
        };

        let functions = source_unit.function_declarations().collect::<Vec<_>>();
        let blocks = source_unit.block_module_declarations().collect::<Vec<_>>();

        let [production_function] = functions.as_slice() else {
            panic!("expected one source-unit function: {functions:?}");
        };

        let [test_module] = blocks.as_slice() else {
            panic!("expected one block module declaration: {blocks:?}");
        };

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(source_declaration.module_path().full_text(), "net");

        assert_eq!(
            production_function.identifier_token().text(source),
            Some("parse_packet")
        );

        assert_eq!(test_module.module_path().full_text(), "net.tests ");
        assert_eq!(test_module.module_directives().test_directives().count(), 1);
        assert_eq!(test_module.module_body().function_declarations().count(), 1);
        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_recovers_unbraced_items_after_block_module_suffixes() {
        let source = concat!(
            "module main;\n",
            "func before() {}\n",
            "module extra {}\n",
            "func after() {}",
        );

        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];

        let functions = source_unit.function_declarations().collect::<Vec<_>>();
        let blocks = source_unit.block_module_declarations().collect::<Vec<_>>();
        let skipped = source_unit.skipped_syntax().collect::<Vec<_>>();

        let [before] = functions.as_slice() else {
            panic!("expected one source-unit function: {functions:?}");
        };

        let [block] = blocks.as_slice() else {
            panic!("expected one block module declaration: {blocks:?}");
        };

        let [after] = skipped.as_slice() else {
            panic!("expected one skipped-syntax node: {skipped:?}");
        };

        assert_eq!(before.identifier_token().text(source), Some("before"));
        assert_eq!(block.module_path().full_text(), "extra ");
        assert_eq!(after.full_text(), "func after() {}");
        assert_eq!(source_unit.full_text(), source);
    }

    #[test]
    fn parser_parses_test_directives_on_source_unit_module_declarations() {
        let sources = source_store(["@test module main;"]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];

        let declaration = match source_unit.source_unit_module_declaration() {
            Some(declaration) => declaration,
            None => panic!("expected source-unit module declaration"),
        };

        let directives = declaration.module_directives();

        assert_eq!(source_unit.full_text(), "@test module main;");
        assert_eq!(directives.full_text(), "@test ");

        assert_eq!(directives.test_directives().count(), 1);
        assert_eq!(directives.target_directives().count(), 0);
        assert_eq!(directives.link_directives().count(), 0);

        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_parses_target_and_link_module_directives() {
        let sources = source_store(["@target(host) @link(\"m\") module main;"]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];

        let declaration = match source_unit.source_unit_module_declaration() {
            Some(declaration) => declaration,
            None => panic!("expected source-unit module declaration"),
        };

        let directives = declaration.module_directives();
        let targets = directives.target_directives().collect::<Vec<_>>();
        let links = directives.link_directives().collect::<Vec<_>>();

        let [target] = targets.as_slice() else {
            panic!("expected one target directive: {targets:?}");
        };

        let [link] = links.as_slice() else {
            panic!("expected one link directive: {links:?}");
        };

        assert_eq!(
            source_unit.full_text(),
            "@target(host) @link(\"m\") module main;"
        );

        assert_eq!(directives.full_text(), "@target(host) @link(\"m\") ");
        assert_eq!(target.full_text(), "@target(host) ");

        assert_eq!(
            target
                .directive_argument_list()
                .directive_arguments()
                .count(),
            1
        );

        assert_eq!(link.full_text(), "@link(\"m\") ");

        assert_eq!(
            link.directive_argument_list().directive_arguments().count(),
            1
        );

        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_recovers_unknown_module_directives_without_losing_later_directives() {
        let sources = source_store(["@unknown(foo) @test module main;"]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];

        let declaration = match source_unit.source_unit_module_declaration() {
            Some(declaration) => declaration,
            None => panic!("expected source-unit module declaration"),
        };

        let directives = declaration.module_directives();
        let skipped_syntax = directives.skipped_syntax().collect::<Vec<_>>();

        let [skipped] = skipped_syntax.as_slice() else {
            panic!("expected one skipped-syntax node: {skipped_syntax:?}");
        };

        assert_eq!(source_unit.full_text(), "@unknown(foo) @test module main;");
        assert_eq!(directives.full_text(), "@unknown(foo) @test ");
        assert_eq!(skipped.full_text(), "@unknown(foo) ");

        assert_eq!(directives.test_directives().count(), 1);

        assert_eq!(parse_diagnostic_kinds(&result), []);
    }

    #[test]
    fn parser_parses_block_module_declarations_and_function_body_items() {
        let sources =
            source_store(["@test internal module main { func run() {} } module extra {}"]);

        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.block_module_declarations().collect::<Vec<_>>();

        let [first, second] = declarations.as_slice() else {
            panic!("expected two block module declarations: {declarations:?}");
        };

        assert_eq!(
            source_unit.full_text(),
            "@test internal module main { func run() {} } module extra {}"
        );

        assert!(source_unit.source_unit_module_declaration().is_none());
        assert_eq!(first.module_directives().test_directives().count(), 1);

        assert_eq!(
            first
                .module_modifiers()
                .visibility_token()
                .map(|token| token.kind()),
            Some(SyntaxKind::InternalKeyword)
        );

        assert_eq!(first.module_path().full_text(), "main ");
        assert_eq!(first.module_body().full_text(), "{ func run() {} } ");
        assert_eq!(first.module_body().function_declarations().count(), 1);
        assert_eq!(first.module_body().skipped_syntax().count(), 0);
        assert_eq!(second.module_path().full_text(), "extra ");
        assert!(second.module_body().skipped_syntax().next().is_none());

        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_parses_using_and_export_declarations_after_source_unit_modules() {
        let sources = source_store(["module main; using internal core.io; export api;"]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let using_declarations = source_unit.using_declarations().collect::<Vec<_>>();
        let export_declarations = source_unit.export_declarations().collect::<Vec<_>>();

        let [using_declaration] = using_declarations.as_slice() else {
            panic!("expected one using declaration: {using_declarations:?}");
        };

        let [export_declaration] = export_declarations.as_slice() else {
            panic!("expected one export declaration: {export_declarations:?}");
        };

        assert_eq!(
            source_unit.full_text(),
            "module main; using internal core.io; export api;"
        );

        assert_eq!(using_declaration.full_text(), "using internal core.io; ");

        assert_eq!(
            using_declaration
                .internal_keyword()
                .map(|token| token.kind()),
            Some(SyntaxKind::InternalKeyword)
        );

        assert_eq!(using_declaration.path().full_text(), "core.io");
        assert_eq!(using_declaration.path().identifier_tokens().count(), 2);
        assert_eq!(using_declaration.path().dot_tokens().count(), 1);
        assert_eq!(export_declaration.full_text(), "export api;");
        assert_eq!(export_declaration.path().full_text(), "api");
        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_reports_missing_using_semicolon_before_following_declaration_start() {
        let source = "module main; using std.io\nfunc main() {}";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let using_declarations = source_unit.using_declarations().collect::<Vec<_>>();
        let function_declarations = source_unit.function_declarations().collect::<Vec<_>>();

        let insertion = marker_offset(source, "func");

        let [using_declaration] = using_declarations.as_slice() else {
            panic!("expected one using declaration: {using_declarations:?}");
        };

        let [function_declaration] = function_declarations.as_slice() else {
            panic!("expected one function declaration: {function_declarations:?}");
        };

        let semicolon_token = using_declaration.semicolon_token();

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(using_declaration.full_text(), "using std.io\n");
        assert!(using_declaration.skipped_syntax().next().is_none());
        assert_eq!(function_declaration.full_text(), "func main() {}");
        assert!(source_unit.skipped_syntax().next().is_none());

        assert!(semicolon_token.is_missing());

        assert_eq!(semicolon_token.kind(), SyntaxKind::SemicolonToken);
        assert_eq!(semicolon_token.range(), TextRange::empty(insertion));

        assert_missing_semicolon_diagnostic(
            &result,
            insertion,
            SyntaxKind::FuncKeyword,
            "func",
            &[DiagnosticKind::SyntaxExpectedToken],
        );
    }

    #[test]
    fn parser_parses_using_and_export_declarations_inside_block_modules() {
        let sources = source_store(["module main { using core; export api; }"]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.block_module_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one block module declaration: {declarations:?}");
        };

        let body = declaration.module_body();
        let using_declarations = body.using_declarations().collect::<Vec<_>>();
        let export_declarations = body.export_declarations().collect::<Vec<_>>();

        let [using_declaration] = using_declarations.as_slice() else {
            panic!("expected one using declaration: {using_declarations:?}");
        };

        let [export_declaration] = export_declarations.as_slice() else {
            panic!("expected one export declaration: {export_declarations:?}");
        };

        assert_eq!(
            source_unit.full_text(),
            "module main { using core; export api; }"
        );

        assert_eq!(body.full_text(), "{ using core; export api; }");
        assert_eq!(using_declaration.full_text(), "using core; ");
        assert!(using_declaration.internal_keyword().is_none());
        assert_eq!(using_declaration.path().full_text(), "core");
        assert_eq!(export_declaration.full_text(), "export api; ");
        assert_eq!(export_declaration.path().full_text(), "api");

        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_recovers_invalid_module_items_without_losing_later_items() {
        let sources = source_store(["module main { 123 using core; }"]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.block_module_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one block module declaration: {declarations:?}");
        };

        let body = declaration.module_body();
        let skipped_syntax = body.skipped_syntax().collect::<Vec<_>>();

        let [skipped] = skipped_syntax.as_slice() else {
            panic!("expected one skipped-syntax node: {skipped_syntax:?}");
        };

        assert_eq!(source_unit.full_text(), "module main { 123 using core; }");

        assert_eq!(body.using_declarations().count(), 1);
        assert_eq!(body.export_declarations().count(), 0);

        assert_eq!(skipped.full_text(), "123 ");

        assert_eq!(parse_diagnostic_kinds(&result), []);
    }
}
