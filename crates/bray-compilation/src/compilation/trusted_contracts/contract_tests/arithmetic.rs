use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{compilation};

#[test]
fn spare_capacity_bounds_prove_unsigned_sums_without_wrapping_subtraction() {
    for (ty, bounds, valid) in [
        ("u8", "initialized <= capacity", true),
        ("u8", "true", false),
        ("r64", "initialized <= capacity", false),
    ] {
        let source = format!(
            r#"
            module app;
            func append_length(initialized: {ty}, count: {ty}, capacity: {ty}) -> {ty}
                requires({bounds})
                when(true) {{ ensures(result <= capacity) }}
            {{
                let available = capacity - initialized;
                if count > available {{ panic("exceeds spare capacity"); }}
                return initialized + count;
            }}
        "#
        );

        let compilation = compilation(&source);

        assert_eq!(
            !compilation.check_diagnostics().has_errors(),
            valid,
            "{source}: {:?}",
            compilation.check_diagnostics()
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(),
                DiagnosticKind::CheckingPostconditionNotProven,
            );
        }
    }

    let compilation = compilation(
        r#"
        trusted module app;

        using core.memory;
        struct Header
        {
            capacity: usize;
        }
        trusted func write_unknown_storage(pos header: &mut Header, pos destination: DataPointer<u8>) -> usize
            requires(
                trusted core.memory.valid_write<u8>(pointer = destination, count = 1),
                trusted core.memory.aligned_for<u8>(pointer = destination),
            )
            when(true) { ensures(result <= header.capacity) }
            uses(raw_memory, unchecked_init)
        {
            let capacity = header.capacity;
            trusted core.memory.write<u8>(destination, 0);
            return capacity;
        }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingPostconditionNotProven,
    );
}

#[test]
fn machine_products_retain_proven_bounds_across_boolean_joins() {
    let compound = r"
        if stride != 0 && capacity > maximum / stride
        {
            return;
        }
    ";
    let split = r"
        if stride != 0
        {
            if capacity > maximum / stride
            {
                return;
            }
        }
    ";

    for (source_value, guard, valid) in [
        ("supplied", "", false),
        ("supplied", compound, true),
        ("supplied", split, true),
        ("trusted opaque_stride()", compound, true),
    ] {
        let source = format!(r"
            trusted module app;

            struct Owner
            {{
            }}

            trusted predicate extent(owner: &Owner, bytes: usize);

            trusted func allocate(pos bytes: usize) -> Owner
                ensures(trusted extent(owner = &result, bytes = bytes))
            {{
                return Owner
                {{
                }};
            }}

            extern const func opaque_stride() -> usize;

            func inspect(pos owner: &Owner, pos stride: usize, pos capacity: usize)
                requires(
                    stride * capacity <= 18446744073709551615,
                    trusted extent(owner = owner, bytes = stride * capacity)
                )
            {{
            }}

            trusted func caller(pos supplied: usize, pos capacity: usize)
            {{
                let stride: usize = {source_value};
                let maximum: usize = 18446744073709551615;

                {guard}

                let bytes: usize = stride * capacity;
                let owner = allocate(bytes);

                inspect(&owner, stride, capacity);
            }}
        ");
        let compilation = compilation(&source);
        let diagnostics = compilation.check_diagnostics();

        assert_eq!(!diagnostics.has_errors(), valid, "source={source_value}, guard={guard}: {diagnostics:#?}");
        assert!(diagnostics.iter().all(|diagnostic| diagnostic.kind() == DiagnosticKind::CheckingTrustedObligationNotProven),
            "the negative must reach its ownership requirement: {diagnostics:#?}");
    }
}

#[test]
fn machine_arithmetic_totality_distinguishes_wrapping_from_trapping_operations() {
    for (operator, valid) in [
        ("+", true),
        ("-", true),
        ("*", true),
        ("/", false),
        ("%", false),
        ("<<", false),
    ] {
        for body in [
            format!("return left {operator} right;"),
            format!("let mut value: u64 = left; value {operator}= right; return value;"),
        ] {
            let source = format!(
                r#"
            module app;
            func arithmetic(left: u64, right: u64) -> u64
                executes(total)
            {{
                {body}
            }}
        "#
            );

            let compilation = compilation(&source);

            assert_eq!(
                !compilation.check_diagnostics().has_errors(),
                valid,
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }
}
