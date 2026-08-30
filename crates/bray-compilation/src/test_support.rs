// rust-style: allow(module-too-large, reason = "test-only compiler fixtures and query-key helpers share one crate-wide support surface")

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

use bray_bound_tree::{
    BoundSourceAnchor, BoundUnitKey, CheckedSemanticSelections, SemanticSelection,
    SemanticSelectionEntry,
};
use bray_declarations::{DeclarationId, discover_source_unit_declarations};
use bray_diagnostics::{DiagnosticBag, DiagnosticKind};
use bray_package_interface::{
    InterfaceLanguageRevision, InterfaceValidationPolicy,
    test_support::EncodedSemanticTestInterface,
};
use bray_parser::parse_source_unit;
use bray_source::{
    SourceId, SourceIdentity, SourceInput, SourceOrigin, SourceSnapshot, SourceVersion,
};
use bray_symbols::{
    AnySymbolId, FunctionSymbolId, ModulePathKey, PackageIdentity, PackageVersion, SymbolGraph,
    SymbolKey, SymbolKind, SymbolOrigin, SymbolRootKey,
};

use crate::fact::{
    CompilationFactKey, FactCellTestEvent, FactCellTestObserver, FactEvaluationTestObserver,
};
use crate::{
    Compilation, CompilationOptions, CompilationRequest, DependencyInterfaceInput,
    PackageInterfaceExportRequest, WorkerBudget,
};

const FACT_OBSERVATION_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) const RUNTIME_MEMORY_SOURCE: &str = r#"trusted internal module std.runtime.memory;

trusted func allocate(pos bytes: usize, pos align: usize) -> RawPointer<u8>
{
    return core.memory.null<u8>();
}

trusted func deallocate(
    pos pointer: RawPointer<u8>,
    pos bytes: usize,
    pos align: usize,
) -> unit
{
}
"#;

pub(crate) const RUNTIME_TEXT_SOURCE: &str = r#"trusted internal module std.runtime.text;

internal struct OwnedText
{
    data: RawPointer<u8>;
    length: usize;
    owner: RawPointer<u8>;
}

internal struct ValidatedText
{
    valid: bool;
    text: OwnedText;
}

trusted internal func scalar_count(pos data: RawPointer<u8>, pos length: usize) -> usize
{
    return 0;
}

trusted internal func equals(
    pos left_data: RawPointer<u8>,
    pos left_length: usize,
    pos right_data: RawPointer<u8>,
    pos right_length: usize,
) -> bool
{
    return false;
}

trusted internal func scalar_at(pos data: RawPointer<u8>, pos length: usize, pos index: usize) -> u32?
{
    return none;
}

trusted internal func scalar_slice(
    pos data: RawPointer<u8>,
    pos length: usize,
    pos start: usize,
    pos end: usize,
) -> OwnedText
{
    return empty_text();
}

trusted internal func from_utf8(pos data: RawPointer<u8>, pos length: usize) -> ValidatedText
{
    return
    {
        valid = true,
        text = empty_text(),
    };
}

internal func empty_text() -> OwnedText
{
    let empty: RawPointer<u8> = core.memory.null<u8>();

    return
    {
        data = empty,
        length = 0,
        owner = empty,
    };
}
"#;

pub(crate) const RUNTIME_CHARACTER_SOURCE: &str = r#"trusted internal module std.runtime.character;

trusted internal func scalar_value(pos value: u32) -> u32
{
    return value;
}

trusted internal func from_scalar_value(pos value: u32) -> u32?
{
    let mut result: u32? = none;

    result = value;

    return result;
}

trusted internal func utf8_length(pos value: u32) -> usize
{
    return 1;
}

trusted internal func utf8_byte(pos value: u32, pos index: usize) -> u8
{
    return 0;
}

trusted internal func is_alphabetic(pos value: u32) -> bool
{
    return false;
}

trusted internal func is_numeric(pos value: u32) -> bool
{
    return false;
}

trusted internal func is_whitespace(pos value: u32) -> bool
{
    return false;
}
"#;

static RUNTIME_STANDARD_LIBRARY_DEPENDENCIES: OnceLock<
    Mutex<HashMap<crate::SelectedTarget, DependencyInterfaceInput>>,
