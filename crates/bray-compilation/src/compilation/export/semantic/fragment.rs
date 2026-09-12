use std::collections::BTreeMap;

use bray_package_interface::{
    InterfaceCallableInstanceId, InterfaceConstantTermId, InterfaceConstantValueId,
    InterfaceDependencyContractId, InterfaceGenericSubstitutionId,
    InterfaceImplementationInstanceId, InterfaceSemanticIdRemap, InterfaceSemantics,
    InterfaceTraitApplicationId, InterfaceTypeId,
};
use bray_symbols::{
    AnySymbolId, CallableInstanceId, ConstantTermId, ConstantValueId, ExternalSymbolKey,
    GenericSubstitutionId, ImplementationInstanceId, TraitApplicationId, TypeId,
};

use super::super::PackageInterfaceExportError;
use super::context::SemanticExporter;
use super::declarations::{
    ExportedDeclarations, export_callable_semantics, export_default_semantics,
    export_generic_semantics, export_type_semantics,
};
use super::implementation::export_constant_semantics;
use super::predicate::export_predicate_semantics;
use crate::compilation::Compilation;
use crate::compilation::binder::CompilationBindingContext;

pub(super) struct SemanticFragment {
    symbol: AnySymbolId,
    identity: ExternalSymbolKey,
    semantics: InterfaceSemantics,
    origins: SemanticValueOrigins,
}

impl SemanticFragment {
    pub(super) fn build(
        compilation: &Compilation,
        graph: &bray_symbols::SymbolGraph,
        binder: &CompilationBindingContext<'_>,
        surface: &bray_package_interface::PackageInterfaceSurface,
        keys: &BTreeMap<AnySymbolId, ExternalSymbolKey>,
        values: &bray_symbols::SemanticValueStore,
        symbol: AnySymbolId,
    ) -> Result<Self, PackageInterfaceExportError> {
        let identity = keys.get(&symbol).cloned().ok_or(
            PackageInterfaceExportError::IncompletePublicDeclarationSemantics(symbol.kind()),
        )?;

        let mut export = SemanticExporter::new(compilation, graph, surface, keys, values)
            .with_declaration(symbol);

        let mut declarations = ExportedDeclarations::default();

        export_callable_semantics(
            compilation,
            graph,
            binder,
            symbol,
            &mut export,
            &mut declarations,
        )?;

        export_generic_semantics(compilation, binder, symbol, &mut export, &mut declarations)?;

        export_constant_semantics(compilation, binder, symbol, &mut export, &mut declarations)?;

        export_predicate_semantics(compilation, binder, symbol, &mut export, &mut declarations)?;

        export_default_semantics(
            compilation,
            graph,
            binder,
            symbol,
            &mut export,
            &mut declarations,
        )?;

        export_type_semantics(compilation, symbol, &mut export, &mut declarations)?;

        let (semantics, origins) = SemanticValueOrigins::from_export(export, declarations);

        Ok(Self {
            symbol,
            identity,
            semantics,
            origins,
        })
    }

    pub(super) const fn identity(&self) -> &ExternalSymbolKey {
        &self.identity
    }

    pub(super) fn diagnostic_identity(
        &self,
        graph: &bray_symbols::SymbolGraph,
    ) -> Result<bray_diagnostics::DiagnosticInterfaceSymbolIdentity, PackageInterfaceExportError>
    {
        crate::compilation::diagnostics::symbol_diagnostic_identity(graph, None, self.symbol)
            .map_err(super::super::fragment_coordination_export_error)
    }

    pub(super) fn exact_diagnostic_identity(
        &self,
        graph: &bray_symbols::SymbolGraph,
    ) -> Result<bray_diagnostics::DiagnosticInterfaceSymbolIdentity, PackageInterfaceExportError>
    {
        graph
            .symbol_key(self.symbol)
            .map(bray_symbols::diagnostic_symbol_identity)
            .ok_or_else(|| {
                PackageInterfaceExportError::MissingSemanticFragmentSymbol(self.identity.clone())
            })
    }

