# Module and package model

## Overview

Packages are build, versioning, distribution, and dependency units.

Modules are namespaces and source-organization units inside a package.

A package owns a set of modules.

A module owns declarations.

Module declarations are source declarations.

Package identity is not declared in source.

The current package is supplied by the package/build layer.

Source code declares modules inside the current package.

---

## Module paths

A module path is one or more identifiers separated by `.`.

```bray
net
net.http
net.http.tests
```

A module path declared in source is relative to the current package.

It does not include package identity.

When an external path begins with a package identity, that leading component is resolved as the package, not as part of the source-declared module path inside that package.

For example, if another package is known as `geometry`, source in that package can declare:

```bray
module shapes;
```

Other packages can refer to that module through the package path:

```bray
geometry.shapes.Circle
```

---

## Package identity

A package has a package identity supplied outside Bray source.

The compiler receives package identity from the package/build layer.

Package identity determines how other packages refer to the compiled package.

Package identity is not a module declaration.

Package identity is not imported by writing a source declaration with the same name.

Package dependencies are declared by the package/build layer, not by module declarations.

The package dependency graph is acyclic.

The module graph inside a package is a declaration graph and can be cyclic.

---

## Package products

A package can define one or more products.

A product is a selected compilation surface over the package's source graph and dependency graph.

The language-defined product kinds are:

- library,
- executable,
- test.

Package identity is shared by all products of the package.

Product identity, selected source inputs, selected dependencies, and target constraints are supplied by the package/build layer.

A product is not a module and does not create a namespace.

A source declaration can contribute to more than one product when it is present in each product's selected source graph and is valid
under each product's product kind and target constraints.

---

## Source graphs

A package product is compiled from an explicit source graph.

The source graph is the selected set of source inputs for that product.

The compiler checks only source inputs that are in the selected source graph.

Every source input in the source graph must be a Bray source file containing module declarations.

Module declarations define module identity.

File paths and source graph order do not define module identity.

Split module declarations are valid when all contributing source inputs are part of the same selected source graph and satisfy the
split module rules.

The same source input cannot appear more than once in the same product source graph.

Source graph construction is deterministic. The same package identity, product kind, selected source graph, selected dependency
graph, and target profile produce the same compiler input.

---

## Library products

A library product exposes the package's reachable public declaration graph.

The reachable public declaration graph is computed from the selected source graph after module visibility, declaration visibility,
internal access, using declarations, exports, re-exports, implementation coherence, overload declarations, and ordinary path
resolution have been checked.

An internal module or declaration is not part of the public library surface unless a public wrapper or public re-export exposes a
public declaration whose signature does not require internal access.

A library product has no runtime entry point.

An `@entrypoint` directive in a library product source graph is rejected.

---

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

---

## Test products

A test product is a package product whose selected source graph is checked and executed by the test model.

Test products can include ordinary source inputs, test-only source inputs, ordinary dependencies, and test-only dependencies selected
for that product.

The test model defines test declarations, test modules, test entry point formation, and test execution behavior.

---

## Target constraints

Target constraints are product constraints supplied by the package/build layer.

The compiler checks a product only when the selected target profile satisfies that product's target constraints.

Target facts exposed to Bray source are ordinary compiler-known facts of the selected target profile.

When a product's target constraints are not satisfied, the product is rejected before module bodies are checked.

---

## Module declarations

Every source file must declare its file-scoped module.

The file-scoped module declaration syntax is:

```bray
module net;
```

In grammar terms:

```text
module-modifiers 'module' module-path ';'
```

where:

```text
module-modifiers = ['trusted'] [visibility]
```

The canonical modifier order is `trusted` before visibility.

The file-scoped module declaration applies to the rest of the source file outside later block module declarations.

The file-scoped module declaration must appear before any `using`, `export`, function, type, trait, implementation, constant, predicate, lifecycle, or other semantic declaration in the file.

Comments and documentation comments can appear before the file-scoped module declaration.

A source file without a file-scoped module declaration is rejected.

Module identity is explicit.

File paths do not define module identity.

Moving a source file does not rename its module.

---

## Block module declarations

A block module declaration contributes declarations to a named module:

```bray
module net.tests
{
    ...
}
```

In grammar terms:

```text
module-modifiers 'module' module-path block
```

A block module declaration is a package-level module contribution.

It is not nested inside the file-scoped module.

It does not inherit the file-scoped module path.

It can name any module in the current package.

This permits test and support modules to live in the same file as the declarations they exercise:

```bray
module net;

func parse_packet(pos bytes: &[u8]) -> Packet
{
    ...
}

module net.tests
{
    func parses_minimal_packet()
    {
        ...
    }
}
```

Module declarations cannot be nested.

```bray
module net
{
    module net.tests
    {
        ...
    }
}
```

This is rejected.

---

## Split modules