> = OnceLock::new();

pub(crate) struct FactTestGate {
    held_event: FactCellTestEvent,
    shared: Arc<(Mutex<FactTestGateState>, Condvar)>,
}

#[derive(Clone, Default)]
pub(crate) struct FactEvaluationLog {
    keys: Arc<Mutex<Vec<CompilationFactKey>>>,
}

impl FactEvaluationLog {
    pub(crate) fn observer(&self) -> FactEvaluationTestObserver {
        let keys = Arc::clone(&self.keys);

        FactEvaluationTestObserver::new(move |key| {
            keys.lock()
                .unwrap_or_else(|_| panic!("fact evaluation log must remain available"))
                .push(key.clone());
        })
    }

    pub(crate) fn keys(&self) -> BTreeSet<CompilationFactKey> {
        self.keys
            .lock()
            .unwrap_or_else(|_| panic!("fact evaluation log must remain available"))
            .iter()
            .cloned()
            .collect()
    }
}

#[derive(Default)]
struct FactTestGateState {
    computing: usize,
    waiting: usize,
    computed: usize,
    released: bool,
}

impl FactTestGate {
    pub(crate) fn holding(held_event: FactCellTestEvent) -> Self {
        Self {
            held_event,
            shared: Arc::new((Mutex::new(FactTestGateState::default()), Condvar::new())),
        }
    }

    pub(crate) fn observer(&self) -> FactCellTestObserver {
        let held_event = self.held_event;
        let shared = Arc::clone(&self.shared);

        FactCellTestObserver::new(move |event| {
            let (state, changed) = &*shared;

            let mut state = state
                .lock()
                .unwrap_or_else(|_| panic!("fact test gate must remain available"));

            state.record(event);
            changed.notify_all();

            while event == held_event && !state.released {
                state = changed
                    .wait(state)
                    .unwrap_or_else(|_| panic!("fact test gate must remain available"));
            }
        })
    }

    pub(crate) fn wait_until_observed(&self, event: FactCellTestEvent, count: usize) {
        let (state, changed) = &*self.shared;

        let state = state
            .lock()
            .unwrap_or_else(|_| panic!("fact test gate must remain available"));

        let waited = changed
            .wait_timeout_while(state, FACT_OBSERVATION_TIMEOUT, |state| {
                state.count(event) < count
            })
            .unwrap_or_else(|_| panic!("fact test gate must remain available"));

        if waited.0.count(event) < count {
            drop(waited.0);
            self.release();

            panic!("fact test event was not observed before the timeout");
        }
    }

    pub(crate) fn release(&self) {
        let (state, changed) = &*self.shared;

        let mut state = state
            .lock()
            .unwrap_or_else(|_| panic!("fact test gate must remain available"));

        state.released = true;
        changed.notify_all();
    }
}

impl FactTestGateState {
    fn record(&mut self, event: FactCellTestEvent) {
        match event {
            FactCellTestEvent::Computing => self.computing += 1,
            FactCellTestEvent::Waiting => self.waiting += 1,
            FactCellTestEvent::Computed => self.computed += 1,
        }
    }

    const fn count(&self, event: FactCellTestEvent) -> usize {
        match event {
            FactCellTestEvent::Computing => self.computing,
            FactCellTestEvent::Waiting => self.waiting,
            FactCellTestEvent::Computed => self.computed,
        }
    }
}

pub(crate) fn package_identity() -> PackageIdentity {
    match PackageIdentity::try_new("test.package") {
        Some(identity) => identity,
        None => panic!("test package identity must be valid"),
    }
}

pub(crate) fn package_version() -> PackageVersion {
    PackageVersion::try_new("1.0.0").unwrap_or_else(|| panic!("test package version must be valid"))
}

pub(crate) fn source_input(text: &str, version: u32) -> SourceInput {
    SourceInput::virtual_text(
        SourceIdentity::new(version),
        format!("source-{version}"),
        SourceVersion::new(u64::from(version)),
        text,
    )
}

