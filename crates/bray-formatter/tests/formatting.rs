use std::num::NonZeroU16;

use bray_formatter::FormatterRule;
use bray_formatter::{
    FormatBytesErrorKind, FormattedSource, FormatterConfiguration, format_bytes, format_text,
};
use bray_parser::parse_source_unit;
use bray_testing::test_source_snapshot;

#[test]
fn formats_representative_declarations_and_expressions() {
    let source = concat!(
        "module app;using std.io;",
        "@copy public struct Point{x:r64;y:r64;}",
        "union Shape{Circle(center:Point,radius:r64);Empty;}",
        "trait Display{func show(pos value:Point);}",
        "impl Point(Display){func show(pos value:Point){",
        "if true{return value.x+1;}else{return 0;}}}",
        "func main(){let point:Point={x=1.0,y=2.0,};",
        "let values:[i32;3]=[1,2,3];",
        "match point{case _{return;}}}",
    );

    let output = formatted(source);

    assert_eq!(
        output.text(),
        concat!(
            "module app;\n",
            "\n",
            "using std.io;\n",
            "\n",
            "@copy\n",
            "public struct Point\n",
            "{\n",
            "    x: r64;\n",
            "    y: r64;\n",
            "}\n",
            "\n",
            "union Shape\n",
            "{\n",
            "    Circle(center: Point, radius: r64);\n",
            "    Empty;\n",
            "}\n",
            "\n",
            "trait Display\n",
            "{\n",
            "    func show(pos value: Point);\n",
            "}\n",
            "\n",
            "impl Point(Display)\n",
            "{\n",
            "    func show(pos value: Point)\n",
            "    {\n",
            "        if true\n",
            "        {\n",
            "            return value.x + 1;\n",
            "        }\n",
            "        else\n",
            "        {\n",
            "            return 0;\n",
            "        }\n",
            "    }\n",
            "}\n",
            "\n",
            "func main()\n",
            "{\n",
            "    let point: Point =\n",
            "    {\n",
            "        x = 1.0,\n",
            "        y = 2.0,\n",
            "    };\n",
            "    let values: [i32; 3] = [1, 2, 3];\n",
            "    match point\n",
            "    {\n",
            "        case _ { return; }\n",
            "    }\n",
            "}\n",
        )
    );
}

#[test]
fn formatting_is_idempotent_across_representative_grammar() {
    let sources = [
        "module app;",
        "module app; func add(left:i32,right:i32)->i32{return left+right;}",
        "module app; struct Box<T>{value:T;} impl Box<T>{func get()->&T{return &self.value;}}",
        "module app; predicate positive<T,const N:i32>(value:T)=value>0;",
        "module app; overload parse={parse_int,parse_float,}",
        "module app; func main(){let pair:(i32,r64)=(1,2.0);let a=[1,2,3];}",
        "module app; func main(){while ready{continue;}for item in items{break;}loop{break;}}",
        "module app; func main(){let value=match input{case Some(x){yield x;}case None{yield 0;}};}",
    ];

    for source in sources {
        let first = formatted(source);
        let second = formatted(first.text());

        assert_eq!(second.text(), first.text(), "{source}");
        assert!(!second.changed(), "{source}");
    }
}

#[test]
fn repository_programs_remain_valid_and_idempotent() {
    let sources = [
        include_str!("../../../examples/hello_world/src/main.bray"),
        include_str!("../../../xtask/fixtures/native-execution/control-flow.bray"),
        include_str!("../../../xtask/fixtures/native-execution/abi-primitive.bray"),
    ];

    for source in sources {
        let output = formatted(source);
        let snapshot = test_source_snapshot(output.text());
        let parsed = parse_source_unit(&snapshot);

        assert!(
            parsed.diagnostics().is_empty(),
            "formatted repository program produced diagnostics"
        );

        assert!(
            !parsed.source_unit().is_recovered(),
            "formatted repository program required recovery"
        );

        assert!(!formatted(output.text()).changed());
    }
}

#[test]
fn preserves_comment_text_and_positions_comments_deterministically() {
    let source = concat!(
        "// file header\n",
        "module   app; /* module note */\n",
        "\n",
        "/// adds values\n",
        "@test // directive note\n",
        "func add(left:i32,/* right value */right:i32)->i32",
        "{return left+right; // sum\n",
        "}\n",
        "// final comment",
    );

    let output = formatted(source);

    for comment in [
        "// file header",
        "/* module note */",
        "/// adds values",
        "// directive note",
        "/* right value */",
        "// sum",
        "// final comment",
    ] {
        assert_eq!(output.text().matches(comment).count(), 1, "{comment}");
    }

    let second = formatted(output.text());

    assert_eq!(second.text(), output.text());
}

