use std::sync::Arc;

use bray_binder::{BinderFactContext, SymbolFactProvider};
use bray_checker::{
    CheckerFactError, CheckerFactResult, CheckerInfrastructureError, CheckerOutcome, CheckerSource,
    DeclaredStorageMember, DeclaredTypeDefinition, DeclaredUnionVariant, RepresentationIntegerType,
    TypeRepresentationContext, check_declared_type_representation,
};
use bray_compiler_known::RepresentationRole;
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_package_interface::{ImportedSemanticFact, InterfaceSemanticFactKind};
use bray_source::SourceSpan;
use bray_symbols::{
    ConstantExpressionExpectedType, ConstantExpressionOccurrence, ConstantExpressionOccurrenceKey,
    DeclarationExpressionTemplate, DeclaredTypeRepresentation, IntegerConstant, NamedTypeSymbolId,
    StructFieldTypeFact, StructSymbolId, SymbolFactRequest, SymbolProvider, TypeId,
    UnionPayloadFieldTypeFact,
};

use super::super::Compilation;
use super::super::binder::{self, CompilationBinderFacts};
use super::super::checker::checker_fact_error;
use super::super::substitution::named_type;
use super::support::{checked_integer, checked_integer_constant, integer_role, symbol_span};
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError, ImportedSemanticFactKey};

impl Compilation {
    /// Returns the checked source-level representation contract of one named type.
    pub fn declared_type_representation(
        &self,
        subject: NamedTypeSymbolId,
    ) -> Result<Arc<DiagnosticResult<DeclaredTypeRepresentation>>, FactQueryError> {
        self.declared_type_representation_with_cancellation(subject, &self.state.cancellation)
    }

    pub(in crate::compilation) fn declared_type_representation_with_cancellation(
        &self,
        subject: NamedTypeSymbolId,
        cancellation: &CancellationToken,
    ) -> Result<Arc<DiagnosticResult<DeclaredTypeRepresentation>>, FactQueryError> {
        let cell = self.state.declared_type_representations.cell(subject)?;

        let result = cell.get_or_compute(
            &self.state.fact_runtime,
            CompilationFactKey::DeclaredTypeRepresentation(subject),
            cancellation,
            || {
                let semantic_values = self.semantic_value_store()?;

                let available_compiler_known_symbols =
                    self.selected_target().available_compiler_known_symbols();

                let context = CompilationTypeRepresentationContext {
                    compilation: self,
                    cancellation,
                    semantic_values,
                    available_compiler_known_symbols,
                };

                match check_declared_type_representation(&context, subject) {
                    CheckerOutcome::Complete(result) => Ok(Arc::new(result)),
                    CheckerOutcome::Cancelled => Err(FactQueryError::Cancelled),
                    CheckerOutcome::InfrastructureFailure(error) => {
                        Err(FactQueryError::CheckerInfrastructure(error))
                    }
                }
            },
        )?;

        Ok(Arc::clone(result))
    }

