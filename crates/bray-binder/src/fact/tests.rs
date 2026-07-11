use std::sync::{Arc, Barrier};
use std::thread;

use bray_declarations::{
    DeclarationTable, discover_source_unit_declarations, merge_declaration_chunks,
};
use bray_diagnostics::DiagnosticResult;
use bray_parser::{SyntaxTreeResult, parse_source_unit};
use bray_source::{SourceIdentity, SourceInput, SourceStore, SourceVersion};
use bray_symbols::{
    ConstantDeclaredTypeFact, ConstantSymbolId, ConstantValueId, PackageIdentity,
    SemanticValueStore, SymbolFactRequest, SymbolGraph, TypeData, TypeId,
};
use bray_syntax::SyntaxTree;

use super::{
    BinderCancellation, BinderFactContext, BinderFactError, BinderFactResult,
    BindingSymbolFactProvider, ImportedSymbolFactProvider, SymbolFactProvider, TargetFactProvider,
    TargetFactResult,
};

struct TestFacts {
    result: Arc<DiagnosticResult<TypeId>>,
}

impl SymbolFactProvider<ConstantDeclaredTypeFact> for TestFacts {
    fn symbol_fact(
        &self,
        _request: SymbolFactRequest<ConstantDeclaredTypeFact>,
    ) -> BinderFactResult<Arc<DiagnosticResult<TypeId>>> {
        Ok(Arc::clone(&self.result))
    }
}

impl ImportedSymbolFactProvider<ConstantDeclaredTypeFact> for TestFacts {
    fn imported_symbol_fact(
        &self,
        request: SymbolFactRequest<ConstantDeclaredTypeFact>,
    ) -> BinderFactResult<Arc<DiagnosticResult<TypeId>>> {
        if request.owner() == ConstantSymbolId::from_symbol_id(bray_symbols::SymbolId::new(99)) {
            return Err(BinderFactError::DependencyUnavailable);
        }

        Ok(Arc::clone(&self.result))
    }
}

struct TestTargetFacts {
    symbol: ConstantSymbolId,
    result: Arc<TargetFactResult>,
}

impl TargetFactProvider for TestTargetFacts {
    fn target_fact(&self, fact: ConstantSymbolId) -> BinderFactResult<Arc<TargetFactResult>> {
        if fact != self.symbol {
            return Err(BinderFactError::DependencyUnavailable);
        }

        Ok(Arc::clone(&self.result))
    }
}

struct TestCancellation {
    barrier: Barrier,
    cancelled: std::sync::atomic::AtomicBool,
}

impl BinderCancellation for TestCancellation {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::Acquire)
    }
}

struct TestContext<'facts> {
    syntax: &'facts SyntaxTree,
    declarations: &'facts DeclarationTable,
    symbols: &'facts SymbolGraph,
    semantic_values: &'facts SemanticValueStore,
    target_facts: &'facts TestTargetFacts,
    imported_symbol_facts: &'facts TestFacts,
    symbol_facts: &'facts TestFacts,
    cancellation: &'facts TestCancellation,
}

impl BinderFactContext for TestContext<'_> {
    type TargetFacts = TestTargetFacts;
    type ImportedSymbolFacts = TestFacts;
    type SymbolFacts = TestFacts;
    type Cancellation = TestCancellation;

    fn syntax(&self) -> &SyntaxTree {
        self.syntax
    }

    fn declarations(&self) -> &DeclarationTable {
        self.declarations
    }

    fn symbols(&self) -> &SymbolGraph {
        self.symbols
    }

    fn semantic_values(&self) -> &SemanticValueStore {
        self.semantic_values
    }

    fn target_facts(&self) -> &Self::TargetFacts {
        self.target_facts
    }

    fn imported_symbol_facts(&self) -> &Self::ImportedSymbolFacts {
        self.imported_symbol_facts
    }

    fn symbol_facts(&self) -> &Self::SymbolFacts {
        self.symbol_facts
    }

    fn cancellation(&self) -> &Self::Cancellation {
        self.cancellation
    }
}

struct DeclaredTypeBinder;

