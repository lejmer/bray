use crate::Compilation;
use crate::test_support::compilation;
use bray_diagnostics::DiagnosticKind;
use bray_testing::assert_goal_state_diagnostic_kind;

#[test]
fn fixed_array_generators_report_divergent_yield_cardinality() {
    let compilation =
        array_generator_compilation(concat!("        yield item;\n", "        yield item;\n",));

    assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingArrayGeneratorCardinalityNotProvable,
    );
}

#[test]
fn fixed_array_generators_require_a_statically_known_source_count() {
    let compilation = array_generator_compilation("        yield item;\n");

    assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingArrayGeneratorCardinalityNotProvable,
    );
}

#[test]
fn fixed_array_generators_use_literal_range_cardinality() {
    let matching = compilation(concat!(
        "module app;\n",
        "func main()\n",
        "{\n",
        "    let generated: [i32; 4] = [each item in 0..4\n",
        "    {\n",
        "        yield item;\n",
        "    }];\n",
        "}\n",
    ));

    assert!(
        matching.check_diagnostics().is_empty(),
        "{:#?}",
        matching.check_diagnostics()
    );

    let mismatched = compilation(concat!(
        "module app;\n",
        "func main()\n",
        "{\n",
        "    let generated: [i32; 3] = [each item in 0..4\n",
        "    {\n",
        "        yield item;\n",
        "    }];\n",
        "}\n",
    ));

    assert_goal_state_diagnostic_kind(
        mismatched.check_diagnostics(),
        DiagnosticKind::CheckingArrayGeneratorCardinalityNotProvable,
    );
}

fn array_generator_compilation(body: &str) -> Compilation {
    let mut source = String::from(concat!(
        "module app;\n",
        "struct Items\n",
        "{\n",
        "}\n",
        "struct ItemsCursor\n",
        "{\n",
        "}\n",
        "impl &Items(Iterable)\n",
        "{\n",
        "    type Element = bool;\n",
        "    type Cursor = ItemsCursor;\n",
        "    consume func iterate() -> ItemsCursor\n",
        "    {\n",
        "    }\n",
        "}\n",
        "impl ItemsCursor(Iterator)\n",
        "{\n",
        "    type Element = bool;\n",
        "    mut func next() -> bool?\n",
        "    {\n",
        "    }\n",
        "}\n",
        "func main()\n",
        "{\n",
        "    let items: Items = Items {};\n",
        "    let generated: [bool; 2] = [each item in items\n",
        "    {\n",
    ));

    source.push_str(body);

    source.push_str(concat!("    }];\n", "}\n",));

    compilation(&source)
}