#[test]
fn preserves_trailing_whitespace_in_exact_line_comments() {
    let source = concat!(
        "module app; // ordinary  \n",
        "/// documentation\t \n",
        "func main(){}",
    );

    let output = formatted(source);

    assert!(output.text().contains("// ordinary  \n"));
    assert!(output.text().contains("/// documentation\t \n"));
    assert!(!formatted(output.text()).changed());
}

#[test]
fn preserves_skipped_and_invalid_recovery_text() {
    let source = "module app; func main(){let value=$ badly ???;return value;}";
    let output = formatted(source);

    assert_eq!(output.text(), source);
    assert!(!output.changed());

    let reformatted = formatted(output.text());

    assert_eq!(reformatted.text(), output.text());
}

#[test]
fn formats_generic_delimiters_prefix_operators_and_inline_collections() {
    let source = concat!(
        "module app;",
        "@test @abi(\"C\") public async func transform<T,const N:i32>",
        "(pos value:i32=1,mut tail:bool,)->unit{}",
        "func negate(value:i32)->i32{return -value;}",
    );

    let output = formatted(source);

    assert_eq!(
        output.text(),
        concat!(
            "module app;\n",
            "\n",
            "@test\n",
            "@abi(\"C\")\n",
            "public async func transform<T, const N: i32>(pos value: i32 = 1, mut tail: bool) -> unit\n",
            "{\n",
            "}\n",
            "\n",
            "func negate(value: i32) -> i32\n",
            "{\n",
            "    return -value;\n",
            "}\n",
        )
    );
}

#[test]
fn separates_binary_operators_from_prefix_operands() {
    let source = concat!(
        "module app;",
        "func main(){",
        "let borrowed=left& &right;",
        "let negated=left+ -right;",
        "}",
    );

    let output = formatted(source);

    assert!(output.text().contains("left & &right"));
    assert!(output.text().contains("left + -right"));

    let snapshot = test_source_snapshot(output.text());
    let parsed = parse_source_unit(&snapshot);

    assert!(parsed.diagnostics().is_empty());
    assert!(!parsed.source_unit().is_recovered());
    assert!(!formatted(output.text()).changed());
}

#[test]
fn separates_mutable_slice_types_from_their_element_list() {
    let source = "module app; func read(pos bytes: &mut[u8]){}";
    let output = formatted(source);

    assert!(output.text().contains("pos bytes: &mut [u8]"));
    assert!(!formatted(output.text()).changed());
}

#[test]
fn removes_trailing_commas_when_lists_flatten() {
    let source = concat!(
        "module app;\n",
        "extern const func layout_of<T>(\n",
        "    count: usize,\n",
        ") -> Result<MemoryLayout, MemoryLayoutError>;\n",
    );

    let output = formatted(source);

    assert!(
        output
            .text()
            .contains("layout_of<T>(count: usize) -> Result<MemoryLayout, MemoryLayoutError>")
    );

    assert!(!formatted(output.text()).changed());
}

#[test]
fn preserves_lf_and_crlf_line_ending_styles() {
    let lf = formatted("module app;\nfunc main(){return;}\n");
    let crlf = formatted("module app;\r\nfunc main(){return;}\r\n");

    assert!(!lf.text().contains("\r\n"));
    assert!(lf.text().contains('\n'));

    assert!(crlf.text().contains("\r\n"));
    assert!(!crlf.text().replace("\r\n", "").contains('\n'));
}

#[test]
fn disabled_rules_preserve_source_policy_while_other_rules_apply() {
    let source = "module app;\r\n\r\nfunc main()\r\n{\r\n    return left+right;\r\n}";

    let configuration = FormatterConfiguration::default()
        .with_rule(FormatterRule::OperatorSpacing, false)
        .with_rule(FormatterRule::LineEndingStyle, false)
        .with_rule(FormatterRule::FinalNewline, false);

    let output = formatted_with_configuration(source, &configuration);

    assert_eq!(
        output.text(),
        "module app;\n\nfunc main()\n{\n    return left+right;\n}"
    );

    assert!(!formatted_with_configuration(output.text(), &configuration).changed());
}