    fn declared_type_definition(
        &self,
        subject: NamedTypeSymbolId,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<DeclaredTypeDefinition>, FactQueryError> {
        let facts = self.binder_facts(cancellation)?;

        let surface =
            self.type_associated_surface_result_with_cancellation(subject, cancellation)?;

        let directives = self.declaration_directives(subject.into_any())?;
        let mut diagnostics = DiagnosticBag::new();

        diagnostics.add_range(surface.diagnostics().iter().cloned());
        diagnostics.add_range(directives.diagnostics().iter().cloned());

        let definition = match subject {
            NamedTypeSymbolId::Struct(id) => {
                let record = SymbolProvider::<StructSymbolId>::symbol(facts.symbols(), id)
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                let fields = record
                    .fields()
                    .iter()
                    .map(|field| self.struct_field_definition(&facts, *field, &mut diagnostics))
                    .collect::<Result<Vec<_>, _>>()?;

                DeclaredTypeDefinition::structure(
                    subject,
                    symbol_span(facts.symbols(), subject.into_any(), record.syntax_anchor())?,
                    fields,
                    directives.value().clone(),
                    !surface.value().lifecycle_members().is_empty(),
                    !surface.value().generic().parameters().is_empty(),
                    record.is_recovered(),
                )
            }
            NamedTypeSymbolId::Union(id) => {
                let record = facts
                    .symbols()
                    .union(id)
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                let span =
                    symbol_span(facts.symbols(), subject.into_any(), record.syntax_anchor())?;

                let variants = record
                    .variants()
                    .iter()
                    .map(|variant| {
                        self.union_variant_definition(&facts, *variant, &mut diagnostics)
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                DeclaredTypeDefinition::union(
                    subject,
                    span,
                    variants,
                    directives.value().clone(),
                    !surface.value().lifecycle_members().is_empty(),
                    !surface.value().generic().parameters().is_empty(),
                    record.is_recovered(),
                )
            }
        };

        Ok(DiagnosticResult::new(definition, diagnostics))
    }

    fn struct_field_definition(
        &self,
        facts: &CompilationBinderFacts<'_>,
        field: bray_symbols::StructFieldSymbolId,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<DeclaredStorageMember, FactQueryError> {
        let record = facts
            .symbols()
            .struct_field(field)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let ty = facts
            .symbol_fact(SymbolFactRequest::<StructFieldTypeFact>::new(field))
            .map_err(binder::binder_fact_error)?;

        diagnostics.add_range(ty.diagnostics().iter().cloned());

        Ok(DeclaredStorageMember::new(
            ty.value().clone(),
            symbol_span(facts.symbols(), field.into(), record.syntax_anchor())?,
            record.is_recovered(),
        ))
    }

    fn union_variant_definition(
        &self,
        facts: &CompilationBinderFacts<'_>,
        variant: bray_symbols::UnionVariantSymbolId,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<DeclaredUnionVariant, FactQueryError> {
        let record = facts
            .symbols()
            .union_variant(variant)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let span = symbol_span(facts.symbols(), variant.into(), record.syntax_anchor())?;

        let directives = self.declaration_directives(variant.into())?;

        diagnostics.add_range(directives.diagnostics().iter().cloned());

        let payload = record
            .payload_fields()
            .iter()
            .map(|field| self.union_payload_definition(facts, *field, diagnostics))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(DeclaredUnionVariant::new(
            variant,
            span,
            payload,
            directives.value().clone(),
            record.is_recovered(),
        ))
    }

    fn union_payload_definition(
        &self,
        facts: &CompilationBinderFacts<'_>,
        field: bray_symbols::UnionPayloadFieldSymbolId,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<DeclaredStorageMember, FactQueryError> {
        let record = facts
            .symbols()
            .union_payload_field(field)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let ty = facts
            .symbol_fact(SymbolFactRequest::<UnionPayloadFieldTypeFact>::new(field))
            .map_err(binder::binder_fact_error)?;

        diagnostics.add_range(ty.diagnostics().iter().cloned());

        Ok(DeclaredStorageMember::new(
            ty.value().clone(),
            symbol_span(facts.symbols(), field.into(), record.syntax_anchor())?,
            record.is_recovered(),
        ))
    }
}

struct CompilationTypeRepresentationContext<'compilation> {
    compilation: &'compilation Compilation,
    cancellation: &'compilation CancellationToken,
    semantic_values: &'compilation bray_symbols::SemanticValueStore,
    available_compiler_known_symbols: &'compilation bray_symbols::AvailableCompilerKnownSymbols,
}

impl TypeRepresentationContext for CompilationTypeRepresentationContext<'_> {
    fn maximum_recursion_depth(&self) -> usize {
        self.compilation
            .options()
            .semantic_analysis_limits()
            .recursion_depth()
    }

    fn semantic_values(&self) -> &bray_symbols::SemanticValueStore {
        self.semantic_values
    }

    fn available_compiler_known_symbols(&self) -> &bray_symbols::AvailableCompilerKnownSymbols {
        self.available_compiler_known_symbols
    }

    fn type_definition(
        &self,
        subject: NamedTypeSymbolId,
    ) -> CheckerFactResult<DiagnosticResult<DeclaredTypeDefinition>> {
        self.compilation
            .declared_type_definition(subject, self.cancellation)
            .map_err(checker_fact_error)
    }

    fn imported_type_representation(
        &self,
        subject: NamedTypeSymbolId,
    ) -> CheckerFactResult<DiagnosticResult<Option<DeclaredTypeRepresentation>>> {
        let symbols = self
            .compilation
            .symbol_graph()
            .map_err(checker_fact_error)?;

        if symbols.symbol_key(subject.into_any()).is_some() {
            return Ok(DiagnosticResult::without_diagnostics(None));
        }

        let imported = self
            .compilation
            .imported_symbol_skeleton_result_with_cancellation(self.cancellation)
            .map_err(checker_fact_error)?;

        let Some(address) = imported
            .value()
            .as_ref()
            .and_then(|symbols| symbols.imported_fact_address(subject.into_any()))
        else {
            return Ok(DiagnosticResult::without_diagnostics(None));
        };

        let result = self
            .compilation
            .imported_semantic_fact_result_with_cancellation(
                ImportedSemanticFactKey::new(
                    address.interface(),
                    address.symbol(),
                    InterfaceSemanticFactKind::TypeRepresentation,
                ),
                self.cancellation,
            )
            .map_err(checker_fact_error)?;

        let representation = match result.value().as_ref() {
            [ImportedSemanticFact::TypeRepresentation(representation)] => {
                Some(representation.clone())
            }
            [] => None,
            _ => {
                return Err(CheckerFactError::Infrastructure(
                    CheckerInfrastructureError::SemanticValueUnavailable,
                ));
            }
        };

        Ok(DiagnosticResult::new(
            representation,
            result.diagnostics().clone(),
        ))
    }

    fn source(
        &self,
        syntax: SyntaxAnchor,
    ) -> Result<CheckerSource<'_>, CheckerInfrastructureError> {
        let source_id = syntax.source_id();

        let Some(source) = self.compilation.source(source_id) else {
            return Err(CheckerInfrastructureError::MissingSource { source_id });
        };

        let span = SourceSpan::new(source_id, syntax.full_range());

        let Some(text) = source.text_slice(span.range()) else {
            return Err(CheckerInfrastructureError::InvalidSourceRange { span });
        };

        Ok(CheckerSource::new(span, text))
    }

    fn unsigned_integer(
        &self,
        expression: DeclarationExpressionTemplate,
    ) -> CheckerFactResult<DiagnosticResult<Option<u64>>> {
        let ty = self
            .representation_type(RepresentationRole::ScalarUsize)
            .map_err(checker_fact_error)?;

        let occurrence = ConstantExpressionOccurrence::new(
            ConstantExpressionOccurrenceKey::new(expression.owner(), expression.syntax()),
            ConstantExpressionExpectedType::Resolved(ty),
        );

        let checked = self
            .compilation
            .embedded_constant_term_with_cancellation(occurrence, self.cancellation)
            .map_err(checker_fact_error)?;

        let value = checked_integer(
            self.compilation
                .semantic_value_store()
                .map_err(checker_fact_error)?,
            *checked.value(),
        )
        .map_err(checker_fact_error)?;

        Ok(DiagnosticResult::new(value, checked.diagnostics().clone()))
    }

    fn integer_type(
        &self,
        expression: DeclarationExpressionTemplate,
    ) -> CheckerFactResult<Option<RepresentationIntegerType>> {
        let source = self
            .source(expression.syntax())
            .map_err(CheckerFactError::Infrastructure)?;

        let Some(role) = integer_role(source.text()) else {
            return Ok(None);
        };

        self.integer_type_for_role(role).map(Some)
    }

    fn integer_constant(
        &self,
        expression: DeclarationExpressionTemplate,
        expected: Option<RepresentationIntegerType>,
    ) -> CheckerFactResult<DiagnosticResult<Option<IntegerConstant>>> {
        let expected = match expected {
            Some(expected) => expected,
            None => self.integer_type_for_role(RepresentationRole::ScalarU128)?,
        };

        let occurrence = ConstantExpressionOccurrence::new(
            ConstantExpressionOccurrenceKey::new(expression.owner(), expression.syntax()),
            ConstantExpressionExpectedType::Resolved(expected.ty()),
        );

        let checked = self
            .compilation
            .embedded_constant_term_with_cancellation(occurrence, self.cancellation)
            .map_err(checker_fact_error)?;

        let value = checked_integer_constant(
            self.compilation
                .semantic_value_store()
                .map_err(checker_fact_error)?,
            *checked.value(),
        )
        .map_err(checker_fact_error)?;

        Ok(DiagnosticResult::new(value, checked.diagnostics().clone()))
    }

    fn integer_type_for_role(
        &self,
        role: RepresentationRole,
    ) -> CheckerFactResult<RepresentationIntegerType> {
        let ty = self.representation_type(role).map_err(checker_fact_error)?;

        let representation =
            role.integer_representation()
                .ok_or(CheckerFactError::Infrastructure(
                    CheckerInfrastructureError::SemanticValueUnavailable,
                ))?;

        let target_width = self
            .compilation
            .selected_target()
            .target()
            .profile()
            .machine()
            .pointer_width_bits();

        Ok(RepresentationIntegerType::new(
            ty,
            representation,
            target_width,
        ))
    }

    fn cancellation(&self) -> &dyn bray_base::Cancellation {
        self.cancellation
    }
}

impl CompilationTypeRepresentationContext<'_> {
    fn representation_type(&self, role: RepresentationRole) -> Result<TypeId, FactQueryError> {
        let Some(definition) = self
            .available_compiler_known_symbols()
            .representation_symbol::<StructSymbolId>(role)
        else {
            return Err(FactQueryError::CheckerInfrastructure(
                CheckerInfrastructureError::CompilerKnownRepresentationUnavailable { role },
            ));
        };

        named_type(self.semantic_values, NamedTypeSymbolId::Struct(definition))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_diagnostics::{DiagnosticArg, DiagnosticKind};
    use bray_symbols::{DeclaredCopyContract, DeclaredLayoutMode, NamedTypeSymbolId, SymbolOrigin};

    use crate::test_support::{
        compilation, compilation_with_options, diagnostic_kinds, encoded_semantic_dependency,
        package_identity, source_input,
    };
    use crate::{
        Compilation, CompilationOptions, CompilationRequest, SemanticAnalysisLimits, WorkerBudget,
    };

    #[test]
    fn valid_product_contracts_publish_source_level_representation_facts() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "@layout(stable, align = 16)\n",
            "@copy\n",
            "struct Value\n",
            "{\n",
            "    value: i32;\n",
            "}\n",
        ));

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("test symbol graph must build: {error:?}"));

