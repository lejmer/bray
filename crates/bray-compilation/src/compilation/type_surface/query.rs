use std::collections::BTreeMap;
use std::sync::Arc;

use bray_binder::{BinderFactContext, SymbolFactProvider};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    GenericDeclarationTemplateFact, GenericOwnerId, ImplementationCoherenceFact,
    ImplementationSymbolId, InherentImplementationSymbol, InherentImplementationSymbolId,
    NamedTypeSymbolId, StructSymbol, SymbolFactRequest, TargetFactDependency,
    TypeAssociatedImplementation, TypeAssociatedSurface, UnionSymbol,
};

use super::aggregation::{collect_implementation_members, collect_named_type_members};
use super::diagnostic::lifecycle_slot_diagnostics;
use crate::compilation::Compilation;
use crate::compilation::binder::{self, CompilationBinderFacts};
use crate::compilation::source_graph::{
    source_declaration_module_parts, source_symbol_contribution_gate,
};
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError};

impl Compilation {
    /// Returns the complete declaration-level member surface of one named type.
    pub fn type_associated_surface_result(
        &self,
        subject: NamedTypeSymbolId,
    ) -> Result<Arc<DiagnosticResult<TypeAssociatedSurface>>, FactQueryError> {
        self.type_associated_surface_result_with_cancellation(subject, &self.state.cancellation)
    }

    pub(in crate::compilation) fn type_associated_surface_result_with_cancellation(
        &self,
        subject: NamedTypeSymbolId,
        cancellation: &CancellationToken,
    ) -> Result<Arc<DiagnosticResult<TypeAssociatedSurface>>, FactQueryError> {
        let cell = self.state.type_associated_surfaces.cell(subject)?;

        let result = cell.get_or_compute(
            &self.state.fact_runtime,
            CompilationFactKey::TypeAssociatedSurface(subject),
            cancellation,
            || {
                self.compute_type_associated_surface(subject, cancellation)
                    .map(Arc::new)
            },
        )?;

        // Exact results are Arc-backed so callers do not retain the cache-cell map lock.
        Ok(Arc::clone(result))
    }

    fn compute_type_associated_surface(
        &self,
        subject: NamedTypeSymbolId,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<TypeAssociatedSurface>, FactQueryError> {
        let facts = self.binder_facts(cancellation)?;

        let record = named_type_record(&facts, subject)?;

        let generic_owner = GenericOwnerId::try_new(subject.into_any())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let generic = facts
            .symbol_fact(SymbolFactRequest::<GenericDeclarationTemplateFact>::new(
                generic_owner,
            ))
            .map_err(binder::binder_fact_error)?;

        let (direct_members, direct_lifecycle) = collect_named_type_members(&facts, record)?;

        let implementation_ids = self
            .type_associated_implementation_index(cancellation)?
            .implementations(subject);

        let source_graph = self.product_source_graph()?;

        let source_module_parts = source_declaration_module_parts(source_graph.declarations());

        let mut implementations = Vec::new();
        let mut diagnostics = DiagnosticBag::new();

        diagnostics.add_range(generic.diagnostics().iter().cloned());

        for &implementation in implementation_ids {
            cancellation.check()?;

            let generic_owner = GenericOwnerId::try_new(implementation.into())
                .ok_or(FactQueryError::InfrastructureFailure)?;

            let implementation_generic = facts
                .symbol_fact(SymbolFactRequest::<GenericDeclarationTemplateFact>::new(
                    generic_owner,
                ))
                .map_err(binder::binder_fact_error)?;

            diagnostics.add_range(implementation_generic.diagnostics().iter().cloned());

            let (target_dependencies, implementation_diagnostics) =
                implementation_metadata(&facts, implementation, &source_module_parts)?;

            diagnostics.add_range(implementation_diagnostics.iter().cloned());

            let record = inherent_implementation_record(&facts, implementation)?;

            let (members, lifecycle_members) = collect_implementation_members(&facts, record)?;

            // The surface independently retains this immutable generic template publication.
            implementations.push(TypeAssociatedImplementation::new(
                implementation,
                implementation_generic.value().clone(),
                target_dependencies,
                members,
                lifecycle_members,
            ));
        }

        // The surface independently retains the named type's immutable generic template.
        let surface = TypeAssociatedSurface::try_new(
            subject,
            generic.value().clone(),
            direct_members,
            direct_lifecycle,
            implementations,
        )
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

        diagnostics.add_range(
            lifecycle_slot_diagnostics(&facts, &surface)?
                .iter()
                .cloned(),
        );

        Ok(DiagnosticResult::new(surface, diagnostics))
    }
}

pub(super) enum NamedTypeRecord<'a> {
    Struct(&'a StructSymbol),
    Union(&'a UnionSymbol),
}

fn named_type_record<'facts>(
    facts: &'facts CompilationBinderFacts<'_>,
    subject: NamedTypeSymbolId,
) -> Result<NamedTypeRecord<'facts>, FactQueryError> {
    let imported = facts
        .imported_symbols()
        .map_err(binder::binder_fact_error)?;

