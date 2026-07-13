use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use bray_bound_tree::{BoundSourceAnchor, BoundUnitKey};
use bray_declarations::{DeclarationId, discover_source_unit_declarations};
use bray_diagnostics::{DiagnosticBag, DiagnosticKind};
use bray_parser::parse_source_unit;
use bray_source::{
    SourceId, SourceIdentity, SourceInput, SourceOrigin, SourceSnapshot, SourceVersion,
};
use bray_symbols::{
    ModulePathKey, PackageIdentity, SymbolGraph, SymbolKey, SymbolKind, SymbolOrigin, SymbolRootKey,
};

use crate::fact::{FactCellTestEvent, FactCellTestObserver};
use crate::{Compilation, CompilationOptions, CompilationRequest, WorkerBudget};

const FACT_OBSERVATION_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) struct FactTestGate {
    held_event: FactCellTestEvent,
    shared: Arc<(Mutex<FactTestGateState>, Condvar)>,
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
    match Compilation::load_sources(package_identity(), vec![source_input(source, 0)]) {
        Ok(compilation) => compilation,
        Err(error) => panic!("test compilation must load: {error:?}"),
    }
}

pub(crate) fn compilation_with_sources_and_worker_budget(
    sources: &[&str],
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
        CompilationOptions::new(worker_budget),
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