#[test]
fn opt_in_nested_conditionals_simplify_only_with_syntax_local_proof() {
    let configuration =
        FormatterConfiguration::default().with_rule(FormatterRule::SimplifyNestedIf, true);

    let source = concat!(
        "module app;",
        "func main(){if ready{if enabled{if available{return;}}}}",
    );

    let output = formatted_with_configuration(source, &configuration);

    assert!(output.text().contains("if ready && enabled && available"));
    assert_eq!(output.text().matches("if ").count(), 1);
    assert!(!formatted_with_configuration(output.text(), &configuration).changed());

    let default_output = formatted(source);

    assert!(!default_output.text().contains("&&"));
    assert_eq!(default_output.text().matches("if ").count(), 3);
}

#[test]
fn nested_conditionals_retain_forms_that_need_semantic_or_comment_reasoning() {
    let configuration =
        FormatterConfiguration::default().with_rule(FormatterRule::SimplifyNestedIf, true);

    let sources = [
        "module app;func main(){if ready(){if enabled{return;}}}",
        "module app;func main(){if ready{if enabled{return;}else{return;}}}",
        "module app;func main(){if ready{// retained\nif enabled{return;}}}",
        "module app;func main(){if ready{observe();if enabled{return;}}}",
    ];

    for source in sources {
        let output = formatted_with_configuration(source, &configuration);

        assert_eq!(output.text().matches("if ").count(), 2, "{source}");
        assert!(!formatted_with_configuration(output.text(), &configuration).changed());
    }
}

#[test]
fn keeps_simple_match_arm_bodies_inline_when_they_fit() {
    let source = concat!(
        "module app;",
        "func choose(value:i32){",
        "match value{",
        "case 0{return;}",
        "case 1{}",
        "case _{observe();return;}",
        "case 2{if value{return;}return;}",
        "case 3{return;}",
        "}",
        "}",
    );

    let output = formatted(source);

    assert!(
        output.text().contains("case 0 { return; }"),
        "{}",
        output.text()
    );

    assert!(output.text().contains("case 1 {}"), "{}", output.text());

    assert!(
        output.text().contains("case _\n        {"),
        "{}",
        output.text()
    );

    assert!(
        output
            .text()
            .contains("        }\n        case 3 { return; }"),
        "{}",
        output.text()
    );

    assert!(!formatted(output.text()).changed());

    let narrow = formatted_with_width(
        concat!(
            "module app; func choose(value: i32) { match value {",
            "case 123456789 { return; }",
            "case ?present { return; }",
            "} }",
        ),
        33,
    );

    assert!(
        narrow.text().contains("case 123456789\n        {"),
        "{}",
        narrow.text()
    );

    assert!(narrow.text().contains("case ?present"), "{}", narrow.text());

    assert!(!narrow.text().contains("\n        \n"), "{}", narrow.text());

    assert!(!formatted_with_width(narrow.text(), 33).changed());

    let multiline_configuration =
        FormatterConfiguration::default().with_rule(FormatterRule::MatchArmBodyLayout, false);

    let multiline = formatted_with_configuration(
        "module app; func choose(value: i32) { match value { case 0 { return; } } }",
        &multiline_configuration,
    );

    assert!(multiline.text().contains("case 0\n        {"));
}

#[test]
fn wraps_parenthesized_and_bracketed_lists_at_configured_width() {
    let source = concat!(
        "module app;",
        "func collect(first:i32,second:i32,third:i32){",
        "let values=[first,second,third,];",
        "collect(first,second,third);",
        "}",
    );

    let output = formatted_with_width(source, 36);

    assert_eq!(
        output.text(),
        concat!(
            "module app;\n",
            "\n",
            "func collect(\n",
            "    first: i32,\n",
            "    second: i32,\n",
            "    third: i32\n",
            ")\n",
            "{\n",
            "    let values = [\n",
            "        first,\n",
            "        second,\n",
            "        third,\n",
            "    ];\n",
            "    collect(first, second, third);\n",
            "}\n",
        )
    );

    assert_eq!(
        formatted_with_width(output.text(), 36).text(),
        output.text()
    );
}

