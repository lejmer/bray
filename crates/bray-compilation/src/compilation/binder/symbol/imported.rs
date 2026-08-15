use bray_binder::{BindingQueryError, BindingQueryResult};
use bray_diagnostics::DiagnosticResult;
use bray_package_interface::{
    ImportedImplementation, ImportedSemanticRecord, InterfacePredicateDefinitionState,
    InterfaceSemanticRecordKind,
};
use bray_symbols::{
    CallableContractTemplate, CallableSignatureTemplate, GenericArgumentTemplate,
    GenericDeclarationTemplate, GenericOwnerId, ImportedSemanticAddress, TraitApplicationTemplate,
    UnevaluatedDefaultTemplate,
};

use crate::compilation::binder::CompilationBindingContext;
use crate::fact::ImportedSemanticRecordKey;

pub(super) fn imported_callable_signature(
    context: &CompilationBindingContext<'_>,
    address: ImportedSemanticAddress,
) -> BindingQueryResult<DiagnosticResult<CallableSignatureTemplate>> {
    let result = imported_records(
        context,
        address,
        InterfaceSemanticRecordKind::CallableSignature,
    )?;

    let [ImportedSemanticRecord::CallableSignature(signature)] = result.value().as_ref() else {
        return Err(BindingQueryError::DependencyUnavailable);
    };

    // Candidate-facing results retain shallow Arc-backed templates and diagnostics.
    Ok(DiagnosticResult::new(
        signature.signature().clone(),
        result.diagnostics().clone(),
    ))
}

pub(super) fn imported_generic_declaration(
    context: &CompilationBindingContext<'_>,
    owner: GenericOwnerId,
    address: ImportedSemanticAddress,
) -> BindingQueryResult<DiagnosticResult<GenericDeclarationTemplate>> {
    let result = imported_records(
        context,
        address,
        InterfaceSemanticRecordKind::GenericDeclaration,
    )?;

    let declaration = match result.value().as_ref() {
        [ImportedSemanticRecord::GenericDeclaration(declaration)] => {
            declaration.declaration().clone()
        }
        [] => GenericDeclarationTemplate::new(owner, [], []),
        _ => return Err(BindingQueryError::DependencyUnavailable),
    };

    // The returned result owns immutable declaration data and diagnostics beyond this exact query.
    Ok(DiagnosticResult::new(
        declaration,
        result.diagnostics().clone(),
    ))
}

pub(super) fn imported_callable_parameter_default(
    context: &CompilationBindingContext<'_>,
    address: ImportedSemanticAddress,
) -> BindingQueryResult<DiagnosticResult<UnevaluatedDefaultTemplate>> {
    let result = imported_records(
        context,
        address,
        InterfaceSemanticRecordKind::CallableParameterDefault,
    )?;

    let [ImportedSemanticRecord::CallableParameterDefault(default)] = result.value().as_ref()
    else {
        return Err(BindingQueryError::DependencyUnavailable);
    };

    // Imported results share immutable diagnostic storage.
    Ok(DiagnosticResult::new(
        default.default(),
        result.diagnostics().clone(),
    ))
}

pub(super) fn imported_predicate_definition_state(
    context: &CompilationBindingContext<'_>,
    address: ImportedSemanticAddress,
) -> BindingQueryResult<DiagnosticResult<InterfacePredicateDefinitionState>> {
    let result = imported_records(
        context,
        address,
        InterfaceSemanticRecordKind::PredicateDefinition,
    )?;

    let [ImportedSemanticRecord::PredicateDefinition(definition)] = result.value().as_ref() else {
        return Err(BindingQueryError::DependencyUnavailable);
    };

    Ok(DiagnosticResult::new(
        definition.state(),
        result.diagnostics().clone(),
    ))
}

pub(super) fn imported_declared_type(
    context: &CompilationBindingContext<'_>,
    address: ImportedSemanticAddress,
) -> BindingQueryResult<DiagnosticResult<bray_symbols::TypeExpressionTemplate>> {
    let result = imported_records(context, address, InterfaceSemanticRecordKind::DeclaredType)?;

    let [ImportedSemanticRecord::DeclaredType(declared)] = result.value().as_ref() else {
        return Err(BindingQueryError::DependencyUnavailable);
    };

    Ok(DiagnosticResult::new(
        bray_symbols::TypeExpressionTemplate::Resolved(declared.ty()),
        result.diagnostics().clone(),
    ))
}

pub(super) fn imported_callable_contract(
    context: &CompilationBindingContext<'_>,
    address: ImportedSemanticAddress,
) -> BindingQueryResult<DiagnosticResult<CallableContractTemplate>> {
    let result = imported_records(
        context,
        address,
        InterfaceSemanticRecordKind::CallableContracts,
    )?;

    let [ImportedSemanticRecord::CallableContracts(contracts)] = result.value().as_ref() else {
        return Err(BindingQueryError::DependencyUnavailable);
    };

    // The candidate-facing result shares the imported contract and diagnostics.
    Ok(DiagnosticResult::new(
        CallableContractTemplate::Resolved(contracts.contract().clone()),
        result.diagnostics().clone(),
    ))
}

