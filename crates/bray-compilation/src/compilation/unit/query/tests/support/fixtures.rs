use crate::Compilation;
use crate::test_support::compilation;

pub(in crate::compilation::unit::query::tests) fn pattern_compilation(body: &str) -> Compilation {
    compilation(&format!("module app;\nfunc main()\n{{\n{body}}}\n",))
}

pub(in crate::compilation::unit::query::tests) fn callable_compilation() -> Compilation {
    compilation(
        r#"module app;
func main()
{
}
"#,
    )
}

pub(in crate::compilation::unit::query::tests) fn custom_index_storage_compilation(
    body: &str,
) -> Compilation {
    compilation(&format!(
        r#"module app;

struct Item
{{
    mut value: i32;
}}

struct Values
{{
    mut first: Item;
    mut second: Item;
}}

impl Values(ElementIndex<i32>)
{{
    type Output = Item;

    func index(pos selector: &i32) -> &Item
    {{
        return &self.first;
    }}
}}

impl Values(MutableElementIndex<i32>)
{{
    type Output = Item;

    mut func index(pos selector: &i32) -> &mut Item
    {{
        return &mut self.second;
    }}
}}

{body}"#
    ))
}
