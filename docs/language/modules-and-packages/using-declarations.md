# Using declarations

A `using` declaration records that a module, package path, or declaration path is intentionally used by the current
module.

```bray
using geometry.shapes;
using std.convert;
```

`using` participates in dependency checking, visibility checking, internal-use acknowledgement, and tooling.

`using` does not introduce unqualified names.

`using` does not execute code.

`using` does not initialize modules.

`using` does not create aliases.

`using` does not silently extend overload sets, implementation overload families, operators, conversions, behavioral
contracts, or other polymorphic behavior.

Use of an internal module or declaration path outside its intended scope requires `using internal` or a local `internal`
access acknowledgement.

```bray
using internal impl;
using internal impl.ParserState;
```

`using internal` applies to specific modules, declarations, or declaration paths.

## Dependency coverage

Every named reference into another logical module requires a matching `using`. This includes expression and type
paths, declaration signatures, contracts, defaults, exports, and implementation references. Source modules in the
current package and modules supplied by compiled dependencies follow the same rule. A fully qualified spelling
identifies the target but does not establish the dependency.

A `using` path covers that path and paths extending it by complete `.`-separated components. A package path covers
its modules and their declarations. A module path covers that module and its descendants. A declaration path covers
that declaration and its members. It does not cover siblings or a name sharing only part of a component.

For example, `using geometry.shapes.Circle;` covers `geometry.shapes.Circle` and its member paths. It does not cover
`geometry.shapes.Square`, `geometry.shapes.CircleCache`, or another spelling of the same declaration through a re-export.

Coverage follows the source namespace path at the reference. A consumer of `facade.Item` declares a dependency on
`facade` or the narrower re-exported path. It does not thereby gain permission to name the provider's other modules.
An imported module's own dependencies remain its dependencies. Checked definitions retain their definition-site
dependencies when used by a consumer.

Selecting members of an obtained value does not itself spell a provider module path. It does not require inventing
a dependency on a module hidden behind an API. Explicit trait implementation participation retains its own rules.

A `using` declaration binds its own target without requiring an earlier dependency declaration. Its target must
still exist, have the appropriate accessibility, and satisfy the ordinary name-collision rules.

## Logical module scope

A module's dependency declarations apply across all enabled contributions to that same logical module, including
split source units and repeated block contributions. Contribution order does not affect availability. Repeating the
same dependency in another contribution is permitted.

A different block module has its own dependency declarations. It does not inherit declarations from a preceding
source-unit contribution, another block, or a dotted parent module.

References within the current logical module need no self-import, including qualified references to its own
declarations. The current package itself contributes no additional ordinary declaration scope.

## Accessibility and ambient names

Dependency availability and internal acknowledgement are separate obligations. `using internal` declares the
dependency and acknowledges the specified internal path. Ordinary `using` declares the dependency without that
acknowledgement. Local `internal` can acknowledge access at a reference after its dependency has been declared.
Neither local `internal` nor `trusted` supplies a missing dependency declaration. Package-wide internal
acknowledgement does not grant access to internal declarations.

Compiler-known names provided directly in the ambient scope remain available without `using`, including the
specified scalar, type-form, predicate and capability names. Compiler-known `target` properties are ambient,
including in contribution gates. Named modules such as `core.memory` require `using`. Standard-library and runtime
source follow the same dependency rules as other source.

## Navigation

- [Language index](../index.md)
- [Modules and packages index](../modules-and-packages.md)
- Previous: [Module bodies](module-bodies.md)
- Next: [Re-exports](re-exports.md)
