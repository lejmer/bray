use crate::CancellationToken;
use bray_codegen::{CodegenOptions, CodegenSpecialization};
use std::collections::BTreeSet;

pub(in super::super) const CONCRETE_GENERIC_SOURCE: &str = concat!(
    "module app;\n",
    "\n",
    "func entry()\n",
    "{\n",
    "    main();\n",
    "}\n",
    "\n",
    "func accept<T>(pos value: T) -> T\n",
    "{\n",
    "    return value;\n",
    "}\n",
    "\n",
    "func repeat<const count: usize>() -> usize\n",
    "{\n",
    "    return count;\n",
    "}\n",
    "\n",
    "func main()\n",
    "{\n",
    "    let accepted: i32 = accept<i32>(1);\n",
    "    let repeated: usize = repeat<2>();\n",
    "}\n",
);

pub(in super::super) fn concrete_generic_specializations(
    compilation: &crate::Compilation,
) -> BTreeSet<CodegenSpecialization> {
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

    compilation
        .codegen_reachability(
            roots,
            None,
            &target,
            CodegenOptions::default(),
            false,
            &cancellation,
        )
        .unwrap_or_else(|error| panic!("generic reachability must close: {error:?}"))
        .graph()
        .instances()
        .iter()
        .filter_map(|instance| match instance.key().specialization() {
            CodegenSpecialization::Generic(_) => Some(instance.key().specialization().clone()),
            CodegenSpecialization::NonGeneric => None,
        })
        .collect()
}
