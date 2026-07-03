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

An entry point can be `async` only when the executable product context supplies an async runtime contract for async entry
execution.

For an entry point with no explicit result type, the result type is `unit`.

`unit` completion is successful executable completion.

`Result.Ok(unit)` completion is successful executable completion.

`Result.Error(error)` completion is failed executable completion with `error` as the reported entry failure value.

An `i32` result is the executable's numeric exit result.

The product runtime contract maps successful completion, failed completion, panic completion, cancellation completion, and numeric
exit results to the host process or embedding environment.

`@entrypoint` does not change a function's name, module, visibility, callable type, ABI, contract, overload participation, or
export behavior.

## Navigation

- [Language index](../index.md)
- [Modules and packages index](../modules-and-packages.md)
- Previous: [Conditional module contributions](conditional-module-contributions.md)
- Next: [Test products and entries](test-products-and-entries.md)
