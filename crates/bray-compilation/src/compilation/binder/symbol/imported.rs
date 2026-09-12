use crate::compilation::binder::BindingQueryResult;
use bray_binder::BindingQueryError;
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
use crate::compilation::{SemanticDataKind, SemanticQueryContext, SemanticQueryViolation};
use crate::fact::{CompilationFactKey, ImportedSemanticRecordKey};

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
        return Err(invalid_imported_record_set(
            address,
            InterfaceSemanticRecordKind::CallableSignature,
            result.value(),
        ));
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
        records => {
            return Err(invalid_imported_record_set(
                address,
                InterfaceSemanticRecordKind::GenericDeclaration,
                records,
            ));
        }
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
        return Err(invalid_imported_record_set(
            address,
            InterfaceSemanticRecordKind::CallableParameterDefault,
            result.value(),
        ));
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
        return Err(invalid_imported_record_set(
            address,
            InterfaceSemanticRecordKind::PredicateDefinition,
            result.value(),
        ));
    };

    Ok(DiagnosticResult::new(
        definition.state(),
        result.diagnostics().clone(),
    ))
}

pub(super) fn imported_predicate_signature(
    context: &CompilationBindingContext<'_>,
    address: ImportedSemanticAddress,
) -> BindingQueryResult<DiagnosticResult<bray_symbols::PredicateSignatureTemplate>> {
    let result = imported_records(
        context,
        address,
        InterfaceSemanticRecordKind::PredicateDefinition,
    )?;

    let [ImportedSemanticRecord::PredicateDefinition(definition)] = result.value().as_ref() else {
        return Err(invalid_imported_record_set(
            address,
            InterfaceSemanticRecordKind::PredicateDefinition,
            result.value(),
        ));
    };

    // Candidate queries share the immutable parameter signature and diagnostics.
    Ok(DiagnosticResult::new(
        definition.signature().clone(),
        result.diagnostics().clone(),
    ))
}