impl BindingSymbolFactProvider<ConstantDeclaredTypeFact, TestContext<'_>> for DeclaredTypeBinder {
    fn compute_symbol_fact(
        &self,
        context: &TestContext<'_>,
        request: SymbolFactRequest<ConstantDeclaredTypeFact>,
    ) -> BinderFactResult<DiagnosticResult<TypeId>> {
        if context.is_cancelled() {
            return Err(BinderFactError::Cancelled);
        }

        let Some(_) = context.symbols().constant(request.owner()) else {
            return Err(BinderFactError::DependencyUnavailable);
        };

        Ok(DiagnosticResult::without_diagnostics(
            *context.symbol_facts.result.value(),
        ))
    }
}

#[test]
fn contexts_expose_exact_immutable_fact_inputs() {
    let fixture = TestFixture::new();
    let context = fixture.context();

    assert_eq!(context.syntax().source_units().len(), 1);
    assert_eq!(context.declarations().declarations().len(), 2);
    assert_eq!(context.symbols().constants().len(), 1);
    assert_eq!(context.semantic_values().id(), fixture.semantic_values.id());

    let target = match context.target_facts().target_fact(fixture.constant) {
        Ok(target) => target,
        Err(error) => panic!("test target fact should exist: {error:?}"),
    };

    assert_eq!(target.value(), &fixture.target_value);
    assert!(!context.is_cancelled());
}

#[test]
fn typed_symbol_fact_reads_are_repeatable_and_concurrent() {
    let fixture = TestFixture::new();
    let request = SymbolFactRequest::<ConstantDeclaredTypeFact>::new(fixture.constant);
    let context = fixture.context();

    let first = match context.symbol_facts().symbol_fact(request) {
        Ok(result) => result,
        Err(error) => panic!("test symbol fact should exist: {error:?}"),
    };

    let second = match context.symbol_facts().symbol_fact(request) {
        Ok(result) => result,
        Err(error) => panic!("test symbol fact should remain available: {error:?}"),
    };

    assert!(Arc::ptr_eq(&first, &second));

    thread::scope(|scope| {
        let handles = (0..4)
            .map(|_| {
                scope.spawn(|| match context.symbol_facts().symbol_fact(request) {
                    Ok(result) => result,
                    Err(error) => panic!("concurrent fact read should succeed: {error:?}"),
                })
            })
            .collect::<Vec<_>>();

        for handle in handles {
            let concurrent = match handle.join() {
                Ok(result) => result,
                Err(error) => panic!("fact reader should not panic: {error:?}"),
            };

            assert!(Arc::ptr_eq(&first, &concurrent));
        }
    });
}

#[test]
fn imported_fact_failures_remain_outer_query_failures() {
    let fixture = TestFixture::new();
    let context = fixture.context();

    let missing = SymbolFactRequest::<ConstantDeclaredTypeFact>::new(
        ConstantSymbolId::from_symbol_id(bray_symbols::SymbolId::new(99)),
    );

    assert_eq!(
        context
            .imported_symbol_facts()
            .imported_symbol_fact(missing),
        Err(BinderFactError::DependencyUnavailable)
    );

    let present = SymbolFactRequest::<ConstantDeclaredTypeFact>::new(fixture.constant);
    let result = match context
        .imported_symbol_facts()
        .imported_symbol_fact(present)
    {
        Ok(result) => result,
        Err(error) => panic!("recovery should not poison later imported reads: {error:?}"),
    };

    assert_eq!(result.value(), &fixture.declared_type);
}

#[test]
fn binding_computation_is_deterministic_and_recovers_from_unknown_owners() {
    let fixture = TestFixture::new();
    let context = fixture.context();
    let request = SymbolFactRequest::<ConstantDeclaredTypeFact>::new(fixture.constant);

    let first = DeclaredTypeBinder.compute_symbol_fact(&context, request);
    let second = DeclaredTypeBinder.compute_symbol_fact(&context, request);

    assert_eq!(first, second);
    assert_eq!(
        first,
        Ok(DiagnosticResult::without_diagnostics(fixture.declared_type))
    );

    let unknown = SymbolFactRequest::<ConstantDeclaredTypeFact>::new(
        ConstantSymbolId::from_symbol_id(bray_symbols::SymbolId::new(99)),
    );

    assert_eq!(
        DeclaredTypeBinder.compute_symbol_fact(&context, unknown),
        Err(BinderFactError::DependencyUnavailable)
    );
}

