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

## Navigation

- [Language index](../index.md)
- [Modules and packages index](../modules-and-packages.md)
- Previous: [Module bodies](module-bodies.md)
- Next: [Re-exports](re-exports.md)