pub(crate) fn runtime_standard_library_dependency(
    target: &crate::SelectedTarget,
) -> DependencyInterfaceInput {
    let dependencies = RUNTIME_STANDARD_LIBRARY_DEPENDENCIES.get_or_init(Mutex::default);

    let mut dependencies = dependencies
        .lock()
        .unwrap_or_else(|_| panic!("runtime standard-library fixture cache must remain available"));

    if let Some(dependency) = dependencies.get(target) {
        return dependency.clone();
    }

    let dependency = build_runtime_standard_library_dependency(target.clone());

    dependencies.insert(target.clone(), dependency.clone());

    dependency
}

fn build_runtime_standard_library_dependency(
    target: crate::SelectedTarget,
) -> DependencyInterfaceInput {
    let package =
        PackageIdentity::try_new(bray_standard_library::PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY)
            .unwrap_or_else(|| panic!("standard-library package identity must be valid"));

    let product = bray_package_interface::InterfaceProductIdentity::try_new(
        bray_standard_library::PUBLIC_STANDARD_LIBRARY_PRODUCT_IDENTITY,
    )
    .unwrap_or_else(|| panic!("standard-library product identity must be valid"));

    let identity = bray_package_interface::PackageInterfaceIdentity::try_new(
        package.clone(),
        package_version(),
        product.clone(),
        bray_package_interface::InterfaceProductKind::Library,
        bray_standard_library::PUBLIC_STANDARD_LIBRARY_SURFACE_IDENTITY,
    )
    .unwrap_or_else(|| panic!("standard-library interface identity must be valid"));

    let export = PackageInterfaceExportRequest::new(identity, InterfaceLanguageRevision::new(0));

    let request = CompilationRequest::with_options(
        package.clone(),
        [
            RUNTIME_MEMORY_SOURCE,
            RUNTIME_TEXT_SOURCE,
            RUNTIME_CHARACTER_SOURCE,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, source)| {
            source_input(
                source,
                u32::try_from(index)
                    .unwrap_or_else(|_| panic!("runtime source identity must fit u32")),
            )
        })
        .collect(),
        CompilationOptions::new(
            WorkerBudget::serial(),
            bray_symbols::ProductKind::Library,
            target,
        ),
    )
    .with_standard_library_source_authority()
    .with_package_interface_export(export);

    let compilation = Compilation::load(request)
        .unwrap_or_else(|error| panic!("runtime standard-library fixture must load: {error:?}"));

    assert!(
        compilation.check_diagnostics().is_empty(),
        "runtime standard-library fixture diagnostics: {:#?}",
        compilation.check_diagnostics(),
    );

    let bundle = compilation
        .package_interface_export_bundle()
        .and_then(|bundle| bundle.as_ref().ok())
        .unwrap_or_else(|| panic!("runtime standard-library fixture must export"));

    let interface =
        bray_package_interface::encode_package_interface(bundle).unwrap_or_else(|error| {
            panic!("runtime standard-library interface must encode: {error:?}")
        });

    let policy = InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0));

    let validated =
        bray_package_interface::ValidatedPackageInterface::try_new(interface.bytes(), policy)
            .unwrap_or_else(|error| {
                panic!("runtime standard-library interface must validate: {error:?}")
            });

    let implementation = bray_package_interface::PackageImplementationArtifact::try_new(
        &validated,
        bundle.surface(),
        bundle.semantics(),
        bundle.implementation_configuration().clone(),
        [],
        bundle.executable_templates().iter().cloned(),
        [],
        [],
        bray_package_interface::InterfaceValidationLimits::default(),
    )
    .unwrap_or_else(|error| {
        panic!("runtime standard-library implementation must encode: {error:?}")
    });

    DependencyInterfaceInput::new(
        package,
        product,
        "std.brayi",
        interface.shared_bytes(),
        policy,
    )
    .with_implementation_artifact("std.brayimpl", Arc::new(implementation))
}

pub(crate) fn diagnostic_kinds(diagnostics: &DiagnosticBag) -> Vec<DiagnosticKind> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.kind())
        .collect()
}

pub(crate) fn only_call_selection(
    selections: &CheckedSemanticSelections,
) -> &SemanticSelectionEntry {
    let mut calls = selections
        .entries()
        .iter()
        .filter(|entry| matches!(entry.selection(), SemanticSelection::Call(_)));

    let Some(call) = calls.next() else {
        panic!("semantic selections must contain one selected call: {selections:?}");
    };

    assert!(
        calls.next().is_none(),
        "semantic selections must contain only one selected call"
    );

    call
}