pub(super) fn imported_declared_type(
    context: &CompilationBindingContext<'_>,
    address: ImportedSemanticAddress,
) -> BindingQueryResult<DiagnosticResult<bray_symbols::TypeExpressionTemplate>> {
    let result = imported_records(context, address, InterfaceSemanticRecordKind::DeclaredType)?;

    let [ImportedSemanticRecord::DeclaredType(declared)] = result.value().as_ref() else {
        return Err(invalid_imported_record_set(
            address,
            InterfaceSemanticRecordKind::DeclaredType,
            result.value(),
        ));
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
        return Err(invalid_imported_record_set(
            address,
            InterfaceSemanticRecordKind::CallableContracts,
            result.value(),
        ));
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
        return Err(invalid_imported_record_set(
            address,
            InterfaceSemanticRecordKind::CallableContracts,
            result.value(),
        ));
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

    let templates = result
        .value()
        .iter()
        .filter_map(|record| match record {
            ImportedSemanticRecord::DeclarationTemplate(template)
                if template.kind() == kind && template.ordinal() == ordinal =>
            {
                Some(template)
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    if templates.len() > 1 {
        return Err(imported_record_count_mismatch(
            address,
            InterfaceSemanticRecordKind::DeclarationTemplate,
            1,
            templates.len(),
        ));
    }

    let template = templates.first().copied();

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
        return Err(invalid_imported_record_set(
            address,
            InterfaceSemanticRecordKind::Implementation,
            result.value(),
        ));
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
        return Err(invalid_imported_record_set(
            address,
            InterfaceSemanticRecordKind::Implementation,
            result.value(),
        ));
    };

    let Some(application) = implementation.trait_application() else {
        return Ok(DiagnosticResult::new(None, result.diagnostics().clone()));
    };

    let application = context
        .semantic_values
        .trait_application_data(application)
        .map_err(crate::compilation::binder::semantic_value_binding_error)?;

    let substitution = context
        .semantic_values
        .generic_substitution_data(application.substitution())
        .map_err(crate::compilation::binder::semantic_value_binding_error)?;

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
        .map_err(super::binding::binder_error)
}

fn invalid_imported_record_set(
    address: ImportedSemanticAddress,
    kind: InterfaceSemanticRecordKind,
    records: &[ImportedSemanticRecord],
) -> BindingQueryError<crate::fact::FactQueryError> {
    if records.len() != 1 {
        return imported_record_count_mismatch(address, kind, 1, records.len());
    }

    imported_record_contract(
        address,
        kind,
        SemanticQueryViolation::ImportedRecordKindMismatch {
            expected: kind,
            actual: records[0].kind(),
        },
    )
}

fn imported_record_count_mismatch(
    address: ImportedSemanticAddress,
    kind: InterfaceSemanticRecordKind,
    expected: usize,
    actual: usize,
) -> BindingQueryError<crate::fact::FactQueryError> {
    imported_record_contract(
        address,
        kind,
        SemanticQueryViolation::CountMismatch {
            data: SemanticDataKind::DeclarationRecord,
            expected,
            actual,
        },
    )
}

pub(in crate::compilation) fn missing_imported_template(
    address: ImportedSemanticAddress,
    kind: InterfaceSemanticRecordKind,
) -> BindingQueryError<crate::fact::FactQueryError> {
    imported_record_contract(
        address,
        kind,
        SemanticQueryViolation::Missing(SemanticDataKind::ImportedTemplate),
    )
}

fn imported_record_contract(
    address: ImportedSemanticAddress,
    kind: InterfaceSemanticRecordKind,
    violation: SemanticQueryViolation,
) -> BindingQueryError<crate::fact::FactQueryError> {
    let key = ImportedSemanticRecordKey::new(address.interface(), address.symbol(), kind);

    crate::compilation::binder::semantic_contract_binding_error(
        SemanticQueryContext::Fact(CompilationFactKey::ImportedSemanticRecord(key)),
        violation,
    )
}

#[cfg(test)]
mod tests {
    use bray_package_interface::{
        InterfaceSemanticRecordKind, test_support::encoded_semantic_test_interface,
    };

    use super::{invalid_imported_record_set, missing_imported_template};
    use crate::compilation::binder::binding_query_error;
    use crate::compilation::{
        SemanticDataKind, SemanticQueryContext, SemanticQueryFailure, SemanticQueryViolation,
    };
    use crate::fact::{CompilationFactKey, FactQueryError, ImportedSemanticRecordKey};
    use crate::test_support::{compilation_with_dependencies, encoded_semantic_dependency};

    #[test]
    fn imported_record_failures_retain_exact_artifact_address_and_kinds() {
        let fixture = encoded_semantic_test_interface();

        let compilation =
            compilation_with_dependencies("module app;", [encoded_semantic_dependency(&fixture)]);

        let skeleton = compilation
            .imported_symbol_skeleton_result()
            .unwrap_or_else(|error| panic!("imported skeleton must load: {error:?}"));

        let skeleton = skeleton
            .value()
            .as_deref()
            .unwrap_or_else(|| panic!("valid dependency must publish its symbol skeleton"));

        let function = skeleton
            .functions()
            .iter()
            .find(|function| {
                function
                    .imported_semantic_key()
                    .is_some_and(|key| key.symbol() == fixture.template_owner)
            })
            .unwrap_or_else(|| panic!("fixture function must be imported"));

        let address = skeleton
            .imported_semantic_address(function.id().into())
            .unwrap_or_else(|| panic!("imported function must retain its semantic address"));

        let kind = InterfaceSemanticRecordKind::CallableSignature;
        let key = ImportedSemanticRecordKey::new(address.interface(), address.symbol(), kind);

        let expected: FactQueryError = SemanticQueryFailure::contract(
            SemanticQueryContext::Fact(CompilationFactKey::ImportedSemanticRecord(key)),
            SemanticQueryViolation::CountMismatch {
                data: SemanticDataKind::DeclarationRecord,
                expected: 1,
                actual: 0,
            },
        )
        .into();

        assert_eq!(
            binding_query_error(invalid_imported_record_set(address, kind, &[])),
            expected
        );

        let template_kind = InterfaceSemanticRecordKind::DeclarationTemplate;

        let template_key =
            ImportedSemanticRecordKey::new(address.interface(), address.symbol(), template_kind);

        let expected: FactQueryError = SemanticQueryFailure::contract(
            SemanticQueryContext::Fact(CompilationFactKey::ImportedSemanticRecord(template_key)),
            SemanticQueryViolation::Missing(SemanticDataKind::ImportedTemplate),
        )
        .into();

        assert_eq!(
            binding_query_error(missing_imported_template(address, template_kind)),
            expected
        );

        let records = compilation
            .imported_semantics(ImportedSemanticRecordKey::new(
                address.interface(),
                address.symbol(),
                kind,
            ))
            .unwrap_or_else(|error| panic!("imported semantics must load: {error:?}"));

        let requested_kind = InterfaceSemanticRecordKind::GenericDeclaration;

        let requested_key =
            ImportedSemanticRecordKey::new(address.interface(), address.symbol(), requested_kind);

        let expected: FactQueryError = SemanticQueryFailure::contract(
            SemanticQueryContext::Fact(CompilationFactKey::ImportedSemanticRecord(requested_key)),
            SemanticQueryViolation::ImportedRecordKindMismatch {
                expected: requested_kind,
                actual: kind,
            },
        )
        .into();

        assert_eq!(
            binding_query_error(invalid_imported_record_set(
                address,
                requested_kind,
                records.value(),
            )),
            expected,
        );
    }
}
