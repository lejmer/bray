    use std::sync::Arc;

    use bray_diagnostics::DiagnosticKind;
    use bray_symbols::{GenericConstraintObligationKey, GenericOwnerId};

    use crate::test_support::{compilation, diagnostic_kinds};

    #[test]
    fn concrete_generic_constraints_participate_in_callable_selection() {
        let accepted = compilation(concat!(
            "module app;\n",
            "\n",
            "func constrained() with(true)\n",
            "{\n",
            "}\n",
            "\n",
            "func main()\n",
            "{\n",
            "    constrained();\n",
            "}\n",
        ));

        assert!(
            accepted.check_diagnostics().is_empty(),
            "{:?}",
            accepted.check_diagnostics()
        );

        let rejected = compilation(concat!(
            "module app;\n",
            "\n",
            "func constrained() with(false)\n",
            "{\n",
            "}\n",
            "\n",
            "func main()\n",
            "{\n",
            "    constrained();\n",
            "}\n",
        ));

        assert!(
            diagnostic_kinds(rejected.check_diagnostics())
                .contains(&DiagnosticKind::CheckingNoApplicableCandidate)
        );
    }

    #[test]
    fn copyable_constraints_apply_inside_generic_bodies_and_at_instantiation() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "func duplicate<T>(pos value: T) -> T\n",
            "    with(T: Copyable)\n",
            "{\n",
            "    let first: T = value;\n",
            "    let second: T = value;\n",
            "\n",
            "    return second;\n",
            "}\n",
            "\n",
            "func main()\n",
            "{\n",
            "    let result: i32 = duplicate<i32>(1);\n",
            "}\n",
        ));

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );
    }

    #[test]
    fn copyable_constraints_reject_non_copyable_instantiations() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "struct Resource\n",
            "{\n",
            "    value: i32;\n",
            "}\n",
            "\n",
            "func duplicate<T>(pos value: T) -> T\n",
            "    with(T: Copyable)\n",
            "{\n",
            "    return value;\n",
            "}\n",
            "\n",
            "func main()\n",
            "{\n",
            "    let resource: Resource = Resource { value = 1 };\n",
            "    let duplicate: Resource = duplicate<Resource>(resource);\n",
            "}\n",
        ));

        assert!(
            diagnostic_kinds(compilation.check_diagnostics())
                .contains(&DiagnosticKind::CheckingNoApplicableCandidate)
        );
    }

    #[test]
    fn trait_satisfaction_constraints_select_exact_implementations() {
        let accepted = compilation(concat!(
            "module app;\n",
            "\n",
            "trait Marker {}\n",
            "\n",
            "struct Resource\n",
            "{\n",
            "    value: i32;\n",
            "}\n",
            "\n",
            "impl Resource(Marker) {}\n",
            "\n",
            "func accept<T>(pos value: T) -> T\n",
            "    with(T: Marker)\n",
            "{\n",
            "    return value;\n",
            "}\n",
            "\n",
            "func main()\n",
            "{\n",
            "    let resource: Resource = Resource { value = 1 };\n",
            "    let accepted: Resource = accept<Resource>(resource);\n",
            "}\n",
        ));

        assert!(
            accepted.check_diagnostics().is_empty(),
            "{:#?}",
            accepted.check_diagnostics()
        );

        let rejected = compilation(concat!(
            "module app;\n",
            "\n",
            "trait Marker {}\n",
            "\n",
            "func accept<T>(pos value: T) -> T\n",
            "    with(T: Marker)\n",
            "{\n",
            "    return value;\n",
            "}\n",
            "\n",
            "func main()\n",
            "{\n",
            "    let rejected: i32 = accept<i32>(1);\n",
            "}\n",
        ));

        assert!(
            diagnostic_kinds(rejected.check_diagnostics())
                .contains(&DiagnosticKind::CheckingNoApplicableCandidate)
        );
    }

    #[test]
    fn generic_constraint_results_are_cached_by_exact_substitution() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "func constrained() with(true)\n",
            "{\n",
            "}\n",
            "\n",
            "func main()\n",
            "{\n",
            "    constrained();\n",
            "}\n",
        ));

        let graph = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("symbol graph must be available: {error:?}"));

        let function = graph
            .functions()
            .iter()
            .find(|function| {
                graph
                    .member_name(function.id().into())
                    .is_some_and(|name| name.as_str() == "constrained")
            })
            .unwrap_or_else(|| panic!("constrained function must be declared"));

        let symbol = function.id().into();

        let owner = GenericOwnerId::try_new(symbol)
            .unwrap_or_else(|| panic!("function must be a generic owner"));

        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must be available: {error:?}"));

        let substitution = super::super::substitution::empty_substitution(values, symbol)
            .unwrap_or_else(|error| panic!("empty substitution must be available: {error:?}"));

        let obligation = GenericConstraintObligationKey::new(owner, substitution);

        let first = compilation
            .generic_constraint_satisfaction(obligation)
            .unwrap_or_else(|error| panic!("constraint result must be available: {error:?}"));

        let second = compilation
            .generic_constraint_satisfaction(obligation)
            .unwrap_or_else(|error| panic!("constraint result must be reusable: {error:?}"));

        assert!(Arc::ptr_eq(&first, &second));
    }
