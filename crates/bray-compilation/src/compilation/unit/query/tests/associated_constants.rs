use crate::test_support::{compilation, source_constant_value, source_function_body_key};
use bray_symbols::ConstantValueKind;

#[test]
fn associated_constants_preserve_direct_and_inherent_generic_arguments() {
    let compilation = compilation(
        r#"
        module app;

        struct Buffer<const N: usize>
        {
            const VALUE: usize = N;
            const VALUES: [i32; N] = [7; N];
        }

        impl Buffer<M>
        {
            const EXTRA: usize = M + 1;
        }

        const FIRST: usize = Buffer<3>.VALUE;
        const SECOND: usize = Buffer<5>.VALUE;
        const EXTRA_RESULT: usize = Buffer<5>.EXTRA;
        const ELEMENT: i32 = Buffer<3>.VALUES[2];

        func main()
        {
            let values: [i32; Buffer<3>.VALUE] = [7; Buffer<3>.VALUE];
            let copied: [i32; 3] = Buffer<3>.VALUES;

            assert(values[2] == copied[2]);
        }
        "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    for (name, expected) in [
        ("FIRST", 3),
        ("SECOND", 5),
        ("EXTRA_RESULT", 6),
        ("ELEMENT", 7),
    ] {
        let value = source_constant_value(&compilation, name);

        let ConstantValueKind::Integer(value) = value.kind() else {
            panic!("fixture constant is an integer");
        };

        assert_eq!(value.to_u64(), Some(expected), "{name}");
    }

    let lowered = compilation
        .lowered_unit(source_function_body_key(&compilation, "main"))
        .expect("associated constant value references must lower");

    assert!(
        lowered.diagnostics().is_empty(),
        "{:#?}",
        lowered.diagnostics()
    );
}

#[test]
fn associated_constants_substitute_the_enclosing_constant_call() {
    let compilation = compilation(
        r#"
        module app;

        struct Buffer<const N: usize>
        {
            const VALUE: usize = N;
        }

        const func capacity<const M: usize>() -> usize
        {
            return Buffer<M>.VALUE;
        }

        const FIRST: usize = capacity<3>();
        const SECOND: usize = capacity<5>();
        "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    for (name, expected) in [("FIRST", 3), ("SECOND", 5)] {
        let value = source_constant_value(&compilation, name);

        let ConstantValueKind::Integer(value) = value.kind() else {
            panic!("fixture constant is an integer");
        };

        assert_eq!(value.to_u64(), Some(expected), "{name}");
    }
}

#[test]
fn associated_constants_select_trait_overrides_and_defaults() {
    let compilation = compilation(
        r#"
        module app;

        trait Capacity
        {
            const VALUE: usize = 2;
        }

        struct Small
        {
        }

        struct Large
        {
        }

        impl Small(Capacity)
        {
        }

        impl Large(Capacity)
        {
            const VALUE: usize = 5;
        }

        const FIRST: usize = Small(Capacity).VALUE;
        const SECOND: usize = Large(Capacity).VALUE;

        func main()
        {
            let values: [i32; 5] = [7; Large(Capacity).VALUE];
            let capacity: usize = Small(Capacity).VALUE;
        }
        "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    for (name, expected) in [("FIRST", 2), ("SECOND", 5)] {
        let value = source_constant_value(&compilation, name);

        let ConstantValueKind::Integer(value) = value.kind() else {
            panic!("fixture constant is an integer");
        };

        assert_eq!(value.to_u64(), Some(expected), "{name}");
    }

    let lowered = compilation
        .lowered_unit(source_function_body_key(&compilation, "main"))
        .expect("trait constant value references must lower");

    assert!(
        lowered.diagnostics().is_empty(),
        "{:#?}",
        lowered.diagnostics()
    );
}

