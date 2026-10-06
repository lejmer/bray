use bray_compiler_known::RepresentationRole;
use bray_symbols::{
    BorrowKind, NamedTypeSymbolId, ProductKind, ReceiverMode, SymbolOrigin, TypeData,
};
use bray_target::TargetValueLayout;

use super::targets::{baseline_codegen_target, codegen_target};
use super::type_fixtures::realized_types;

use crate::CancellationToken;
use crate::compilation::product::realization::support::{
    is_void_result, pointer_layout, receiver_codegen_type,
};
use crate::compilation::substitution::named_type;
use crate::test_support::{compilation, compilation_with_product};

#[test]
fn codegen_receiver_types_preserve_receiver_authority() {
    let compilation = compilation("module app; func main() {}");

    let values = compilation
        .semantic_value_store()
        .expect("semantic values must resolve");

    let receiver = values
        .intern_type(TypeData::tuple([]))
        .expect("receiver type must intern");

    let shared = receiver_codegen_type(values, receiver, ReceiverMode::Shared)
        .expect("shared receiver must resolve");

    let mutable = receiver_codegen_type(values, receiver, ReceiverMode::Mutable)
        .expect("mutable receiver must resolve");

    assert_eq!(
        values.type_data(shared).as_ref(),
        &TypeData::Borrow {
            kind: BorrowKind::Shared,
            target: receiver,
        }
    );

    assert_eq!(
        values.type_data(mutable).as_ref(),
        &TypeData::Borrow {
            kind: BorrowKind::Mutable,
            target: receiver,
        }
    );

    assert_eq!(
        receiver_codegen_type(values, receiver, ReceiverMode::Consuming),
        Ok(receiver)
    );

    assert_eq!(
        receiver_codegen_type(values, receiver, ReceiverMode::ConsumingMutable),
        Ok(receiver)
    );
}

#[test]
fn never_returning_callables_use_void_codegen_results() {
    let compilation = compilation("module app; func main() {}");

    let never = compilation
        .compiler_known_type(RepresentationRole::Never)
        .unwrap_or_else(|error| panic!("never representation must resolve: {error:?}"));

    assert!(
        is_void_result(&compilation, never)
            .unwrap_or_else(|error| panic!("void result classification must resolve: {error:?}"))
    );
}

#[test]
fn real_product_callable_roots_retain_signatures() {
    let source = concat!(
        "module app;\n",
        "static ANSWER: i32 = 42;\n",
        "func main() -> i32\n",
        "{\n",
        "    return ANSWER;\n",
        "}\n",
    );

    let compilation = compilation_with_product(source, ProductKind::Executable);

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let cancellation = CancellationToken::new();
    let target = codegen_target(&compilation);

    let semantic = compilation
        .product_semantics()
        .unwrap_or_else(|error| panic!("product semantics must resolve: {error:?}"));

    let roots = compilation
        .product_root_instances(semantic.value(), None, &target, &cancellation)
        .unwrap_or_else(|error| panic!("product roots must resolve: {error:?}"));

    assert!(
        roots
            .iter()
            .any(|root| root.instance().callable_instance().is_some())
    );

    for root in roots {
        compilation
            .codegen_instance_signature(root.instance(), &cancellation)
            .unwrap_or_else(|error| panic!("root signature must realize: {error:?}"));
    }
}

#[test]
fn callable_indirection_closes_recursive_value_layouts_before_abi_classification() {
    let compilation = compilation(concat!(
        "module app;\n",
        "struct Node\n",
        "{\n",
        "    visit: func(pos node: Node) -> unit;\n",
        "}\n",
    ));

    let target = baseline_codegen_target();

    let symbols = compilation
        .symbol_graph()
        .unwrap_or_else(|error| panic!("symbol graph must be available: {error:?}"));

    let node = symbols
        .structures()
        .iter()
        .find(|symbol| symbol.origin() == SymbolOrigin::Source)
        .unwrap_or_else(|| panic!("fixture must declare Node"));

    let values = compilation
        .semantic_value_store()
        .unwrap_or_else(|error| panic!("semantic values must be available: {error:?}"));

    let ty = named_type(values, NamedTypeSymbolId::Struct(node.id()))
        .unwrap_or_else(|error| panic!("Node type must be available: {error:?}"));

    let mappings = realized_types(&compilation, &target, [ty]);

    assert_eq!(
        mappings[&ty].layout().map(TargetValueLayout::size),
        Some(pointer_layout(&target).size())
    );
}

#[test]
fn raw_pointer_indirection_closes_recursive_value_layouts() {
    let compilation = compilation(concat!(
        "module app;\n",
        "struct Node\n",
        "{\n",
        "    next: RawPointer<Node>;\n",
        "}\n",
    ));

    let target = baseline_codegen_target();

    let symbols = compilation
        .symbol_graph()
        .unwrap_or_else(|error| panic!("symbol graph must be available: {error:?}"));

    let node = symbols
        .structures()
        .iter()
        .find(|symbol| symbol.origin() == SymbolOrigin::Source)
        .unwrap_or_else(|| panic!("fixture must declare Node"));

    let values = compilation
        .semantic_value_store()
        .unwrap_or_else(|error| panic!("semantic values must be available: {error:?}"));

    let ty = named_type(values, NamedTypeSymbolId::Struct(node.id()))
        .unwrap_or_else(|error| panic!("Node type must be available: {error:?}"));

    let mappings = realized_types(&compilation, &target, [ty]);

    assert_eq!(
        mappings[&ty].layout().map(TargetValueLayout::size),
        Some(pointer_layout(&target).size())
    );
}
