use super::support::artifacts::generated_artifacts;
use super::support::compilation::{codegen_compilation_for_product, test_product_identity};
use super::support::specialization::{CONCRETE_GENERIC_SOURCE, concrete_generic_specializations};
use crate::CancellationToken;
use crate::compilation::product::codegen::{ConcreteCodegenRoot, NativeDemandReason};
use crate::compilation::product::specialization::ConcreteCodegenInstance;
use bray_codegen::{
    CodegenGenericArgument, CodegenLinkage, CodegenOptions, CodegenPartitionPolicy,
    CodegenResultMapping, CodegenSpecialization, partition_codegen_units,
};
use bray_compiler_known::RepresentationRole;
use bray_symbols::{
    CallableDefinitionId, CallableInstanceData, ConstantTermData, ConstantValueData,
    ConstantValueKind, GenericArgument, GenericOwnerId, GenericParameterSymbolId,
    GenericSubstitutionData, ImplementationRequirementKey, ImplementationSelection,
    NamedTypeSymbolId, ProductIdentity, ProductKind, SymbolOrigin, TraitApplicationData, TypeData,
};
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn concrete_generic_instances_realize_signatures_and_layouts() {
    let compilation = crate::test_support::compilation(CONCRETE_GENERIC_SOURCE);

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let cancellation = CancellationToken::new();

    let target = compilation
        .selected_target()
        .target()
        .codegen_target()
        .unwrap_or_else(|error| panic!("test codegen target must validate: {error:?}"));

    let semantic = compilation
        .product_semantics()
        .unwrap_or_else(|error| panic!("test product plan must resolve: {error:?}"));

    let roots = compilation
        .product_root_instances(semantic.value(), None, &target, &cancellation)
        .unwrap_or_else(|error| panic!("test roots must resolve: {error:?}"));

    let reachability = compilation
        .codegen_reachability(
            roots.clone(),
            None,
            &target,
            CodegenOptions::default(),
            false,
            &cancellation,
        )
        .unwrap_or_else(|error| panic!("generic reachability must close: {error:?}"));

    let reversed_reachability = compilation
        .codegen_reachability(
            roots.clone().into_iter().rev(),
            None,
            &target,
            CodegenOptions::default(),
            false,
            &cancellation,
        )
        .unwrap_or_else(|error| panic!("reversed reachability must close: {error:?}"));

    assert_eq!(reachability.graph(), reversed_reachability.graph());

    for instance in reachability.graph().instances() {
        assert_eq!(
            reachability.instance(instance.key()),
            reversed_reachability.instance(instance.key())
        );
    }

    let roots: BTreeSet<_> = roots.into_iter().map(|root| root.key().clone()).collect();

    let compatibility = reachability
        .graph()
        .instances()
        .iter()
        .map(|instance| {
            compilation
                .codegen_partition_compatibility(
                    instance,
                    reachability
                        .instance(instance.key())
                        .expect("retained instance must be concrete"),
                    &test_product_identity(),
                    &roots,
                    &cancellation,
                )
                // The test lookup owns the Arc-backed identity during partitioning.
                .map(|compatibility| (instance.key().clone(), compatibility))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()
        .unwrap_or_else(|error| panic!("partition compatibility must resolve: {error:?}"));

    let units = partition_codegen_units(
        CodegenPartitionPolicy::NATIVE_BALANCED,
        reachability.graph(),
        |instance| compatibility.get(instance.key()).cloned(),
        |mir| {
            crate::compilation::product::mir_content_identity(&compilation, mir)
                .unwrap_or_else(|error| panic!("test MIR identity must resolve: {error:?}"))
        },
    )
    .unwrap_or_else(|error| panic!("generic units must partition: {error:?}"));

    let mut saw_concrete_generic_signature = false;
    let mut saw_const_specialization = false;
    let mut saw_product_local_symbol = false;
    let primary_product = test_product_identity();

    let alternate_product =
        ProductIdentity::try_new(primary_product.package().clone(), "alternate-application")
            .unwrap_or_else(|| panic!("alternate test product identity must validate"));

    for unit in units.iter() {
        let mappings = compilation
            .codegen_mappings_for_product(
                &primary_product,
                unit,
                None,
                &BTreeSet::new(),
                &target,
                &reachability.graph().roots().iter().cloned().collect(),
                &reachability,
                false,
                &cancellation,
            )
            .unwrap_or_else(|error| panic!("generic mappings must realize: {error:?}"));

        let alternate_mappings = compilation
            .codegen_mappings_for_product(
                &alternate_product,
                unit,
                None,
                &BTreeSet::new(),
                &target,
                &reachability.graph().roots().iter().cloned().collect(),
                &reachability,
                false,
                &cancellation,
            )
            .unwrap_or_else(|error| panic!("alternate-product mappings must realize: {error:?}"));

        for symbol in mappings
            .symbols()
            .iter()
            .filter(|symbol| symbol.linkage() == CodegenLinkage::Internal)
        {
            let alternate = alternate_mappings
                .symbols()
                .iter()
                .find(|candidate| candidate.key() == symbol.key())
                .unwrap_or_else(|| panic!("alternate mapping must retain every local symbol"));

            assert_ne!(symbol.name(), alternate.name());
            saw_product_local_symbol = true;
        }

        for instance in unit.instances() {
            let realization = reachability
                .instance(instance.key())
                .unwrap_or_else(|| panic!("reachable instance payload must be retained"));

            let signature = compilation
                .codegen_instance_signature(realization, &cancellation)
                .unwrap_or_else(|error| panic!("generic signature must realize: {error:?}"));

            if instance
                .key()
                .specialization()
                .arguments()
                .iter()
                .any(|argument| matches!(argument, CodegenGenericArgument::Constant(_)))
            {
                saw_const_specialization = true;
            }

            let CodegenResultMapping::Direct { ty, .. } = signature.result() else {
                continue;
            };

            assert!(mappings.ty(*ty).is_some());

            let values = compilation
                .semantic_value_store()
                .unwrap_or_else(|error| panic!("semantic values must resolve: {error:?}"));

            let data = values.type_data(*ty);

            assert!(!matches!(data.as_ref(), TypeData::TypeParameter(_)));

            saw_concrete_generic_signature |= matches!(
                instance.key().specialization(),
                CodegenSpecialization::Generic(_)
            );
        }
    }

    assert!(saw_concrete_generic_signature);
    assert!(saw_const_specialization);
    assert!(saw_product_local_symbol);
}

#[test]
fn library_roots_exclude_members_of_open_generic_containers() {
    let compilation = crate::test_support::compilation_with_product(
        concat!(
            "module app;\n",
            "struct Holder<T>\n",
            "{\n",
            "    value: T;\n",
            "\n",
            "    destruct()\n",
            "    {\n",
            "    }\n",
            "}\n",
        ),
        ProductKind::Library,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let cancellation = CancellationToken::new();

    let target = compilation
        .selected_target()
        .target()
        .codegen_target()
        .unwrap_or_else(|error| panic!("test codegen target must validate: {error:?}"));

    let semantic = compilation
        .product_semantics()
        .unwrap_or_else(|error| panic!("test product plan must resolve: {error:?}"));

    let roots = compilation
        .product_root_instances(semantic.value(), None, &target, &cancellation)
        .unwrap_or_else(|error| panic!("test roots must resolve: {error:?}"));

    assert!(roots.is_empty());
}

#[test]
fn library_roots_include_public_implementation_fulfillments() {
    let compilation = crate::test_support::compilation_with_product(
        concat!(
            "module app;\n",
            "trait Equal<Other>\n",
            "{\n",
            "    func equals(pos other: &Other) -> bool;\n",
            "}\n",
            "\n",
            "struct Value\n",
            "{\n",
            "}\n",
            "\n",
            "impl ValueEqual = Value(Equal<Value>)\n",
            "{\n",
            "    func equals(pos other: &Value) -> bool\n",
            "    {\n",
            "        return true;\n",
            "    }\n",
            "}\n",
        ),
        ProductKind::Library,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let cancellation = CancellationToken::new();

    let target = compilation
        .selected_target()
        .target()
        .codegen_target()
        .unwrap_or_else(|error| panic!("test codegen target must validate: {error:?}"));

    let semantic = compilation
        .product_semantics()
        .unwrap_or_else(|error| panic!("test product plan must resolve: {error:?}"));

    let roots = compilation
        .product_root_instances(semantic.value(), None, &target, &cancellation)
        .unwrap_or_else(|error| panic!("test roots must resolve: {error:?}"));

    let symbols = compilation
        .symbol_graph()
        .unwrap_or_else(|error| panic!("test symbols must resolve: {error:?}"));

    let fulfillment = symbols
        .trait_callable_fulfillments()
        .iter()
        .find(|fulfillment| fulfillment.origin() == SymbolOrigin::Source)
        .unwrap_or_else(|| panic!("source fulfillment must exist"));

    let fulfillment = CallableDefinitionId::try_new(fulfillment.id().into())
        .unwrap_or_else(|| panic!("trait callable fulfillment must be callable"));

    assert!(roots.iter().any(|root| {
        root.instance()
            .callable_instance()
            .is_some_and(|callable| callable.definition() == fulfillment)
    }));
}

#[test]
fn trait_owned_default_bodies_specialize_with_the_selected_implementation() {
    let source = concat!(
        "module app;\n",
        "trait Counter\n",
        "{\n",
        "    func count() -> i32;\n",
        "    func doubled() -> i32\n",
        "    {\n",
        "        return self.count() + self.count();\n",
        "    }\n",
        "    func quadrupled() -> i32\n",
        "    {\n",
        "        return self.doubled() + self.doubled();\n",
        "    }\n",
        "}\n",
        "struct Value\n",
        "{\n",
        "    count: i32;\n",
        "}\n",
        "impl ValueCounter = Value(Counter)\n",
        "{\n",
        "    func count() -> i32\n",
        "    {\n",
        "        return self.count;\n",
        "    }\n",
        "}\n",
        "func read_generic<T>(pos value: &T) -> i32\n",
        "    with(T: Counter)\n",
        "{\n",
        "    return value.quadrupled();\n",
        "}\n",
        "public func read() -> i32\n",
        "{\n",
        "    let value: Value = { count = 3 };\n",
        "    return value.quadrupled() + read_generic<Value>(&value);\n",
        "}\n",
    );

    let (backend, compilation) = codegen_compilation_for_product(source, ProductKind::Library);

    let cancellation = CancellationToken::new();

    let target = compilation
        .selected_target()
        .target()
        .codegen_target()
        .unwrap_or_else(|error| panic!("test codegen target must validate: {error:?}"));

    let semantic = compilation
        .product_semantics()
        .unwrap_or_else(|error| panic!("trait default product semantics must resolve: {error:?}"));

    let roots = compilation
        .product_root_instances(semantic.value(), None, &target, &cancellation)
        .unwrap_or_else(|error| panic!("trait default roots must resolve: {error:?}"));

    compilation
        .codegen_reachability(
            roots,
            None,
            &target,
            CodegenOptions::default(),
            false,
            &cancellation,
        )
        .unwrap_or_else(|error| panic!("trait default reachability must close: {error:?}"));

    let plan = compilation
        .native_product_plan(
            test_product_identity(),
            crate::BuildConfiguration::Development,
            None,
            [],
            None,
        )
        .unwrap_or_else(|error| panic!("trait default body must realize: {error:?}"));

    assert!(
        generated_artifacts(&backend, &plan)
            .iter()
            .all(|artifact| !artifact.is_empty())
    );
}

#[test]
fn constrained_trait_owned_default_bodies_dispatch_required_members() {
    let source = concat!(
        "module app;\n",
        "trait Numeric\n",
        "{\n",
        "    func bounds() -> (Self, Self);\n",
        "    func increase(pos amount: Self) -> Self\n",
        "        with(Self: Add<Self>, Self(Add<Self>).Output == Self, Self: Copyable)\n",
        "    {\n",
        "        let bounds: (Self, Self) = self.bounds();\n",
        "        return identity<Self>(bounds.0 + amount);\n",
        "    }\n",
        "}\n",
        "impl I32Numeric = i32(Numeric)\n",
        "{\n",
        "    func bounds() -> (i32, i32)\n",
        "    {\n",
        "        return (1, 10);\n",
        "    }\n",
        "}\n",
        "func identity<Value>(pos value: Value) -> Value\n",
        "{\n",
        "    return value;\n",
        "}\n",
        "public func read() -> i32\n",
        "{\n",
        "    return 1.increase(2);\n",
        "}\n",
    );

    let (backend, compilation) = codegen_compilation_for_product(source, ProductKind::Library);

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let cancellation = CancellationToken::new();

    let target = compilation
        .selected_target()
        .target()
        .codegen_target()
        .unwrap_or_else(|error| panic!("test codegen target must validate: {error:?}"));

    let semantic = compilation
        .product_semantics()
        .unwrap_or_else(|error| panic!("constrained trait semantics must resolve: {error:?}"));

    let roots = compilation
        .product_root_instances(semantic.value(), None, &target, &cancellation)
        .unwrap_or_else(|error| panic!("constrained trait roots must resolve: {error:?}"));

    compilation
        .codegen_reachability(
            roots,
            None,
            &target,
            CodegenOptions::default(),
            false,
            &cancellation,
        )
        .unwrap_or_else(|error| panic!("constrained trait reachability must close: {error:?}"));

    let plan = compilation
        .native_product_plan(
            test_product_identity(),
            crate::BuildConfiguration::Development,
            None,
            [],
            None,
        )
        .unwrap_or_else(|error| panic!("constrained trait default body must realize: {error:?}"));

    assert!(
        generated_artifacts(&backend, &plan)
            .iter()
            .all(|artifact| !artifact.is_empty())
    );
}

#[test]
fn concrete_generic_specializations_are_stable_across_store_order() {
    let first = crate::test_support::compilation(CONCRETE_GENERIC_SOURCE);
    let second = crate::test_support::compilation(CONCRETE_GENERIC_SOURCE);

    perturb_semantic_value_order(&second);

    let first = concrete_generic_specializations(&first);
    let second = concrete_generic_specializations(&second);

    assert_eq!(first, second);

    assert!(first.iter().any(|specialization| {
        specialization
            .arguments()
            .iter()
            .any(|argument| matches!(argument, CodegenGenericArgument::Type(_)))
    }));

    assert!(first.iter().any(|specialization| {
        specialization
            .arguments()
            .iter()
            .any(|argument| matches!(argument, CodegenGenericArgument::Constant(_)))
    }));
}

#[test]
fn concrete_generic_instances_retain_exact_implementation_witnesses() {
    let compilation = crate::test_support::compilation(concat!(
        "module app;\n",
        "\n",
        "func entry()\n",
        "{\n",
        "}\n",
        "\n",
        "trait Converts<T>\n",
        "{\n",
        "}\n",
        "\n",
        "struct Wrapper<T>\n",
        "{\n",
        "}\n",
        "\n",
        "impl WrapperConverts = Wrapper<T>(Converts<T>) with(true)\n",
        "{\n",
        "}\n",
        "\n",
        "func target<T>()\n",
        "{\n",
        "}\n",
    ));

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let cancellation = CancellationToken::new();

    let target = compilation
        .selected_target()
        .target()
        .codegen_target()
        .unwrap_or_else(|error| panic!("test codegen target must validate: {error:?}"));

    let symbols = compilation
        .symbol_graph()
        .unwrap_or_else(|error| panic!("test symbols must resolve: {error:?}"));

    let values = compilation
        .semantic_value_store()
        .unwrap_or_else(|error| panic!("test values must resolve: {error:?}"));

    let wrapper = symbols
        .structures()
        .iter()
        .find(|symbol| symbol.origin() == SymbolOrigin::Source)
        .unwrap_or_else(|| panic!("test wrapper must be declared"));

    let conversion = symbols
        .traits()
        .iter()
        .find(|symbol| symbol.origin() == SymbolOrigin::Source)
        .unwrap_or_else(|| panic!("test trait must be declared"));

    let target_function = symbols
        .functions()
        .iter()
        .find(|symbol| {
            symbols
                .member_name(symbol.id().into())
                .is_some_and(|name| name.as_str() == "target")
        })
        .unwrap_or_else(|| panic!("test target must be declared"));

    let boolean_definition = compilation
        .available_compiler_known_symbols()
        .representation_symbol::<bray_symbols::StructSymbolId>(RepresentationRole::ScalarBool)
        .unwrap_or_else(|| panic!("test bool representation must be available"));

    let boolean = named_test_type(values, boolean_definition, [], []);

    let wrapper_type = named_test_type(
        values,
        wrapper.id(),
        wrapper
            .generic_type_parameters()
            .iter()
            .copied()
            .map(GenericParameterSymbolId::Type),
        [GenericArgument::Type(boolean)],
    );

    let trait_substitution = test_substitution(
        values,
        conversion.id().into(),
        conversion
            .generic_type_parameters()
            .iter()
            .copied()
            .map(GenericParameterSymbolId::Type),
        [GenericArgument::Type(boolean)],
    );

    let trait_application = values
        .intern_trait_application(TraitApplicationData::new(
            conversion.id(),
            trait_substitution,
        ))
        .unwrap_or_else(|error| panic!("test trait application must intern: {error:?}"));

    let selection = compilation
        .implementation_selection_result(ImplementationRequirementKey::new(
            wrapper_type,
            trait_application,
        ))
        .unwrap_or_else(|error| panic!("test witness must select: {error:?}"));

    let ImplementationSelection::Selected(witness) = *selection.value() else {
        panic!("test generic implementation must be selected");
    };

    let callable_substitution = test_substitution(
        values,
        target_function.id().into(),
        target_function
            .generic_type_parameters()
            .iter()
            .copied()
            .map(GenericParameterSymbolId::Type),
        [GenericArgument::Type(boolean)],
    );

    let definition = CallableDefinitionId::try_new(target_function.id().into())
        .unwrap_or_else(|| panic!("test target must be callable"));

    let root = compilation
        .concrete_codegen_callable(
            CallableInstanceData::new(definition, callable_substitution),
            [witness],
            &target,
            &cancellation,
        )
        .unwrap_or_else(|error| panic!("test concrete root must realize: {error:?}"));

    assert!(
        ConcreteCodegenInstance::try_callable(
            root.key().clone(),
            root.callable_instance()
                .unwrap_or_else(|| panic!("test root must retain its callable")),
            CodegenSpecialization::NonGeneric,
            [],
            None,
        )
        .is_none()
    );

    let root_key = root.key().clone();

    let reachability = compilation
        .codegen_reachability(
            [
                ConcreteCodegenRoot::new(root.clone(), NativeDemandReason::ExecutableEntry),
                ConcreteCodegenRoot::new(root, NativeDemandReason::NativeExport),
            ],
            None,
            &target,
            CodegenOptions::default(),
            false,
            &cancellation,
        )
        .unwrap_or_else(|error| panic!("generic reachability must close: {error:?}"));

    let [instance] = reachability.graph().instances() else {
        panic!("test root must be the only reachable instance");
    };

    assert_eq!(
        reachability
            .demands()
            .iter()
            .filter(|demand| demand.predecessor().is_none()
                && demand.instance_target() == Some(&root_key))
            .map(crate::compilation::NativeDemand::reason)
            .collect::<Vec<_>>(),
        [
            NativeDemandReason::ExecutableEntry,
            NativeDemandReason::NativeExport,
        ]
    );

    assert!(matches!(
        instance.key().witnesses()[0].specialization(),
        CodegenSpecialization::Generic(_)
    ));

    let realization = reachability
        .instance(instance.key())
        .unwrap_or_else(|| panic!("test witness payload must be retained"));

    assert_eq!(realization.implementation_witnesses(), [witness]);
}

fn named_test_type(
    values: &bray_symbols::SemanticValueStore,
    definition: bray_symbols::StructSymbolId,
    parameters: impl IntoIterator<Item = GenericParameterSymbolId>,
    arguments: impl IntoIterator<Item = GenericArgument>,
) -> bray_symbols::TypeId {
    let substitution = test_substitution(values, definition.into(), parameters, arguments);

    values
        .intern_type(TypeData::Named {
            definition: NamedTypeSymbolId::Struct(definition),
            substitution,
        })
        .unwrap_or_else(|error| panic!("test named type must intern: {error:?}"))
}

fn test_substitution(
    values: &bray_symbols::SemanticValueStore,
    owner: bray_symbols::AnySymbolId,
    parameters: impl IntoIterator<Item = GenericParameterSymbolId>,
    arguments: impl IntoIterator<Item = GenericArgument>,
) -> bray_symbols::GenericSubstitutionId {
    let owner = GenericOwnerId::try_new(owner)
        .unwrap_or_else(|| panic!("test substitution owner must be generic"));

    let substitution = GenericSubstitutionData::try_new(owner, parameters, arguments)
        .unwrap_or_else(|error| panic!("test substitution must validate: {error:?}"));

    values
        .intern_generic_substitution(substitution)
        .unwrap_or_else(|error| panic!("test substitution must intern: {error:?}"))
}

fn perturb_semantic_value_order(compilation: &crate::Compilation) {
    let values = compilation
        .semantic_value_store()
        .unwrap_or_else(|error| panic!("semantic values must resolve: {error:?}"));

    let mut ty = values
        .intern_type(TypeData::tuple([]))
        .unwrap_or_else(|error| panic!("noise tuple type must intern: {error:?}"));

    let mut value = values
        .intern_constant_value(ConstantValueData::new(ty, ConstantValueKind::Unit))
        .unwrap_or_else(|error| panic!("noise unit value must intern: {error:?}"));

    for _ in 0..4 {
        ty = values
            .intern_type(TypeData::Nullable(ty))
            .unwrap_or_else(|error| panic!("noise nullable type must intern: {error:?}"));

        value = values
            .intern_constant_value(ConstantValueData::new(
                ty,
                ConstantValueKind::NullablePresent(value),
            ))
            .unwrap_or_else(|error| panic!("noise nullable value must intern: {error:?}"));

        values
            .intern_constant_term(ConstantTermData::Value(value))
            .unwrap_or_else(|error| panic!("noise constant term must intern: {error:?}"));
    }
}