    pub(super) fn diagnostic_span(
        &self,
        graph: &bray_symbols::SymbolGraph,
    ) -> Option<bray_source::SourceSpan> {
        graph
            .declaration_syntax_anchor(self.symbol)
            .map(|anchor| bray_source::SourceSpan::new(anchor.source_id(), anchor.full_range()))
    }

    pub(super) fn commit(
        self,
        export: &mut SemanticExporter<'_>,
    ) -> Result<(InterfaceSemantics, InterfaceSemanticIdRemap), PackageInterfaceExportError> {
        let remap = match self.origins.commit(export) {
            Ok(remap) => remap,
            Err(PackageInterfaceExportError::IncompleteSemanticFragment {
                declaration: None,
                table,
                reference,
            }) => {
                let declaration = self.diagnostic_identity(export.graph)?;

                return Err(PackageInterfaceExportError::IncompleteSemanticFragment {
                    declaration: Some(declaration),
                    table,
                    reference,
                });
            }
            Err(error) => return Err(error),
        };

        Ok((self.semantics, remap))
    }
}

struct SemanticValueOrigins {
    substitutions: BTreeMap<GenericSubstitutionId, InterfaceGenericSubstitutionId>,
    trait_applications: BTreeMap<TraitApplicationId, InterfaceTraitApplicationId>,
    callable_instances: BTreeMap<CallableInstanceId, InterfaceCallableInstanceId>,
    implementation_instances: BTreeMap<ImplementationInstanceId, InterfaceImplementationInstanceId>,
    dependency_contracts:
        BTreeMap<bray_symbols::DependencyContractTemplateId, InterfaceDependencyContractId>,
    types: BTreeMap<TypeId, InterfaceTypeId>,
    constant_values: BTreeMap<ConstantValueId, InterfaceConstantValueId>,
    constant_terms: BTreeMap<ConstantTermId, InterfaceConstantTermId>,
}

impl SemanticValueOrigins {
    fn from_export(
        export: SemanticExporter<'_>,
        declarations: ExportedDeclarations,
    ) -> (InterfaceSemantics, Self) {
        let semantics = InterfaceSemantics::new()
            .with_applications(
                export.substitutions,
                export.trait_applications,
                export.callable_instances,
                export.implementation_instances,
            )
            .with_values(
                export.dependency_contracts,
                export.types,
                export.constant_values,
                export.constant_terms,
            )
            .with_contracts(declarations.constraints, [])
            .with_declarations(
                declarations.signatures,
                declarations.generic_declarations,
                declarations.parameter_defaults,
                declarations.predicate_definitions,
            )
            .with_declared_types(declarations.declared_types)
            .with_type_representations(declarations.type_representations)
            .with_templates(
                declarations.checked_templates,
                declarations.declaration_templates,
                declarations.support_entities,
            );

        let origins = Self {
            substitutions: export.substitution_ids,
            trait_applications: export.trait_application_ids,
            callable_instances: export.callable_instance_ids,
            implementation_instances: export.implementation_instance_ids,
            dependency_contracts: export.dependency_contract_ids,
            types: export.type_ids,
            constant_values: export.constant_value_ids,
            constant_terms: export.constant_term_ids,
        };

        (semantics, origins)
    }