#[test]
fn associated_constants_defer_generic_trait_selection() {
    let compilation = compilation(
        r#"
        module app;

        trait Capacity
        {
            const VALUE: usize = 2;
        }

        struct Small
        {
        }

        struct Large
        {
        }

        impl Small(Capacity)
        {
        }

        impl Large(Capacity)
        {
            const VALUE: usize = 5;
        }

        const func capacity<T>() -> usize with(T: Capacity)
        {
            return T(Capacity).VALUE;
        }

        const FIRST: usize = capacity<Small>();
        const SECOND: usize = capacity<Large>();
    "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    for (name, expected) in [("FIRST", 2), ("SECOND", 5)] {
        let value = source_constant_value(&compilation, name);

        let ConstantValueKind::Integer(value) = value.kind() else {
            panic!("fixture constant is an integer");
        };

        assert_eq!(value.to_u64(), Some(expected), "{name}");
    }
}

#[test]
fn associated_constants_keep_instantiations_distinct_in_conditions() {
    let compilation = compilation(
        r#"
        module app;

        struct Buffer<const N: usize>
        {
            const VALUE: usize = N;
        }

        total func distinct()
        {
            assert(Buffer<3>.VALUE != Buffer<5>.VALUE);
        }
    "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn associated_constants_report_rejected_and_recovered_references() {
    for expression in [
        "Buffer<3>.MISSING",
        "Buffer<true>.VALUE",
        "buffer.VALUE",
        "Buffer<3>(Capacity).VALUE",
        "Buffer<3>(Capacity).MISSING",
        "buffer(Capacity).VALUE",
    ] {
        let source = format!(
            r#"
            module app;

            trait Capacity
            {{
                const VALUE: usize;
            }}

            struct Buffer<const N: usize>
            {{
                const VALUE: usize = N;
            }}

            func rejected(pos buffer: Buffer<3>)
            {{
                let value: usize = {expression};
            }}
        "#
        );

        let compilation = compilation(&source);

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "rejected"))
            .expect("invalid associated references recover with diagnostics");

        assert!(
            lowered.diagnostics().has_errors(),
            "{expression} must be rejected"
        );
    }
}

