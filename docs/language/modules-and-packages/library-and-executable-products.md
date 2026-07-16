# Library and executable products

## Library products

A library product exposes the package's reachable public declaration graph.

The reachable public declaration graph is computed from the selected source graph after module visibility, declaration visibility,
internal access, using declarations, exports, re-exports, implementation coherence, overload declarations, and ordinary path
resolution have been checked.

An internal module or declaration is not part of the public library surface unless a public wrapper or public re-export exposes a
public declaration whose signature does not require internal access.

A library product has no runtime entry point.

An `@entrypoint` directive in a library product source graph is rejected.

## Executable products

An executable product has exactly one resolved entry point.

An explicit entry point is declared with `@entrypoint` immediately before a module-level function declaration:

```bray
@entrypoint
func main() -> Result<unit, StartupError>
{
    ...
}
```

`@entrypoint` is valid only in an executable product source graph.

At most one `@entrypoint` directive can appear in a single executable product source graph.

When an executable product source graph contains no explicit `@entrypoint`, the compiler resolves a standard entry point by looking
for exactly one module-level function named `main` that satisfies the entry point contract.

If no valid entry point exists, the executable product is rejected.

If more than one valid entry point exists, the executable product is rejected.

An entry point function:

- is a module-level function declaration,
- has no receiver,
- has no generic parameters,
- has no caller-supplied parameters,
- returns `unit`, `Result<unit, E>`, or `i32`,
- does not expose trusted caller obligations,
- is not `const`.

An entry point can be `async`. An executable with an async entrypoint selects exactly one conforming async runtime implementation
and runtime ABI version through its product configuration.

Product configuration also selects the root parallel-resource limits available to `std.parallel` for runtime tasks, native threads,
and child processes. These limits are host-owned capacity authorities, not implicit queries of the build or execution machine.

For an entry point with no explicit result type, the result type is `unit`.

`unit` completion is successful executable completion.

`Result.Ok(unit)` completion is successful executable completion.

`Result.Error(error)` completion is failed executable completion with `error` as the reported entry failure value.

An `i32` result is the executable's numeric exit result.

The product host owns the executable root run. It observes that run as if it produced `RunResult<T>` and maps successful completion,
failed completion, panic completion, cancellation completion, and numeric exit results to the host process or embedding
environment.

For an async entrypoint, the compiler emits a host stub that invokes the entrypoint to create `Future<T>`, transfers its hidden frame
into the host-owned runtime root task on the distinguished main-thread lane, drives that task to terminal completion, performs
structured product shutdown for every root-owned task, thread, process, and lifecycle obligation, and performs the same result
mapping. No source-level runtime value, runtime import, or inner async block is synthesized.

The async entrypoint body is the root lexical structured task scope.

The host process and its initial operating-system thread do not produce source-visible `std.process.Process<T>` or
`std.thread.Thread<T>` values. Those standard-library types represent source ownership of child runs created within the product.

The complete root-run, main-thread, terminal-observation, and product-shutdown rules are defined by
[Execution roots and product shutdown](../async-and-concurrency/execution-roots-and-product-shutdown.md).

`@entrypoint` does not change a function's name, module, visibility, callable type, ABI, contract, overload participation, or
export behavior.

## Navigation

- [Language index](../index.md)
- [Modules and packages index](../modules-and-packages.md)
- Previous: [Conditional module contributions](conditional-module-contributions.md)
- Next: [Test products and entries](test-products-and-entries.md)