The same module can be declared by multiple source files and block module declarations.

Every declaration block that contributes to a module declares the same explicit module identity.

Split module declarations contribute to one logical module.

Declaration merging is deterministic.

Duplicate declarations in the same module are rejected unless the declaration form explicitly defines merging behavior.

All declarations contributed to the same module share the module's declaration namespace.

Order between source files does not affect name resolution.

Order within a single declaration block follows ordinary source-order rules where a declaration form depends on order.

---

## Module visibility

A module declaration can be public or internal.

`public` is the default.

```bray
public module api;
module api;

internal module impl;
```

Since `public` is the default, `public module api;` and `module api;` declare the same public module visibility.

Public modules are reachable through ordinary package and module path resolution.

An internal module path is available within its intended scope.

Use outside that scope requires explicit internal-use acknowledgement.

```bray
using internal impl;
```

Internal module visibility applies to the module path.

Public declarations inside an internal module still require internal-use acknowledgement to reach from outside the module's intended scope.

Split declarations of the same module must agree on module visibility.

Changing a module's visibility can be a public API change.

---

## Trusted modules

A module declaration can include `trusted`.

```bray
trusted module runtime.memory;
```

Block module declarations can also be trusted:

```bray
trusted module runtime.memory
{
    ...
}
```

The `trusted` modifier permits trusted declarations in that module.

It does not make ordinary declarations in the module trusted.

If a module has split declarations, all declarations that contribute to that module share the same trusted-module state.

Split declarations of the same module must agree on whether the module is trusted.

A trusted module declaration is rejected when the resulting logical module contains no trusted declarations.

A non-trusted module cannot contain trusted declarations.

A module declaration can combine `trusted` with module visibility.

```bray
trusted internal module runtime.memory;
```

---

## Module bodies

A module body contains declarations.

Module bodies do not evaluate at runtime.

Importing, referencing, or contributing to a module does not execute code.

A module has no runtime initialization phase.

Top-level executable statements are not module declarations.

Runtime work belongs in functions, constructors, lifecycle declarations, async tasks, tests, or other explicit executable constructs.

---

## Using declarations

A `using` declaration records that a module, package path, or declaration path is intentionally used by the current module.

```bray
using geometry.shapes;
using std.convert;
```

`using` participates in dependency checking, visibility checking, internal-use acknowledgement, diagnostics, and tooling.

`using` does not import unqualified names.

`using` does not execute code.

`using` does not initialize modules.

`using` does not create aliases.

`using` does not silently extend overload sets, implementation overload families, operators, conversions, behavioral contracts, or other polymorphic behavior.

Use of an internal module or declaration path outside its intended scope requires `using internal` or a local `internal` access acknowledgement.

```bray
using internal impl;
using internal impl.ParserState;
```

`using internal` applies to specific modules, declarations, or declaration paths.

---

## Re-exports

An export declaration re-exports a visible declaration path from the current module.

```bray
export impl.Buffer;
export impl.parse_header;
```

In grammar terms:

```text
export declaration-path ';'
```

The exported declaration is made reachable through the current module using its original final name.

The exported declaration path must resolve from the exporting module.

The exported declaration must be visible to the exporting module.

An export declaration can appear only in a module body.

The exported final name must not conflict with another declaration or export in the current module.

An export does not create a new declaration identity.

An export does not rename the declaration.

An export does not execute code, initialize a module, activate implementations, or change overload participation.

An export does not import the declaration as an unqualified name inside the exporting module.

Exporting a named implementation makes that implementation declaration reachable through the exported path.

It does not make the implementation participate in another coherence domain unless that coherence domain explicitly imports the implementation according to implementation coherence rules.

If a different public name or different public contract is needed, source code declares an explicit wrapper.

---

## Internal re-exports

Re-exporting an internal declaration requires internal-use acknowledgement.

```bray
module api;

using internal impl.Buffer;

export impl.Buffer;
```

The re-export of an internal declaration is internal.

Acknowledgement does not make an internal declaration public.

Acknowledgement does not propagate through re-exports.

A public API exposes internal declarations only through an explicit public wrapper that removes the internal declaration from the public signature.

```bray
module api;

using internal impl.Buffer;

public struct Buffer
{
    internal inner: impl.Buffer;
}
```

The public wrapper is a new public declaration with its own public contract.

---

## Finalization TODOs

- TODO: Define the test model, including how test declarations or test modules are discovered, how test entry points are formed,
  how test-only dependencies participate in the package graph, and how test execution interacts with panics, results, async work,
  trusted declarations, and internal access.

---

## Design principles

Module identity is explicit in source.

Package identity belongs to the package/build layer.

Package products select explicit source and dependency graphs.

Modules are namespaces, not runtime objects.

Imports and exports do not execute code.

Internal access is explicit and lexical.

Re-exports preserve declaration identity.
