# Block-level declarations

Block expressions can contain block-level declarations.

The block-level declaration forms are:

- local binding declarations,
- constant declarations.

Local binding declarations introduce local bindings.

Local binding declaration rules are defined in [Local binding declarations inside block expressions](../expressions/local-binding-declarations-inside-block-expressions.md).

A block-level constant declaration introduces a named compile-time value scoped to the block region.

Block-level declarations are checked as block items.

They are not module-level declarations and do not add declarations to the containing module.

They are visible only according to ordinary block scope rules.

Named function declarations, type declarations, trait declarations, implementation declarations, module declarations, and package declarations are not block-level declarations.

## Navigation

- [Language index](../index.md)
- [Declarations index](../declarations.md)
- Previous: [Member declarations](member-declarations.md)
- Next: [Declaration names and identity](declaration-names-and-identity.md)