        let Some(structure) = symbols
            .structures()
            .iter()
            .find(|symbol| symbol.origin() == SymbolOrigin::Source)
        else {
            panic!("test source must declare one structure");
        };

        let result = compilation
            .declared_type_representation(NamedTypeSymbolId::from(structure.id()))
            .unwrap_or_else(|error| panic!("representation contract must check: {error:?}"));

        let repeated = compilation
            .declared_type_representation(NamedTypeSymbolId::from(structure.id()))
            .unwrap_or_else(|error| {
                panic!("representation contract must remain cached: {error:?}")
            });

        assert!(Arc::ptr_eq(&result, &repeated));
        assert!(result.diagnostics().is_empty());
        assert_eq!(result.value().layout(), DeclaredLayoutMode::Stable);
        assert_eq!(result.value().alignment(), Some(16));

        assert_eq!(
            result.value().copy_contract(),
            DeclaredCopyContract::Unconditional
        );

        assert!(result.value().is_plain_storage());
        assert!(result.value().has_finite_size());
    }

    #[test]
    fn inline_recursive_products_report_representation_diagnostics() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "struct Node\n",
            "{\n",
            "    next: Node;\n",
            "}\n",
        ));

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("test symbol graph must build: {error:?}"));

        let Some(structure) = symbols
            .structures()
            .iter()
            .find(|symbol| symbol.origin() == SymbolOrigin::Source)
        else {
            panic!("test source must declare one structure");
        };

        let result = compilation
            .declared_type_representation(NamedTypeSymbolId::from(structure.id()))
            .unwrap_or_else(|error| panic!("recursive representation must recover: {error:?}"));

        assert!(!result.value().has_finite_size());
        assert!(result.value().is_recovered());

        assert_eq!(
            diagnostic_kinds(result.diagnostics()),
            [
                DiagnosticKind::CheckingInvalidStoredType,
                DiagnosticKind::CheckingRecursiveTypeRepresentation,
            ]
        );
    }

    #[test]
    fn representation_recursion_limit_recovers_with_a_structured_diagnostic() {
        let options = CompilationOptions::new(
            WorkerBudget::serial(),
            bray_symbols::ProductKind::Library,
            crate::SelectedTarget::baseline(),
        )
        .with_semantic_analysis_limits(SemanticAnalysisLimits::new(1, 100));

        let compilation = compilation_with_options(
            concat!(
                "module app;\n",
                "\n",
                "struct Outer\n",
                "{\n",
                "    inner: Inner;\n",
                "}\n",
                "\n",
                "struct Inner\n",
                "{\n",
                "    value: i32;\n",
                "}\n",
            ),
            options,
        );

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("test symbol graph must build: {error:?}"));

        let Some(structure) = symbols
            .structures()
            .iter()
            .find(|symbol| symbol.origin() == SymbolOrigin::Source)
        else {
            panic!("test source must declare Outer");
        };

        let result = compilation
            .declared_type_representation(NamedTypeSymbolId::from(structure.id()))
            .unwrap_or_else(|error| panic!("limited representation must recover: {error:?}"));

        assert!(result.value().is_recovered());

        assert!(
            diagnostic_kinds(result.diagnostics())
                .contains(&DiagnosticKind::CheckingTypeRepresentationRecursionLimitExceeded)
        );

        let diagnostic = result
            .diagnostics()
            .iter()
            .find(|diagnostic| {
                diagnostic.kind()
                    == DiagnosticKind::CheckingTypeRepresentationRecursionLimitExceeded
            })
            .unwrap_or_else(|| panic!("recursion limit diagnostic must be present"));

        assert_eq!(diagnostic.args(), &[DiagnosticArg::maximum_count(1)]);
    }

    #[test]
    fn union_layouts_publish_checked_tag_types_and_values() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "@layout(c, tag = u8)\n",
            "union Choice\n",
            "{\n",
            "    @tag(1)\n",
            "    First;\n",
            "\n",
            "    @tag(2)\n",
            "    Second;\n",
            "}\n",
        ));

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("test symbol graph must build: {error:?}"));

        let Some(union) = symbols
            .unions()
            .iter()
            .find(|symbol| symbol.origin() == SymbolOrigin::Source)
        else {
            panic!("test source must declare one union");
        };

        let result = compilation
            .declared_type_representation(NamedTypeSymbolId::from(union.id()))
            .unwrap_or_else(|error| panic!("union representation must check: {error:?}"));

        assert!(result.diagnostics().is_empty());
        assert_eq!(result.value().layout(), DeclaredLayoutMode::C);
        assert!(result.value().union_tag_type().is_some());

        assert_eq!(
            result
                .value()
                .union_tags()
                .iter()
                .map(|tag| tag.value().to_u64())
                .collect::<Vec<_>>(),
            [Some(1), Some(2)]
        );
    }

    #[test]
    fn stable_unions_derive_the_smallest_unsigned_tag_type() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "@layout(stable)\n",
            "union Choice\n",
            "{\n",
            "    First;\n",
            "    Second;\n",
            "}\n",
        ));

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("test symbol graph must build: {error:?}"));

        let Some(union) = symbols
            .unions()
            .iter()
            .find(|symbol| symbol.origin() == SymbolOrigin::Source)
        else {
            panic!("test source must declare one union");
        };

        let result = compilation
            .declared_type_representation(NamedTypeSymbolId::from(union.id()))
            .unwrap_or_else(|error| panic!("union representation must check: {error:?}"));

        assert!(result.diagnostics().is_empty());
        assert!(result.value().union_tag_type().is_some());

        assert_eq!(
            result
                .value()
                .union_tags()
                .iter()
                .map(|tag| tag.value().to_u64())
                .collect::<Vec<_>>(),
            [Some(0), Some(1)]
        );
    }

    #[test]
    fn union_tags_must_fit_the_selected_integer_type() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "@layout(stable, tag = u8)\n",
            "union Choice\n",
            "{\n",
            "    @tag(256)\n",
            "    First;\n",
            "}\n",
        ));

        assert!(
            diagnostic_kinds(compilation.check_diagnostics())
                .contains(&DiagnosticKind::CheckingInvalidUnionTag)
        );
    }

    #[test]
    fn owned_indirection_breaks_recursive_storage_cycles() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "struct Node\n",
            "{\n",
            "    next: box[Heap] Node;\n",
            "}\n",
        ));

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("test symbol graph must build: {error:?}"));

        let Some(structure) = symbols
            .structures()
            .iter()
            .find(|symbol| symbol.origin() == SymbolOrigin::Source)
        else {
            panic!("test source must declare one structure");
        };

        let result = compilation
            .declared_type_representation(NamedTypeSymbolId::from(structure.id()))
            .unwrap_or_else(|error| panic!("indirect recursion must check: {error:?}"));

        assert!(result.diagnostics().is_empty());
        assert!(result.value().has_finite_size());
    }

    #[test]
    fn invalid_layout_contracts_flow_into_package_check_diagnostics() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "@layout(transparent)\n",
            "struct Empty\n",
            "{\n",
            "}\n",
        ));

        assert!(
            diagnostic_kinds(compilation.check_diagnostics())
                .contains(&DiagnosticKind::CheckingInvalidLayoutDirective)
        );
    }

    #[test]
    fn copy_contracts_reject_declared_lifecycle_behavior() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "@copy\n",
            "struct Resource\n",
            "{\n",
            "    value: i32;\n",
            "\n",
            "    destruct()\n",
            "    {\n",
            "    }\n",
            "}\n",
        ));

        assert!(
            diagnostic_kinds(compilation.check_diagnostics())
                .contains(&DiagnosticKind::CheckingInvalidCopyContract)
        );
    }

    #[test]
    fn variant_tags_require_an_explicit_union_layout() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "union Choice\n",
            "{\n",
            "    @tag(1)\n",
            "    First;\n",
            "}\n",
        ));

        assert!(
            diagnostic_kinds(compilation.check_diagnostics())
                .contains(&DiagnosticKind::CheckingInvalidUnionTag)
        );
    }

    #[test]
    fn invalid_layout_options_cannot_be_replaced_by_duplicates() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "@layout(stable, align = 3, align = 4)\n",
            "struct Value\n",
            "{\n",
            "    value: i32;\n",
            "}\n",
        ));

        let kinds = diagnostic_kinds(compilation.check_diagnostics());

        assert_eq!(
            kinds
                .iter()
                .filter(|kind| **kind == DiagnosticKind::CheckingInvalidLayoutDirective)
                .count(),
            2
        );
    }

    #[test]
    fn generic_copy_contracts_remain_conditional() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "@copy\n",
            "struct Wrapper<T>\n",
            "{\n",
            "    value: T;\n",
            "}\n",
        ));

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("test symbol graph must build: {error:?}"));

        let Some(structure) = symbols
            .structures()
            .iter()
            .find(|symbol| symbol.origin() == SymbolOrigin::Source)
        else {
            panic!("test source must declare one structure");
        };

        let result = compilation
            .declared_type_representation(NamedTypeSymbolId::from(structure.id()))
            .unwrap_or_else(|error| panic!("generic representation must check: {error:?}"));

        assert!(result.diagnostics().is_empty());

        assert_eq!(
            result.value().copy_contract(),
            DeclaredCopyContract::Conditional
        );

        assert_eq!(result.value().copy_dependencies().len(), 1);
    }

    #[test]
    fn unused_generic_arguments_do_not_affect_represented_storage() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "@copy\n",
            "struct Phantom<T>\n",
            "{\n",
            "    value: i32;\n",
            "}\n",
            "\n",
            "@copy\n",
            "struct Holder\n",
            "{\n",
            "    value: Phantom<box[Heap] i32>;\n",
            "}\n",
        ));

        let subject = last_source_structure(&compilation);

        let result = compilation
            .declared_type_representation(subject)
            .unwrap_or_else(|error| panic!("generic representation must check: {error:?}"));

        assert!(result.diagnostics().is_empty());

        assert_eq!(
            result.value().copy_contract(),
            DeclaredCopyContract::Unconditional
        );

        assert!(result.value().copy_dependencies().is_empty());
    }

    #[test]
    fn used_generic_arguments_determine_concrete_copyability() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "@copy\n",
            "struct Wrapper<T>\n",
            "{\n",
            "    value: T;\n",
            "}\n",
            "\n",
            "@copy\n",
            "struct Holder\n",
            "{\n",
            "    value: Wrapper<box[Heap] i32>;\n",
            "}\n",
        ));

        let subject = last_source_structure(&compilation);

        let result = compilation
            .declared_type_representation(subject)
            .unwrap_or_else(|error| panic!("generic representation must check: {error:?}"));

        assert_eq!(
            diagnostic_kinds(result.diagnostics()),
            [DiagnosticKind::CheckingInvalidCopyContract]
        );

        assert_eq!(result.value().copy_contract(), DeclaredCopyContract::Absent);
    }

    #[test]
    fn imported_types_use_package_interface_representation_facts() {
        let interface = bray_package_interface::test_support::encoded_semantic_test_interface();

        let request =
            CompilationRequest::new(package_identity(), vec![source_input("module app;", 0)])
                .with_dependency_interfaces([encoded_semantic_dependency(&interface)]);

        let compilation = Compilation::load(request)
            .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"));

        let skeleton = compilation
            .imported_symbol_skeleton_result()
            .unwrap_or_else(|error| panic!("imported skeleton query must complete: {error:?}"));

        let subject = skeleton
            .value()
            .as_ref()
            .and_then(|symbols| symbols.structures().first())
            .map(|structure| NamedTypeSymbolId::from(structure.id()))
            .unwrap_or_else(|| panic!("test dependency must publish an imported structure"));

        let result = compilation
            .declared_type_representation(subject)
            .unwrap_or_else(|error| panic!("imported representation must load: {error:?}"));

        assert!(result.diagnostics().is_empty());
        assert_eq!(result.value().subject(), subject);
        assert!(result.value().is_plain_storage());
        assert!(result.value().has_finite_size());
    }

    fn last_source_structure(compilation: &Compilation) -> NamedTypeSymbolId {
        compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("test symbol graph must build: {error:?}"))
            .structures()
            .iter()
            .rev()
            .find(|symbol| symbol.origin() == SymbolOrigin::Source)
            .map(|structure| NamedTypeSymbolId::from(structure.id()))
            .unwrap_or_else(|| panic!("test source must declare a structure"))
    }
}