pub(super) fn imported_callable_contracts(
    context: &CompilationBindingContext<'_>,
    address: ImportedSemanticAddress,
) -> BindingQueryResult<DiagnosticResult<bray_symbols::CallableContractSet>> {
    let result = imported_records(
        context,
        address,
        InterfaceSemanticRecordKind::CallableContracts,
    )?;

    let [ImportedSemanticRecord::CallableContracts(contracts)] = result.value().as_ref() else {
        return Err(BindingQueryError::DependencyUnavailable);
    };

    Ok(DiagnosticResult::new(
        contracts.contract().clone(),
        result.diagnostics().clone(),
    ))
}

pub(in crate::compilation) fn imported_declaration_template(
    context: &CompilationBindingContext<'_>,
    address: ImportedSemanticAddress,
    kind: bray_bound_tree::CheckedTemplateKind,
) -> BindingQueryResult<DiagnosticResult<Option<bray_package_interface::ImportedDeclarationTemplate>>>
{
    imported_declaration_template_at(context, address, kind, bray_symbols::SymbolOrdinal::new(0))
}

pub(in crate::compilation) fn imported_declaration_template_at(
    context: &CompilationBindingContext<'_>,
    address: ImportedSemanticAddress,
    kind: bray_bound_tree::CheckedTemplateKind,
    ordinal: bray_symbols::SymbolOrdinal,
) -> BindingQueryResult<DiagnosticResult<Option<bray_package_interface::ImportedDeclarationTemplate>>>
{
    let result = imported_records(
        context,
        address,
        InterfaceSemanticRecordKind::DeclarationTemplate,
    )?;

    let mut templates = result.value().iter().filter_map(|record| match record {
        ImportedSemanticRecord::DeclarationTemplate(template)
            if template.kind() == kind && template.ordinal() == ordinal =>
        {
            Some(template)
        }
        _ => None,
    });

    let template = templates.next();

    if templates.next().is_some() {
        return Err(BindingQueryError::DependencyUnavailable);
    }

    // The imported result and this typed view share the same immutable template graph.
    Ok(DiagnosticResult::new(
        template.cloned(),
        result.diagnostics().clone(),
    ))
}

pub(in crate::compilation) fn imported_implementation(
    context: &CompilationBindingContext<'_>,
    address: ImportedSemanticAddress,
) -> BindingQueryResult<DiagnosticResult<ImportedImplementation>> {
    let result = imported_records(
        context,
        address,
        InterfaceSemanticRecordKind::Implementation,
    )?;

    let [ImportedSemanticRecord::Implementation(implementation)] = result.value().as_ref() else {
        return Err(BindingQueryError::DependencyUnavailable);
    };

    // The adapter owns an Arc-backed header independently of the exact query result.
    Ok(DiagnosticResult::new(
        implementation.clone(),
        result.diagnostics().clone(),
    ))
}

pub(super) fn imported_implemented_trait_application(
    context: &CompilationBindingContext<'_>,
    address: ImportedSemanticAddress,
) -> BindingQueryResult<DiagnosticResult<Option<TraitApplicationTemplate>>> {
    let result = imported_records(
        context,
        address,
        InterfaceSemanticRecordKind::Implementation,
    )?;

    let [ImportedSemanticRecord::Implementation(implementation)] = result.value().as_ref() else {
        return Err(BindingQueryError::DependencyUnavailable);
    };

    let Some(application) = implementation.trait_application() else {
        return Ok(DiagnosticResult::new(None, result.diagnostics().clone()));
    };

    let application = context
        .semantic_values
        .trait_application_data(application)
        .map_err(|_| BindingQueryError::DependencyUnavailable)?;

    let substitution = context
        .semantic_values
        .generic_substitution_data(application.substitution())
        .map_err(|_| BindingQueryError::DependencyUnavailable)?;

    let template = TraitApplicationTemplate::new(
        application.definition(),
        substitution
            .bindings()
            .iter()
            .map(|binding| binding.parameter()),
        substitution
            .bindings()
            .iter()
            .map(|binding| GenericArgumentTemplate::Resolved(binding.argument())),
    );

    // The reconstructed result owns imported diagnostics beyond the shared interface query.
    Ok(DiagnosticResult::new(
        Some(template),
        result.diagnostics().clone(),
    ))
}

fn imported_records(
    context: &CompilationBindingContext<'_>,
    address: ImportedSemanticAddress,
    kind: InterfaceSemanticRecordKind,
) -> BindingQueryResult<std::sync::Arc<DiagnosticResult<std::sync::Arc<[ImportedSemanticRecord]>>>>
{
    context
        .compilation
        .imported_semantics_with_cancellation(
            ImportedSemanticRecordKey::new(address.interface(), address.symbol(), kind),
            context.cancellation,
        )
        .map_err(|error| match error {
            crate::fact::FactQueryError::Cancelled => BindingQueryError::Cancelled,
            _ => BindingQueryError::DependencyUnavailable,
        })
}
