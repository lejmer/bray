# Bray Style Guide

This is a working document for the conventions Bray code should follow. It is not yet the final style guide.

## Modules and source files

### Small modules

A small module should initially live in a single `<name>.bray` source file.

Source files should use `snake_case` names. A file should normally be named after the central concept it owns.

### Splitting modules

A module should be split when its source file contains distinct cohesive responsibilities. Do not wait for the size limit when a
natural conceptual split already exists.

A production source file must not exceed 800 lines unless it has a justified exemption. Test modules do not count toward this
limit, although tests should still be split when that makes them easier to navigate.

Once split, the module uses a thin `<name>.bray` root and additional source files under a same-named `<name>/` directory.

For example:

```text
src/
|- bytes.bray
|- bytes/
|  |- buffer.bray
|  |- comparison.bray
|  `- conversion.bray
|- io.bray
`- io/
   |- buffer.bray
   |- reader.bray
   |- writer.bray
   |- print.bray
   `- platform.bray
```

The directory structure organizes source contributions. It does not create nested logical modules. Every source file under
`bytes/` in this example explicitly contributes to the same module as `bytes.bray`.

Do not create semantic modules solely to reduce file size. Modules define namespaces, visibility, and other semantic boundaries.
Files define physical organization.

### Thin module roots

The thin module root contains:

- the canonical braydoc comment for the module,
- the module declaration,
- metadata that genuinely applies to the whole module.

It does not contain ordinary declarations or implementations and must not become an API header. Files contributing to the module
document the declarations they own without repeating the module-level braydoc comment.

### Concept ownership

Organize files by cohesive domain concept, not by syntax category or implementation status. A concept file should own its public
surface, implementations, internal support declarations, and closely related constants together.

Do not separate declarations from their implementations through pairs such as `<name>.bray` and `<name>_impl.bray`.

Avoid structural or vague filenames such as `_impl`, `_types`, `_functions`, `common`, `helpers`, `misc`, and `util`. Such names are
appropriate only when the word itself names a genuine domain concept.

### Source-unit independence

Each source unit should carry the `using` declarations needed to understand it. Do not use the thin root as a shared import file
unless the language semantics explicitly make an import module-wide.

Do not use numbered filenames or rely on source-file ordering. Split module contributions must remain understandable and correct
independently of filesystem enumeration or compilation order.

## Naming

Name declarations for the operation or concept they represent. Do not add prefixes such as `support_`, `helper_`, `internal_`,
`external_`, or `compiler_` merely to describe a declaration's implementation role, visibility, or compiler-known status.

Internal, external, and compiler-known declarations follow the same naming rules as ordinary declarations. Their syntax and
catalog metadata already communicate those properties. A prefix is appropriate only when the prefixed word is part of the
domain concept itself.