    match subject {
        NamedTypeSymbolId::Struct(id) => facts
            .symbols()
            .structure(id)
            .or_else(|| imported.and_then(|symbols| symbols.structure(id)))
            .map(NamedTypeRecord::Struct),
        NamedTypeSymbolId::Union(id) => facts
            .symbols()
            .union(id)
            .or_else(|| imported.and_then(|symbols| symbols.union(id)))
            .map(NamedTypeRecord::Union),
    }
    .ok_or(FactQueryError::InfrastructureFailure)
}

fn inherent_implementation_record<'facts>(
    facts: &'facts CompilationBinderFacts<'_>,
    implementation: InherentImplementationSymbolId,
) -> Result<&'facts InherentImplementationSymbol, FactQueryError> {
    if let Some(record) = facts.symbols().inherent_implementation(implementation) {
        return Ok(record);
    }

    facts
        .imported_symbols()
        .map_err(binder::binder_fact_error)?
        .and_then(|symbols| symbols.inherent_implementation(implementation))
        .ok_or(FactQueryError::InfrastructureFailure)
}

fn implementation_metadata(
    facts: &CompilationBinderFacts<'_>,
    implementation: InherentImplementationSymbolId,
    source_module_parts: &BTreeMap<
        bray_declarations::DeclarationId,
        bray_declarations::ModulePartId,
    >,
) -> Result<(Vec<TargetFactDependency>, DiagnosticBag), FactQueryError> {
    if let Some(address) = facts
        .imported_fact_address(implementation.into())
        .map_err(binder::binder_fact_error)?
    {
        let imported =
            binder::imported_implementation(facts, address).map_err(binder::binder_fact_error)?;

        // The contribution retains imported diagnostics beyond the exact fact result.
        return Ok((
            imported.value().target_dependencies().to_vec(),
            imported.diagnostics().clone(),
        ));
    }

    let coherence = facts
        .symbol_fact(SymbolFactRequest::<ImplementationCoherenceFact>::new(
            ImplementationSymbolId::from(implementation),
        ))
        .map_err(binder::binder_fact_error)?;

    let source_graph = facts.compilation().product_source_graph()?;

    let dependencies = source_symbol_contribution_gate(
        source_graph,
        facts.symbols(),
        source_module_parts,
        implementation.into(),
    )
    .into_iter()
    .flat_map(|gate| gate.dependencies().iter().cloned())
    .collect();

    Ok((dependencies, coherence.diagnostics().clone()))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_compiler_known::CompilerKnownDeclarationKey;
    use bray_symbols::{
        AnySymbolId, MemberLookupResult, SymbolOrigin, TypeAssociatedLifecycleSlot,
        TypeAssociatedMemberOrigin, UnionSymbolId,
    };

    use crate::test_support::{
        compilation, encoded_semantic_dependency, package_identity, source_input,
    };
    use crate::{Compilation, CompilationRequest};

    const ASSOCIATED_MEMBERS: &str = r#"module app;

struct Holder
{
    value: bool;

    construct(value: bool) -> Self
    {
    }

    func shared()
    {
    }
}

impl Holder
{
    func shared()
    {
    }

    type Item = bool;

    finalize()
    {
    }
}
"#;

    #[test]
    fn source_surfaces_retain_members_lifecycle_conflicts_and_implementation_metadata() {
        let compilation = compilation(ASSOCIATED_MEMBERS);
        let subject = source_subject(&compilation);

        let surface = compilation
            .type_associated_surface_result(subject)
            .unwrap_or_else(|error| panic!("type-associated surface must build: {error:?}"));

        assert!(surface.diagnostics().is_empty());

        assert_eq!(
            surface.value().generic().owner().symbol(),
            subject.into_any()
        );

        assert_eq!(surface.value().implementations().len(), 1);

        let MemberLookupResult::Ambiguous(shared) = surface.value().lookup("shared") else {
            panic!("direct and inherent declarations must remain conflicting candidates");
        };

        assert_eq!(shared.len(), 2);

        let origins = shared
            .iter()
            .filter_map(|member| surface.value().member(*member))
            .map(|member| member.origin())
            .collect::<Vec<_>>();

        assert!(matches!(
            origins.first(),
            Some(TypeAssociatedMemberOrigin::Direct)
        ));

        assert!(origins.iter().any(|origin| matches!(
            origin,
            TypeAssociatedMemberOrigin::InherentImplementation(_)
        )));

        let slots = surface
            .value()
            .lifecycle_members()
            .iter()
            .map(|member| member.slot())
            .collect::<Vec<_>>();

        assert_eq!(
            slots,
            [
                TypeAssociatedLifecycleSlot::PrimaryConstructor,
                TypeAssociatedLifecycleSlot::Finalizer,
            ]
        );
    }

    #[test]
    fn source_surfaces_retain_target_dependencies_from_contributing_modules() {
        let compilation = compilation(
            r#"@target(target.scalar.u64)
module app
{
    impl Holder
    {
        type Item = bool;
    }
}

module app
{
    struct Holder
    {
    }
}
"#,
        );

        let subject = source_subject(&compilation);

        let surface = compilation
            .type_associated_surface_result(subject)
            .unwrap_or_else(|error| panic!("type-associated surface must build: {error:?}"));

        let [implementation] = surface.value().implementations() else {
            panic!("enabled inherent implementation must contribute once");
        };

        let [dependency] = implementation.target_dependencies() else {
            panic!("target-gated implementation must retain one dependency");
        };

        assert_eq!(
            compilation
                .available_compiler_known_symbols()
                .provider()
                .symbol_target_fact(dependency.fact()),
            Some(bray_target::TargetFactKind::ScalarU64)
        );
    }

    #[test]
    fn disabled_implementation_contributions_do_not_enter_type_surfaces() {
        let compilation = compilation(
            r#"@target(false)
module app
{
    impl Holder
    {
        type Item = bool;
    }
}

module app
{
    struct Holder
    {
    }
}
"#,
        );

        let subject = source_subject(&compilation);

        let surface = compilation
            .type_associated_surface_result(subject)
            .unwrap_or_else(|error| panic!("type-associated surface must build: {error:?}"));

        assert!(surface.value().implementations().is_empty());
        assert_eq!(surface.value().lookup("Item"), MemberLookupResult::NotFound);
    }

    #[test]
    fn recovered_unnamed_members_remain_typed_but_do_not_enter_name_lookup() {
        let compilation = compilation(
            r#"module app;

struct Holder
{
    func ()
    {
    }
}
"#,
        );

        let subject = source_subject(&compilation);

        let surface = compilation
            .type_associated_surface_result(subject)
            .unwrap_or_else(|error| panic!("type-associated surface must build: {error:?}"));

        let members = surface.value().callable_members().collect::<Vec<_>>();

        let [member] = members.as_slice() else {
            panic!("recovered callable member must remain in the typed surface");
        };

        let member = surface
            .value()
            .member((*member).into())
            .unwrap_or_else(|| panic!("typed member identity must resolve"));

        assert!(member.is_recovered());
        assert_eq!(member.entry(), None);
        assert_eq!(surface.value().lookup(""), MemberLookupResult::NotFound);
    }

    #[test]
    fn ordinary_lookup_conflicts_follow_mixed_kind_source_order() {
        let compilation = compilation(
            r#"module app;

struct Holder
{
    func shared()
    {
    }

    shared: bool;
}
"#,
        );

        let subject = source_subject(&compilation);

        let surface = compilation
            .type_associated_surface_result(subject)
            .unwrap_or_else(|error| panic!("type-associated surface must build: {error:?}"));

        let MemberLookupResult::Ambiguous(shared) = surface.value().lookup("shared") else {
            panic!("mixed member kinds with the same name must remain conflicting candidates");
        };

        assert!(matches!(
            shared.as_ref(),
            [
                AnySymbolId::TypeCallableMember(_),
                AnySymbolId::StructField(_)
            ]
        ));
    }

    #[test]
    fn unrelated_implementation_diagnostics_do_not_enter_requested_surfaces() {
        let compilation = compilation(
            r#"module app;

struct Holder
{
}

impl Missing
{
}
"#,
        );

        let subject = source_subject(&compilation);

        let surface = compilation
            .type_associated_surface_result(subject)
            .unwrap_or_else(|error| panic!("type-associated surface must build: {error:?}"));

        assert!(surface.diagnostics().is_empty());
        assert!(surface.value().implementations().is_empty());
    }

    #[test]
    fn compiler_known_surfaces_retain_direct_union_variants() {
        let compilation = compilation("module app;");

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("symbol graph must be available: {error:?}"));

        let key = CompilerKnownDeclarationKey::try_new("Ordering")
            .unwrap_or_else(|| panic!("Ordering must be a valid declaration key"));

        let subject = symbols
            .compiler_known_provider()
            .declaration_symbol::<UnionSymbolId>(&key)
            .unwrap_or_else(|| panic!("compiler-known Ordering must be available"));

        let surface = compilation
            .type_associated_surface_result(subject.into())
            .unwrap_or_else(|error| panic!("type-associated surface must build: {error:?}"));

        assert!(surface.diagnostics().is_empty());

        assert!(matches!(
            surface.value().lookup("Less"),
            MemberLookupResult::Found(_)
        ));

        assert!(matches!(
            surface.value().lookup("Equal"),
            MemberLookupResult::Found(_)
        ));

        assert!(matches!(
            surface.value().lookup("Greater"),
            MemberLookupResult::Found(_)
        ));
    }

    #[test]
    fn imported_named_types_publish_type_associated_surfaces() {
        let interface = bray_package_interface::test_support::encoded_semantic_test_interface();

        let request =
            CompilationRequest::new(package_identity(), vec![source_input("module app;", 0)])
                .with_dependency_interfaces([encoded_semantic_dependency(&interface)]);

        let compilation = Compilation::load(request)
            .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"));

        let skeleton = compilation
            .imported_symbol_skeleton_result()
            .unwrap_or_else(|error| panic!("imported skeleton query must complete: {error:?}"));

        let skeleton = skeleton
            .value()
            .as_ref()
            .unwrap_or_else(|| panic!("test dependency must publish imported symbols"));

        let subject = skeleton
            .structures()
            .first()
            .unwrap_or_else(|| panic!("test dependency must publish an imported structure"))
            .id()
            .into();

        let surface = compilation
            .type_associated_surface_result(subject)
            .unwrap_or_else(|error| panic!("type-associated surface must build: {error:?}"));

        assert!(surface.diagnostics().is_empty());
        assert_eq!(surface.value().subject(), subject);

        assert_eq!(
            surface.value().generic().owner().symbol(),
            subject.into_any()
        );

        assert_eq!(surface.value().fields().count(), 0);
        assert_eq!(surface.value().variants().count(), 0);
        assert_eq!(surface.value().callable_members().count(), 2);
        assert_eq!(surface.value().constructors().count(), 0);
        assert_eq!(surface.value().constants().count(), 0);
        assert_eq!(surface.value().predicates().count(), 0);
        assert_eq!(surface.value().type_members().count(), 0);
        assert_eq!(surface.value().callable_overloads().count(), 0);
        assert_eq!(surface.value().implementations().len(), 1);

        let MemberLookupResult::Found(direct) = surface.value().lookup("direct") else {
            panic!("imported direct member must enter ordinary lookup");
        };

        let direct = surface
            .value()
            .member(direct)
            .unwrap_or_else(|| panic!("imported direct member must remain in the typed surface"));

        assert_eq!(direct.origin(), TypeAssociatedMemberOrigin::Direct);

        let MemberLookupResult::Found(inherent) = surface.value().lookup("extension") else {
            panic!("imported inherent member must enter ordinary lookup");
        };

        let inherent = surface
            .value()
            .member(inherent)
            .unwrap_or_else(|| panic!("imported inherent member must remain in the typed surface"));

        assert!(matches!(
            inherent.origin(),
            TypeAssociatedMemberOrigin::InherentImplementation(_)
        ));
    }

    #[test]
    fn exact_surface_queries_publish_once_across_concurrent_demand() {
        let compilation = compilation(ASSOCIATED_MEMBERS);
        let subject = source_subject(&compilation);

        let surfaces = std::thread::scope(|scope| {
            let first = scope.spawn(|| compilation.type_associated_surface_result(subject));
            let second = scope.spawn(|| compilation.type_associated_surface_result(subject));

            [first, second].map(|thread| {
                thread
                    .join()
                    .unwrap_or_else(|_| panic!("surface query thread must not panic"))
                    .unwrap_or_else(|error| panic!("surface query must complete: {error:?}"))
            })
        });

        assert!(Arc::ptr_eq(&surfaces[0], &surfaces[1]));
    }

    fn source_subject(compilation: &crate::Compilation) -> bray_symbols::NamedTypeSymbolId {
        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("symbol graph must be available: {error:?}"));

        let Some(subject) = symbols
            .structures()
            .iter()
            .find(|symbol| symbol.origin() == SymbolOrigin::Source)
        else {
            panic!("fixture must contain one source structure");
        };

        subject.id().into()
    }
}