#[test]
fn associated_constants_in_trait_default_aggregates_use_selected_self() {
    let compilation = compilation(
        r#"
        module app;

        trait Capacity
        {
            const VALUE: usize;
            const VALUES: [i32; VALUE] = [7; VALUE];
        }

        struct Small
        {
        }

        impl Small(Capacity)
        {
            const VALUE: usize = 2;
        }

        const FIRST: i32 = Small(Capacity).VALUES[1];

        func main()
        {
            let values: [i32; 2] = Small(Capacity).VALUES;
        }
    "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let value = source_constant_value(&compilation, "FIRST");

    assert!(matches!(value.kind(), ConstantValueKind::Integer(value) if value.to_u64() == Some(7)));
}

#[test]
fn associated_constants_preserve_borrowed_trait_subjects() {
    let compilation = compilation(
        r#"
        module app;

        trait Capacity
        {
            const VALUE: usize;
        }

        struct Item
        {
        }

        impl Item(Capacity)
        {
            const VALUE: usize = 2;
        }

        impl &Item(Capacity)
        {
            const VALUE: usize = 5;
        }

        const func capacity<T>() -> usize with(T: Capacity)
        {
            return T(Capacity).VALUE;
        }

        const FIRST: usize = capacity<Item>();
        const SECOND: usize = capacity<(&Item)>();
    "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    for (name, expected) in [("FIRST", 2), ("SECOND", 5)] {
        let value = source_constant_value(&compilation, name);

        assert!(
            matches!(value.kind(), ConstantValueKind::Integer(value) if value.to_u64() == Some(expected)),
            "{name}"
        );
    }
}

#[test]
fn associated_constants_preserve_generic_trait_arguments() {
    let compilation = compilation(
        r#"
        module app;

        trait Scale<const K: usize>
        {
            const VALUE: usize = K;
        }

        struct Buffer<const N: usize>
        {
        }

        impl Buffer<M>(Scale<M>)
        {
        }

        const func capacity<T, const K: usize>() -> usize with(T: Scale<K>)
        {
            return T(Scale<K>).VALUE;
        }

        const FIRST: usize = capacity<Buffer<3>, 3>();
        const SECOND: usize = capacity<Buffer<5>, 5>();
    "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    for (name, expected) in [("FIRST", 3), ("SECOND", 5)] {
        let value = source_constant_value(&compilation, name);

        assert!(
            matches!(value.kind(), ConstantValueKind::Integer(value) if value.to_u64() == Some(expected)),
            "{name}"
        );
    }
}

#[test]
fn associated_constants_follow_type_valued_qualifiers() {
    let compilation = compilation(
        r#"
        module app;

        trait Capacity
        {
            const VALUE: usize;
        }

        trait Project
        {
            type Target;
        }

        struct Item<const N: usize>
        {
            const VALUE: usize = N;

            static const func read() -> usize
            {
                return N;
            }
        }

        impl Item<M>(Capacity)
        {
            const VALUE: usize = M;
        }

        impl &Item<M>(Capacity)
        {
            const VALUE: usize = M + 1;
        }

        struct Subjects<const N: usize>
        {
        }

        impl Subjects<M>
        {
            type Target = Item<M>;
            type Borrowed = &Item<M>;
        }

        impl Subjects<M>(Project)
        {
            type Target = Item<M>;
        }

        struct FixedSubjects
        {
        }

        impl FixedSubjects
        {
            type Target = Item<3>;
        }

        const func capacity<const N: usize>() -> usize
        {
            return Subjects<N>.Target(Capacity).VALUE;
        }

        func projected<T>() -> usize with(T: Project, T(Project).Target: Capacity)
        {
            return T(Project).Target(Capacity).VALUE;
        }

        const FIRST: usize = Subjects<3>.Target.VALUE;
        const SECOND: usize = Subjects<5>.Target(Capacity).VALUE;
        const BORROWED: usize = Subjects<5>.Borrowed(Capacity).VALUE;
        const PROJECTED: usize = Subjects<3>(Project).Target(Capacity).VALUE;
        const GENERIC: usize = capacity<5>();
        const CALLED: usize = Subjects<5>.Target.read();

        func main()
        {
            let values: [i32; Subjects<3>.Target.VALUE] = [7; Subjects<3>.Target.VALUE];
            let observed: usize = Subjects<5>.Target(Capacity).VALUE;
            let called: usize = Subjects<5>.Target.read();
            let deferred: usize = projected<Subjects<5>>();
            let constructed = FixedSubjects.Target {};
            let borrowed = &constructed;
        }
    "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    for (name, expected) in [
        ("FIRST", 3),
        ("SECOND", 5),
        ("BORROWED", 6),
        ("PROJECTED", 3),
        ("GENERIC", 5),
        ("CALLED", 5),
    ] {
        let value = source_constant_value(&compilation, name);

        assert!(
            matches!(value.kind(), ConstantValueKind::Integer(value) if value.to_u64() == Some(expected)),
            "{name}"
        );
    }

    let lowered = compilation
        .lowered_unit(source_function_body_key(&compilation, "main"))
        .expect("type-qualified references lower");

    assert!(
        lowered.diagnostics().is_empty(),
        "{:#?}",
        lowered.diagnostics()
    );
}

#[test]
fn type_valued_qualifiers_reject_runtime_uses() {
    for expression in [
        "Subjects.Target",
        "subjects.Target",
        "subjects.Target.VALUE",
        "Subjects.Target.field",
    ] {
        let source = format!(
            r#"
        module app;

        struct Item
        {{
            field: usize;
            const VALUE: usize = 3;
        }}

        struct Subjects
        {{
        }}

        impl Subjects
        {{
            type Target = Item;
        }}

        func rejected(pos subjects: Subjects)
        {{
            let value: Item = {expression};
        }}
        "#
        );

        let compilation = compilation(&source);

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "rejected"))
            .expect("invalid type qualifiers recover");

        assert!(
            lowered
                .diagnostics()
                .by_kind(bray_diagnostics::DiagnosticKind::CheckingNoApplicableCandidate)
                .next()
                .is_some(),
            "{expression}: {:#?}",
            lowered.diagnostics()
        );
    }
}

#[test]
fn associated_constants_preserve_syntax_recovery_diagnostics() {
    let compilation = compilation(
        r#"
        module app;

        struct Buffer<const N: usize>
        {
            const VALUE: usize = N;
        }

        func main()
        {
            let value: usize = Buffer<3>.VALUE +;
        }
    "#,
    );

    assert!(compilation.check_diagnostics().has_errors());
}