pub(crate) fn compilation(source: &str) -> Compilation {
    compilation_with_options(source, CompilationOptions::default())
}

pub(crate) fn compilation_with_target_profile(
    source: &str,
    profile: bray_target::TargetProfile,
) -> Compilation {
    compilation_with_sources_and_target_profile(&[source], profile)
}

pub(crate) fn compilation_with_sources_and_target_profile(
    sources: &[&str],
    profile: bray_target::TargetProfile,
) -> Compilation {
    let baseline = crate::SelectedTarget::baseline();

    let options = CompilationOptions::new(
        WorkerBudget::serial(),
        bray_symbols::ProductKind::Library,
        crate::SelectedTarget::new(profile, baseline.runtime_abi()),
    );

    compilation_with_sources_and_options(sources, options)
}

pub(crate) fn compilation_with_target_operations(
    source: &str,
    raw_memory: bool,
    allocation: bool,
) -> Compilation {
    let baseline = crate::SelectedTarget::baseline();
    let profile = baseline.profile();
    let baseline_properties = profile.properties();

    let properties = bray_target::TargetProperties::new(
        baseline_properties.identity().clone(),
        baseline_properties.scalars(),
        baseline_properties.atomics(),
        baseline_properties.abis(),
        baseline_properties.c_abi(),
        baseline_properties.address_spaces(),
        baseline_properties.alignments(),
        bray_target::TargetOperationSupport::new(raw_memory, allocation, raw_memory),
    );

    let profile = bray_target::TargetProfile::try_new(
        profile.identity().clone(),
        profile.machine().clone(),
        properties,
    )
    .unwrap_or_else(|error| panic!("test target profile must be valid: {error:?}"));

    let options = CompilationOptions::new(
        WorkerBudget::serial(),
        bray_symbols::ProductKind::Library,
        crate::SelectedTarget::new(profile, baseline.runtime_abi()),
    );

    compilation_with_options(source, options)
}

pub(crate) fn compilation_with_options(source: &str, options: CompilationOptions) -> Compilation {
    compilation_with_sources_and_options(&[source], options)
}

fn compilation_with_sources_and_options(
    sources: &[&str],
    options: CompilationOptions,
) -> Compilation {
    let request =
        CompilationRequest::with_options(package_identity(), source_inputs(sources), options);

    match Compilation::load(request) {
        Ok(compilation) => compilation,
        Err(error) => panic!("test compilation must load: {error:?}"),
    }
}

pub(crate) fn compilation_with_dependencies(
    source: &str,
    dependencies: impl IntoIterator<Item = DependencyInterfaceInput>,
) -> Compilation {
    let request = CompilationRequest::new(package_identity(), vec![source_input(source, 0)])
        .with_dependency_interfaces(dependencies);

    match Compilation::load(request) {
        Ok(compilation) => compilation,
        Err(error) => panic!("test compilation must load: {error:?}"),
    }
}

pub(crate) fn encoded_semantic_dependency(
    fixture: &EncodedSemanticTestInterface,
) -> DependencyInterfaceInput {
    DependencyInterfaceInput::new(
        fixture.package.clone(),
        fixture.product.clone(),
        "dependency.brayi",
        Arc::<[u8]>::from(fixture.bytes.clone()),
        InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
    )
}

pub(crate) fn compilation_with_sources_and_worker_budget(
    sources: &[&str],
    worker_budget: WorkerBudget,
) -> Compilation {
    compilation_with_sources_product_and_worker_budget(
        sources,
        bray_symbols::ProductKind::Library,
        worker_budget,
    )
}

pub(crate) fn compilation_with_product(
    source: &str,
    product_kind: bray_symbols::ProductKind,
) -> Compilation {
    compilation_with_sources_product_and_worker_budget(
        &[source],
        product_kind,
        WorkerBudget::serial(),
    )
}

pub(crate) fn compilation_with_sources_product_and_worker_budget(
    sources: &[&str],
    product_kind: bray_symbols::ProductKind,
    worker_budget: WorkerBudget,
) -> Compilation {
    let request = CompilationRequest::with_options(
        package_identity(),
        source_inputs(sources),
        CompilationOptions::new(
            worker_budget,
            product_kind,
            crate::SelectedTarget::baseline(),
        ),
    );

    match Compilation::load(request) {
        Ok(compilation) => compilation,
        Err(error) => panic!("test compilation must load: {error:?}"),
    }
}

