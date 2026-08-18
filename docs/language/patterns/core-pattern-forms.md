# Core pattern forms

The core pattern forms are:

```bray
_                         // discard
name                      // resolved name pattern or binding
mut name                  // mutable owned binding
literal                   // literal pattern
Path.Name                 // path pattern
name(...)                 // resolved payload pattern
.name                     // explicit expected-subject no-payload pattern
.name(...)                // explicit expected-subject payload pattern
Type.Variant              // full no-payload variant pattern
Type.Variant(...)         // full payload variant pattern
Type { ... }              // product pattern with explicit type
{ ... }                   // product pattern with expected type
(pattern1, pattern2)      // tuple pattern
[pattern1, pattern2]      // fixed-size array pattern
none                      // nullable absent pattern
?pattern                  // nullable present pattern
box(pattern)              // owned-indirection pattern
pattern1 | pattern2       // alternative pattern
```

Each pattern form preserves the same core rules: structural matching, explicit binding, refutability tracking,
ownership-mode checking, and condition refinement.

## Navigation

- [Language index](../index.md)
- [Patterns index](../patterns.md)
- Previous: [Overview](overview.md)
- Next: [Pattern resolution](pattern-resolution.md)
