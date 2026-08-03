use std::collections::BTreeSet;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use bray_bound_tree::{BoundSourceAnchor, BoundUnitKey};
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
    Compilation, CompilationOptions, CompilationRequest, DependencyInterfaceInput, WorkerBudget,
};

const FACT_OBSERVATION_TIMEOUT: Duration = Duration::from_secs(5);

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
    PackageVersion::try_new("1.0.0")
        .unwrap_or_else(|| panic!("test package version must be valid"))
}

pub(crate) fn source_input(text: &str, version: u32) -> SourceInput {
    SourceInput::virtual_text(
        SourceIdentity::new(version),
        format!("source-{version}"),
        SourceVersion::new(u64::from(version)),
        text,
    )
}

pub(crate) fn diagnostic_kinds(diagnostics: &DiagnosticBag) -> Vec<DiagnosticKind> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.kind())
        .collect()
}

pub(crate) fn compilation(source: &str) -> Compilation {
    compilation_with_options(source, CompilationOptions::default())
}

pub(crate) fn compilation_with_target_operations(
    source: &str,
    raw_memory: bool,
    allocation: bool,
) -> Compilation {
    let baseline = crate::SelectedTarget::baseline();
    let profile = baseline.profile();
    let baseline_facts = profile.facts();

    let facts = bray_target::TargetFacts::new(
        baseline_facts.identity().clone(),
        baseline_facts.scalars(),
        baseline_facts.atomics(),
        baseline_facts.abis(),
        baseline_facts.address_spaces(),
        baseline_facts.alignments(),
        bray_target::TargetOperationFacts::new(raw_memory, allocation),
    );

    let profile = bray_target::TargetProfile::try_new(
        profile.identity().clone(),
        profile.machine().clone(),
        facts,
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
    let request = CompilationRequest::with_options(
        package_identity(),
        vec![source_input(source, 0)],
        options,
    );

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
    let sources = sources
        .iter()
        .copied()
        .enumerate()
        .map(|(index, source)| match u32::try_from(index) {
            Ok(version) => source_input(source, version),
            Err(_) => panic!("test source index must fit in u32"),
        })
        .collect();

    let request = CompilationRequest::with_options(
        package_identity(),
        sources,
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