    fn commit(
        &self,
        export: &mut SemanticExporter<'_>,
    ) -> Result<InterfaceSemanticIdRemap, PackageInterfaceExportError> {
        commit_origins(&self.substitutions, |id| export.substitution_id(id))?;

        commit_origins(&self.trait_applications, |id| {
            export.trait_application_id(id)
        })?;

        commit_origins(&self.callable_instances, |id| {
            export.callable_instance_id(id)
        })?;

        commit_origins(&self.implementation_instances, |id| {
            export.implementation_instance_id(id)
        })?;

        commit_origins(&self.dependency_contracts, |id| {
            export.dependency_contract_id(id)
        })?;

        commit_origins(&self.types, |id| export.type_id(id))?;
        commit_origins(&self.constant_values, |id| export.constant_value_id(id))?;
        commit_origins(&self.constant_terms, |id| export.constant_term_id(id))?;

        Ok(InterfaceSemanticIdRemap::new()
            .with_applications(
                remap_values(
                    &self.substitutions,
                    &export.substitution_ids,
                    bray_package_interface::InterfaceSemanticTableKind::GenericSubstitution,
                    bray_package_interface::InterfaceGenericSubstitutionId::raw,
                )?,
                remap_values(
                    &self.trait_applications,
                    &export.trait_application_ids,
                    bray_package_interface::InterfaceSemanticTableKind::TraitApplication,
                    bray_package_interface::InterfaceTraitApplicationId::raw,
                )?,
                remap_values(
                    &self.callable_instances,
                    &export.callable_instance_ids,
                    bray_package_interface::InterfaceSemanticTableKind::CallableInstance,
                    bray_package_interface::InterfaceCallableInstanceId::raw,
                )?,
                remap_values(
                    &self.implementation_instances,
                    &export.implementation_instance_ids,
                    bray_package_interface::InterfaceSemanticTableKind::ImplementationInstance,
                    bray_package_interface::InterfaceImplementationInstanceId::raw,
                )?,
            )
            .with_values(
                remap_values(
                    &self.dependency_contracts,
                    &export.dependency_contract_ids,
                    bray_package_interface::InterfaceSemanticTableKind::DependencyContract,
                    bray_package_interface::InterfaceDependencyContractId::raw,
                )?,
                remap_values(
                    &self.types,
                    &export.type_ids,
                    bray_package_interface::InterfaceSemanticTableKind::Type,
                    bray_package_interface::InterfaceTypeId::raw,
                )?,
                remap_values(
                    &self.constant_values,
                    &export.constant_value_ids,
                    bray_package_interface::InterfaceSemanticTableKind::ConstantValue,
                    bray_package_interface::InterfaceConstantValueId::raw,
                )?,
                remap_values(
                    &self.constant_terms,
                    &export.constant_term_ids,
                    bray_package_interface::InterfaceSemanticTableKind::ConstantTerm,
                    bray_package_interface::InterfaceConstantTermId::raw,
                )?,
            ))
    }
}

fn commit_origins<K, I>(
    origins: &BTreeMap<K, I>,
    mut commit: impl FnMut(K) -> Result<I, PackageInterfaceExportError>,
) -> Result<(), PackageInterfaceExportError>
where
    K: Copy + Ord,
    I: Copy + Ord,
{
    let mut ordered = origins
        .iter()
        .map(|(key, id)| (*id, *key))
        .collect::<Vec<_>>();

    ordered.sort_unstable();

    for (_, key) in ordered {
        commit(key)?;
    }

    Ok(())
}

fn remap_values<K, I>(
    local: &BTreeMap<K, I>,
    package: &BTreeMap<K, I>,
    table: bray_package_interface::InterfaceSemanticTableKind,
    reference: impl Fn(I) -> u32,
) -> Result<Vec<I>, PackageInterfaceExportError>
where
    K: Ord,
    I: Copy + Ord,
{
    let mut ordered = local.iter().map(|(key, id)| (*id, key)).collect::<Vec<_>>();

    ordered.sort_unstable_by_key(|(id, _)| *id);

    ordered
        .into_iter()
        .map(|(local_id, key)| {
            package.get(key).copied().ok_or(
                PackageInterfaceExportError::IncompleteSemanticFragment {
                    declaration: None,
                    table,
                    reference: reference(local_id),
                },
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use bray_package_interface::{InterfaceSemanticTableKind, InterfaceTypeId};

    use super::remap_values;
    use crate::compilation::PackageInterfaceExportError;

    #[test]
    fn missing_package_mapping_retains_local_table_reference() {
        let local = BTreeMap::from([(1_u8, InterfaceTypeId::new(0))]);
        let package = BTreeMap::new();

        let error = remap_values(
            &local,
            &package,
            InterfaceSemanticTableKind::Type,
            InterfaceTypeId::raw,
        )
        .expect_err("missing package mapping must be rejected");

        assert_eq!(
            error,
            PackageInterfaceExportError::IncompleteSemanticFragment {
                declaration: None,
                table: InterfaceSemanticTableKind::Type,
                reference: 0,
            }
        );
    }
}
