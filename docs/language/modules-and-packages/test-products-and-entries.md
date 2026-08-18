# Test products and entries

## Test products

A test product is a package product whose selected source graph is checked and executed as independent test entries.

Test products can include ordinary source inputs, test-only source inputs, ordinary dependencies, and test-only
dependencies selected for that product.

Test-only source inputs and test-only dependencies participate only in test products that select them.

They do not contribute declarations, dependencies, implementations, overloads, conversions, public API, or
coherence-domain behavior to library or executable products.

A package integration-test product may select a library product from the same package as its tested library. The test
product consumes that library product through its public compiled contract. Library source files are not compiled as
part of the test product, so declarations that are absent from the public contract are unavailable unless another
language access rule explicitly provides them.

Test products use ordinary module declarations, path resolution, dependency checking, visibility checking, internal
access rules, trusted-module rules, target gates, contract checking, ownership checking, and
[async and run-boundary rules](../async-and-concurrency.md).

An `@entrypoint` directive in a test product source graph is rejected because test products form test entries through
`@test`.

A test product can contain zero or more test entries.

## Test-only module contributions

A module contribution can be marked test-only with `@test`.

`@test` attaches to a source-unit module declaration or a block module declaration.

Only one `@test` directive can apply to a module declaration.

```bray
@test
module net.tests;

using net.parser;

func minimal_packet_bytes() -> &[u8]
{
    ...
}

@test
func parses_minimal_packet()
{
    let packet = net.parser.parse_packet(minimal_packet_bytes());
    assert(packet.kind == PacketKind.minimal);
}
```

`@test` enables the module contribution for test products and disables it for library and executable products.

`@test` does not change module identity, module visibility, trusted-module state, declaration visibility, path
resolution, internal access, trusted capability access, or runtime behavior.

`@test` and `@target(...)` can both apply to the same module declaration according to conditional module contribution
rules.

## Test declarations

A module-level function declaration can be marked as a test entry with `@test`.

```bray
@test
func parses_minimal_packet()
{
    ...
}
```

An `@test` function contributes only to test products.

For non-test products, an `@test` function contributes no declaration and its body is not semantically checked.

An `@test` function must still be lexically and syntactically valid Bray source.

Only one `@test` directive can apply to a function declaration.

`@test(serial)` marks a function test entry as command-wide serial. Bare `@test` marks it as parallel. A serial entry
cannot overlap another test entry selected by the same test command, including an entry from another package or product.
`serial` is not permitted on a module-level `@test` directive, and no other test-directive argument is defined.

```bray
@test(serial)
func uses_process_global_state()
{
    ...
}
```

An `@test` function:

- is a module-level function declaration,
- has no receiver,
- has no generic parameters,
- has no caller-supplied parameters,
- returns `unit` or `Result<unit, E>`,
- does not expose trusted caller obligations,
- is not `const`.

An `@test` function can be `async`. A test product containing async tests selects one conforming runtime implementation
and runtime ABI version through its product configuration.

For an `@test` function with no explicit result type, the result type is `unit`.

Normal trusted implementation rules apply to test functions.

`@test` does not grant trusted implementation capabilities.

`@test` does not grant internal access.

If a test needs internal access, it uses ordinary internal-use acknowledgement.

```bray
using internal net.parser.impl;
```

An `@test` function can call trusted declarations only when ordinary trusted caller obligation rules are satisfied.

## Test entry formation

Test discovery happens after source graph selection, target-gated contribution selection, test-only contribution
selection, module merging, and declaration checking.

Each enabled `@test` function forms one test entry.

The test entry identity is the function's fully qualified declaration path.

Test entry formation is not ordinary external path access.

It does not make the test function public.

It does not export the test function from its module.

It does not make the test function visible in any other module.

Helper functions in test-only modules are ordinary functions unless they are marked `@test`.

Test execution order is not language-defined. The serial constraint provides exclusion, not a specified position
relative to other entries.

Each test entry is reported independently.

Shared mutable state between tests must be mediated by ordinary synchronization, atomic, ownership, borrowing, internal
access, and trusted contract rules.

## Test execution outcomes

A test entry is executed behind a panic-catching run boundary owned by the test product.

A synchronous test that completes with `unit` passes.

A synchronous test that completes with `Result.Ok(unit)` passes.

A synchronous test that completes with `Result.Error(error)` fails with `error` as its recoverable test failure value.

A synchronous test that panics fails with the caught `PanicReport`.

An async test invocation creates `Future<T>` and is driven as a root task by the test product's selected runtime.

An async test whose run completes with `unit` or `Result.Ok(unit)` passes.

An async test whose run completes with `Result.Error(error)` fails with `error` as its recoverable test failure value.

An async test whose run boundary reports `RunResult.Panicked(report)` fails with `report`.

An async test whose run boundary reports `RunResult.Cancelled` is reported as cancelled.

Children created by a test obey ordinary [task](../async-and-concurrency/task-handles-and-obligations.md),
[standard-library thread and process](../async-and-concurrency/standard-library-concurrency.md), and
[structured scope exit](../async-and-concurrency/structured-task-scope-exit.md) rules. Each test root resolves every
owned child run before that test outcome is reported.

All selected test entries in one test-product activation share that product's product-static instances. Thread-local
statics remain per exact native-thread attachment. Static cleanup begins only after every test root has resolved, and a
static cleanup incident is reported as a test-product failure rather than attributed to one test.

Lifecycle, finalization, destruction, panic, cancellation, and cleanup behavior during test execution follows the
ordinary language rules.

## Navigation

- [Language index](../index.md)
- [Modules and packages index](../modules-and-packages.md)
- Previous: [Library and executable products](library-and-executable-products.md)
- Next: [Target constraints and gates](target-constraints-and-gates.md)