#[test]
fn binding_computation_observes_concurrent_cancellation() {
    let fixture = TestFixture::new();
    let context = fixture.context();
    let request = SymbolFactRequest::<ConstantDeclaredTypeFact>::new(fixture.constant);

    thread::scope(|scope| {
        let cancellation = context.cancellation();

        scope.spawn(move || {
            cancellation.barrier.wait();
            cancellation
                .cancelled
                .store(true, std::sync::atomic::Ordering::Release);
        });

        context.cancellation().barrier.wait();

        while !context.is_cancelled() {
            thread::yield_now();
        }

        assert_eq!(
            DeclaredTypeBinder.compute_symbol_fact(&context, request),
            Err(BinderFactError::Cancelled)
        );
    });
}

struct TestFixture {
    syntax: SyntaxTree,
    declarations: DeclarationTable,
    symbols: SymbolGraph,
    semantic_values: SemanticValueStore,
    constant: ConstantSymbolId,
    declared_type: TypeId,
    target_value: ConstantValueId,
    symbol_facts: TestFacts,
    target_facts: TestTargetFacts,
    cancellation: TestCancellation,
}

impl TestFixture {
    fn new() -> Self {
        let mut sources = SourceStore::new();
        let source = SourceInput::virtual_text(
            SourceIdentity::new(0),
            "binder-facts",
            SourceVersion::new(0),
            "module app; const Size: Int = 1;",
        );

        let source_id = match sources.insert_input(source) {
            Ok(source_id) => source_id,
            Err(error) => panic!("test source should load: {error:?}"),
        };

        let snapshot = match sources.get(source_id) {
            Some(snapshot) => snapshot,
            None => panic!("inserted test source should exist"),
        };

        let source_syntax = parse_source_unit(snapshot);
        let declaration_chunk = discover_source_unit_declarations(source_syntax.source_unit());
        let (syntax, _) = SyntaxTreeResult::from_source_unit_results([source_syntax]).into_parts();
        let (declarations, _) = merge_declaration_chunks([&declaration_chunk]).into_parts();

        let package = match PackageIdentity::try_new("test.package") {
            Some(package) => package,
            None => panic!("test package identity should be valid"),
        };

        let symbols = match SymbolGraph::build_source(package, &declarations) {
            Ok(symbols) => symbols,
            Err(error) => panic!("test symbol graph should build: {error:?}"),
        };

        let [constant] = symbols.constants() else {
            panic!("test graph should contain one constant");
        };

        let constant = constant.id();
        let semantic_values = match SemanticValueStore::try_new() {
            Ok(store) => store,
            Err(error) => panic!("test semantic store should be available: {error:?}"),
        };

        let declared_type = match semantic_values.intern_type(TypeData::Error) {
            Ok(ty) => ty,
            Err(error) => panic!("test type should intern: {error:?}"),
        };

        let target_value =
            match semantic_values.intern_constant_value(bray_symbols::ConstantValueData::new(
                declared_type,
                bray_symbols::ConstantValueKind::Error,
            )) {
                Ok(value) => value,
                Err(error) => panic!("test target value should intern: {error:?}"),
            };

        let symbol_result = Arc::new(DiagnosticResult::without_diagnostics(declared_type));
        let target_result = Arc::new(DiagnosticResult::without_diagnostics(target_value));

        Self {
            syntax,
            declarations,
            symbols,
            semantic_values,
            constant,
            declared_type,
            target_value,
            symbol_facts: TestFacts {
                result: symbol_result,
            },
            target_facts: TestTargetFacts {
                symbol: constant,
                result: target_result,
            },
            cancellation: TestCancellation {
                barrier: Barrier::new(2),
                cancelled: std::sync::atomic::AtomicBool::new(false),
            },
        }
    }

    fn context(&self) -> TestContext<'_> {
        TestContext {
            syntax: &self.syntax,
            declarations: &self.declarations,
            symbols: &self.symbols,
            semantic_values: &self.semantic_values,
            target_facts: &self.target_facts,
            imported_symbol_facts: &self.symbol_facts,
            symbol_facts: &self.symbol_facts,
            cancellation: &self.cancellation,
        }
    }
}