fn source_inputs(sources: &[&str]) -> Vec<SourceInput> {
    sources
        .iter()
        .copied()
        .enumerate()
        .map(|(index, source)| match u32::try_from(index) {
            Ok(version) => source_input(source, version),
            Err(_) => panic!("test source index must fit in u32"),
        })
        .collect()
}

pub(crate) fn source_callable_body_key(compilation: &Compilation) -> BoundUnitKey {
    let symbols = match compilation.symbol_graph() {
        Ok(symbols) => symbols,
        Err(error) => panic!("test symbol graph must build: {error:?}"),
    };

    source_callable_body_key_from_symbols(compilation, symbols)
}

pub(crate) fn source_function_body_key(compilation: &Compilation, name: &str) -> BoundUnitKey {
    let symbols = compilation
        .symbol_graph()
        .unwrap_or_else(|error| panic!("symbol graph must be available: {error:?}"));

    let function = source_function(compilation, name);

    source_symbol_body_key(compilation, symbols, function.into())
}

pub(crate) fn source_trait_callable_fulfillment_body_key(
    compilation: &Compilation,
    name: &str,
) -> BoundUnitKey {
    let symbols = compilation
        .symbol_graph()
        .unwrap_or_else(|error| panic!("symbol graph must be available: {error:?}"));

    let member = symbols
        .trait_callable_fulfillments()
        .iter()
        .find(|member| {
            member.origin() == SymbolOrigin::Source
                && symbols
                    .member_name(member.id().into())
                    .is_some_and(|member_name| member_name.as_str() == name)
        })
        .unwrap_or_else(|| panic!("source trait callable fulfillment {name} must exist"));

    source_symbol_body_key(compilation, symbols, member.id().into())
}

pub(crate) fn source_trait_callable_member_body_key(
    compilation: &Compilation,
    name: &str,
) -> BoundUnitKey {
    let symbols = compilation
        .symbol_graph()
        .unwrap_or_else(|error| panic!("symbol graph must be available: {error:?}"));

    let member = symbols
        .trait_callable_members()
        .iter()
        .find(|member| {
            member.origin() == SymbolOrigin::Source
                && symbols
                    .member_name(member.id().into())
                    .is_some_and(|member_name| member_name.as_str() == name)
        })
        .unwrap_or_else(|| panic!("source trait callable member {name} must exist"));

    source_symbol_body_key(compilation, symbols, member.id().into())
}

pub(crate) fn source_named_trait_callable_fulfillment_body_key(
    compilation: &Compilation,
    implementation_name: &str,
    member_name: &str,
) -> BoundUnitKey {
    let symbols = compilation
        .symbol_graph()
        .unwrap_or_else(|error| panic!("symbol graph must be available: {error:?}"));

    let implementation = symbols
        .named_trait_implementations()
        .iter()
        .find(|implementation| {
            implementation.origin() == SymbolOrigin::Source
                && symbols
                    .member_name(implementation.id().into())
                    .is_some_and(|name| name.as_str() == implementation_name)
        })
        .unwrap_or_else(|| {
            panic!("source named trait implementation {implementation_name} must exist")
        });

    let member = symbols
        .trait_callable_fulfillments()
        .iter()
        .find(|member| {
            member.origin() == SymbolOrigin::Source
                && symbols.containing_symbol(member.id().into()) == Some(implementation.id().into())
                && symbols
                    .member_name(member.id().into())
                    .is_some_and(|name| name.as_str() == member_name)
        })
        .unwrap_or_else(|| {
            panic!(
                "source trait callable fulfillment {implementation_name}.{member_name} must exist"
            )
        });

    source_symbol_body_key(compilation, symbols, member.id().into())
}

