# Module body declarations

A module body contains declarations that belong to that module.

The module-level declaration forms are:

- constant declarations,
- static declarations,
- function declarations, including extern callable declarations,
- named callable contract declarations,
- type declarations,
- trait declarations,
- implementation declarations,
- overload declarations,
- predicate declarations.

Using declarations and export declarations can also appear in module bodies.

Using declarations and export declarations are module body items with their own module reachability rules.

Using declarations are defined in [Using declarations](../modules-and-packages/using-declarations.md).

Export declarations are defined in [Re-exports](../modules-and-packages/re-exports.md).

Block module declarations are package-level module contributions.

A block module declaration is not nested inside a source-unit module.

Module-level declarations introduce declarations into the logical module produced after split module merging.

Duplicate declarations in the same logical module are rejected unless the declaration form explicitly defines merging behavior.

Callable overload declarations and implementation overload declarations define explicit overload families instead of relying on same-name declaration merging.

## Navigation

- [Language index](../index.md)
- [Declarations index](../declarations.md)
- Previous: [Declaration contexts](declaration-contexts.md)
- Next: [Member declarations](member-declarations.md)