#[test]
fn wraps_complete_callable_headers_and_binary_chains() {
    let source = concat!(
        "module app;",
        "extern trusted func transform<Target,Source>",
        "(pos pointer:RawPointer<Source>,count:usize)->RawPointer<Target>",
        "requires(ready)uses(raw_memory);",
        "func combine()->u64{return first+second+third+fourth+fifth+sixth;}"
    );

    let output = formatted_with_width(source, 52);

    assert_eq!(
        output.text(),
        concat!(
            "module app;\n",
            "\n",
            "extern trusted func transform<Target, Source>(\n",
            "    pos pointer: RawPointer<Source>,\n",
            "    count: usize\n",
            ") -> RawPointer<Target>\n",
            "    requires(ready)\n",
            "    uses(raw_memory);\n",
            "\n",
            "func combine() -> u64\n",
            "{\n",
            "    return first + second + third + fourth + fifth +\n",
            "        sixth;\n",
            "}\n",
        )
    );

    assert_eq!(
        formatted_with_width(output.text(), 52).text(),
        output.text()
    );
}

#[test]
fn declaration_contract_clauses_use_continuation_lines() {
    let source = concat!(
        "module app;",
        "extern trusted func reinterpret<Target, Source>",
        "(pos pointer: RawPointer<Source>) -> RawPointer<Target> uses(layout_reinterpret);",
        "impl Buffer<T> with(T: Copyable){}",
    );

    let output = formatted(source);

    assert!(output.text().lines().all(|line| line.len() <= 120));

    assert!(
        output
            .text()
            .contains(") -> RawPointer<Target>\n    uses(layout_reinterpret);")
    );

    assert!(
        output
            .text()
            .contains("impl Buffer<T>\n    with(T: Copyable)\n{")
    );

    assert!(!formatted(output.text()).changed());
}

#[test]
fn empty_and_comment_only_standard_input_are_stable() {
    let empty = formatted("");
    let whitespace = formatted(" \t\r\n");
    let comment = formatted("// retained");

    assert_eq!(empty.text(), "");
    assert_eq!(whitespace.text(), " \t\r\n");
    assert_eq!(comment.text(), "// retained");

    assert!(!empty.changed());
    assert!(!whitespace.changed());
    assert!(!comment.changed());
    assert!(!formatted(comment.text()).changed());
}

#[test]
fn standard_input_text_returns_formatted_output_without_filesystem_state() {
    let output = formatted("module editor;func main(){return;}");

    assert!(output.changed());

    assert_eq!(
        output.text(),
        concat!(
            "module editor;\n",
            "\n",
            "func main()\n",
            "{\n",
            "    return;\n",
            "}\n",
        )
    );
}

#[test]
fn editor_text_preserves_utf8_byte_order_mark() {
    let output = formatted("\u{feff}module editor;func main(){}");

    assert!(output.changed());
    assert!(output.text().starts_with('\u{feff}'));

    assert_eq!(
        output.text().trim_start_matches('\u{feff}'),
        "module editor;\n\nfunc main()\n{\n}\n"
    );
}

#[test]
fn standard_input_bytes_preserve_bom_and_report_invalid_utf8() {
    let configuration = FormatterConfiguration::default();

    let output = match format_bytes(b"\xef\xbb\xbfmodule editor;func main(){}", &configuration) {
        Ok(output) => output,
        Err(error) => panic!("BOM input should format: {error:?}"),
    };

    assert!(output.text().as_bytes().starts_with(b"\xef\xbb\xbf"));

    let error = match format_bytes(&[b'm', 0xff, b'x'], &configuration) {
        Ok(output) => panic!("invalid UTF-8 should fail, got {output:?}"),
        Err(error) => error,
    };

    assert_eq!(error.kind(), FormatBytesErrorKind::InvalidUtf8);
    assert_eq!(error.byte_count(), 3);
    assert_eq!(error.invalid_utf8_at(), Some(1));
}

fn formatted(source: &str) -> FormattedSource {
    formatted_with_configuration(source, &FormatterConfiguration::default())
}

fn formatted_with_configuration(
    source: &str,
    configuration: &FormatterConfiguration,
) -> FormattedSource {
    match format_text(source, configuration) {
        Ok(formatted) => formatted,
        Err(error) => panic!("test source should fit in formatter ranges: {error:?}"),
    }
}

fn formatted_with_width(source: &str, width: u16) -> FormattedSource {
    let width = NonZeroU16::new(width).unwrap_or_else(|| unreachable!());
    let configuration = FormatterConfiguration::default().with_maximum_line_width(width);

    match format_text(source, &configuration) {
        Ok(formatted) => formatted,
        Err(error) => panic!("test source should fit in formatter ranges: {error:?}"),
    }
}