pub(crate) fn source_type_callable_member_body_key(
    compilation: &Compilation,
    name: &str,
) -> BoundUnitKey {
    let symbols = compilation
        .symbol_graph()
        .unwrap_or_else(|error| panic!("symbol graph must be available: {error:?}"));

    let member = symbols
        .type_callable_members()
        .iter()
        .find(|member| {
            member.origin() == SymbolOrigin::Source
                && symbols
                    .member_name(member.id().into())
                    .is_some_and(|member_name| member_name.as_str() == name)
        })
        .unwrap_or_else(|| panic!("source type callable member {name} must exist"));

    source_symbol_body_key(compilation, symbols, member.id().into())
}

fn source_symbol_body_key(
    compilation: &Compilation,
    symbols: &SymbolGraph,
    symbol: AnySymbolId,
) -> BoundUnitKey {
    let Some(key) = symbols.symbol_key(symbol) else {
        panic!("source callable must retain its symbol key");
    };

    let Some(anchor) = symbols.declaration_syntax_anchor(symbol) else {
        panic!("source callable must retain its syntax anchor");
    };

    let Some(source) = compilation.source(anchor.source_id()) else {
        panic!("callable source must be loaded");
    };

    valid_key(BoundUnitKey::callable_body(
        key.clone(),
        BoundSourceAnchor::new(anchor, source.version()),
    ))
}

pub(crate) fn source_function(compilation: &Compilation, name: &str) -> FunctionSymbolId {
    let symbols = compilation
        .symbol_graph()
        .unwrap_or_else(|error| panic!("symbol graph must be available: {error:?}"));

    symbols
        .functions()
        .iter()
        .find(|function| {
            function.origin() == SymbolOrigin::Source
                && symbols
                    .member_name(function.id().into())
                    .is_some_and(|member_name| member_name.as_str() == name)
        })
        .map(bray_symbols::FunctionSymbol::id)
        .unwrap_or_else(|| panic!("source function {name} must exist"))
}

pub(crate) fn source_callable_body_key_from_symbols(
    compilation: &Compilation,
    symbols: &SymbolGraph,
) -> BoundUnitKey {
    let Some(function) = symbols
        .functions()
        .iter()
        .find(|function| function.origin() == SymbolOrigin::Source)
    else {
        panic!("test compilation must contain a source function");
    };

    let Some(anchor) = function.syntax_anchor() else {
        panic!("source function must retain its syntax anchor");
    };

    let Some(source) = compilation.source(anchor.source_id()) else {
        panic!("function source must be loaded");
    };

    // Stable symbol keys share their immutable identity storage.
    valid_key(BoundUnitKey::callable_body(
        function.key().clone(),
        BoundSourceAnchor::new(anchor, source.version()),
    ))
}

pub(crate) fn callable_body_key(declaration: u32) -> BoundUnitKey {
    valid_key(BoundUnitKey::callable_body(
        symbol_key(SymbolKind::Function, declaration),
        source_anchor(),
    ))
}

fn source_anchor() -> BoundSourceAnchor {
    let snapshot = match SourceSnapshot::new(
        SourceId::new(0),
        SourceIdentity::new(0),
        SourceOrigin::virtual_source("compilation-test"),
        SourceVersion::new(1),
        "module example;",
    ) {
        Ok(snapshot) => snapshot,
        Err(error) => panic!("test source must be representable: {error:?}"),
    };

    let syntax = parse_source_unit(&snapshot);
    let declarations = discover_source_unit_declarations(syntax.source_unit());

    let [module] = declarations.chunk().module_parts() else {
        panic!("test source must produce one module part");
    };

    BoundSourceAnchor::new(module.syntax_anchor(), SourceVersion::new(1))
}

fn symbol_key(kind: SymbolKind, declaration: u32) -> SymbolKey {
    let Some(package) = PackageIdentity::try_new("example.package") else {
        panic!("test package identity must be valid");
    };

    let Some(path) = ModulePathKey::try_new(["example"]) else {
        panic!("test module path must be valid");
    };

    let owner = SymbolKey::module(SymbolRootKey::Package(package), path);

    let Some(key) = SymbolKey::source_declaration(owner, kind, DeclarationId::new(declaration))
    else {
        panic!("test declaration kind must be source-declared");
    };

    key
}

fn valid_key(key: Option<BoundUnitKey>) -> BoundUnitKey {
    match key {
        Some(key) => key,
        None => panic!("test owner must support the requested unit category"),
    }
}
