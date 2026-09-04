use std::collections::BTreeMap;

use bray_bound_tree::BoundUnitKey;
use bray_checker::{
    CheckerUnitView, ConstantChecker, ConstantEvaluationInput, ConstantEvaluationLimits,
    DefaultConstantChecker,
};
use bray_diagnostics::DiagnosticBag;
use bray_package_interface::{
    InterfaceConstantTermId, InterfaceConstantValueId, InterfaceDependencyContractId,
    InterfaceGenericSubstitutionId, InterfaceImplementationInstanceId, InterfaceSymbolReference,
    InterfaceTraitApplicationId, InterfaceTypeId,
};
use bray_symbols::{
    AnySymbolId, ConstantTermId, ConstantValueId, GenericSubstitutionId, ImplementationInstanceId,
    TraitApplicationId, TypeId,
};

use super::super::PackageInterfaceExportError;
use crate::compilation::Compilation;
use crate::compilation::checker::{CompilationCheckerContext, checker_result};
use crate::compilation::unit::semantic_unit_context_for;

use super::context::{CheckedConstantExpression, SemanticExporter};

pub(super) struct ExecutableTemplateExporter<'export, 'compilation> {
    pub(super) semantic: &'export mut SemanticExporter<'compilation>,
    pub(super) nested: &'export BTreeMap<BoundUnitKey, bray_ir::MirExecutableTemplateId>,
}

impl<'export, 'compilation> ExecutableTemplateExporter<'export, 'compilation> {
    pub(super) const fn new(
        semantic: &'export mut SemanticExporter<'compilation>,
        nested: &'export BTreeMap<BoundUnitKey, bray_ir::MirExecutableTemplateId>,
    ) -> Self {
        Self { semantic, nested }
    }
}

impl bray_package_interface::ExecutableTemplateEncodeContext
    for ExecutableTemplateExporter<'_, '_>
{
    type Error = PackageInterfaceExportError;

    fn type_id(&mut self, id: TypeId) -> Result<InterfaceTypeId, Self::Error> {
        self.semantic.type_id(id)
    }

    fn constant_value_id(
        &mut self,
        id: ConstantValueId,
    ) -> Result<InterfaceConstantValueId, Self::Error> {
        self.semantic.constant_value_id(id)
    }

    fn constant_term_id(
        &mut self,
        id: ConstantTermId,
    ) -> Result<InterfaceConstantTermId, Self::Error> {
        self.semantic.constant_term_id(id)
    }

    fn substitution_id(
        &mut self,
        id: GenericSubstitutionId,
    ) -> Result<InterfaceGenericSubstitutionId, Self::Error> {
        self.semantic.substitution_id(id)
    }

    fn trait_application_id(
        &mut self,
        id: TraitApplicationId,
    ) -> Result<InterfaceTraitApplicationId, Self::Error> {
        self.semantic.trait_application_id(id)
    }

    fn implementation_instance_id(
        &mut self,
        id: ImplementationInstanceId,
    ) -> Result<InterfaceImplementationInstanceId, Self::Error> {
        self.semantic.implementation_instance_id(id)
    }

    fn dependency_contract_id(
        &mut self,
        id: bray_symbols::DependencyContractTemplateId,
    ) -> Result<InterfaceDependencyContractId, Self::Error> {
        self.semantic.dependency_contract_id(id)
    }

    fn symbol_reference(
        &mut self,
        id: AnySymbolId,
    ) -> Result<InterfaceSymbolReference, Self::Error> {
        self.semantic.symbol_reference(id)
    }

    fn nested_executable_id(
        &mut self,
        key: &BoundUnitKey,
    ) -> Result<bray_ir::MirExecutableTemplateId, Self::Error> {
        self.nested.get(key).copied().ok_or_else(|| {
            super::super::export_contract_error(
                super::super::PackageInterfaceExportContract::MissingNestedExecutableTemplate,
            )
        })
    }
}

pub(super) fn index(length: usize) -> Result<u32, PackageInterfaceExportError> {
    u32::try_from(length)
        .map_err(|_| super::super::capacity_export_error("semantic_template_node_count", length))
}

pub(super) fn checked_constraint_expression(
    compilation: &Compilation,
    generic: &bray_symbols::GenericDeclarationTemplate,
    unit: bray_declarations::SyntaxAnchor,
    expression: bray_symbols::DeclarationExpressionTemplate,
) -> Result<CheckedConstantExpression, PackageInterfaceExportError> {
    let cancellation = &compilation.state.cancellation;

    let key = compilation
        .constraint_unit_key(expression.owner(), unit)
        .map_err(|error| super::super::fact_query_export_error(error))?;

    let bound = compilation
        .bound_unit_with_cancellation(key.clone(), cancellation)
        .map_err(|error| super::super::fact_query_export_error(error))?;

    let semantics = compilation
        .expression_semantics_with_cancellation(key.clone(), cancellation)
        .map_err(|error| super::super::fact_query_export_error(error))?;

    let diagnostics = DiagnosticBag::merged_all([
        bound.result().diagnostics(),
        semantics.result().diagnostics(),
    ]);

    if diagnostics.has_errors() {
        return Err(incomplete(expression.owner()));
    }

    let root = crate::compilation::generic_constraint::constraint_expression(
        bound.result().value(),
        expression.syntax(),
    )
    .ok_or_else(|| incomplete(expression.owner()))?;

    let values = compilation
        .semantic_value_store()
        .map_err(|error| super::super::fact_query_export_error(error))?;

    let substitution = crate::compilation::substitution::identity_substitution(
        values,
        generic.owner(),
        generic.parameters(),
    )
    .map_err(|error| super::super::fact_query_export_error(error))?;

    let (references, reference_diagnostics) = compilation
        .concrete_call_references(
            bound.result().value(),
            semantics.result().value().selections(),
            substitution,
            None,
            &BTreeMap::new(),
            ConstantEvaluationLimits::default(),
            cancellation,
        )
        .map_err(|error| super::super::fact_query_export_error(error))?;

    if reference_diagnostics.has_errors() {
        return Err(incomplete(expression.owner()));
    }

    let context = CompilationCheckerContext::new(
        compilation
            .binding_context_for(bound.result().value().key(), cancellation)
            .map_err(|error| super::super::fact_query_export_error(error))?,
    );

    let semantic_context = semantic_unit_context_for(context.symbols(), bound.result().value())
        .map_err(super::super::fact_query_export_error)?;

    let request = CheckerUnitView::new(bound.result().value(), &semantic_context, &context)
        .map_err(|error| {
            super::super::checker_infrastructure_export_error(
                bray_checker::CheckerInfrastructureError::InvalidUnitView(error),
            )
        })?;

    let resolver = crate::compilation::constant::CompilationConstantCallResolver::new(
        compilation,
        cancellation,
    );

    let input = ConstantEvaluationInput::for_expression(
        semantics.result().value(),
        root,
        references,
        &resolver,
    );

    let checked = checker_result(DefaultConstantChecker.check_constant_term(request, &input))
        .map_err(|error| super::super::fact_query_export_error(error))?;

    if checked.diagnostics().has_errors() {
        return Err(incomplete(expression.owner()));
    }

    let ty = semantics
        .result()
        .value()
        .types()
        .expression(root)
        .ok_or_else(|| incomplete(expression.owner()))?
        .ty();

    Ok(CheckedConstantExpression {
        term: *checked.value(),
        ty,
    })
}

pub(super) fn incomplete(symbol: AnySymbolId) -> PackageInterfaceExportError {
    PackageInterfaceExportError::IncompletePublicDeclarationSemantics(symbol.kind())
}
