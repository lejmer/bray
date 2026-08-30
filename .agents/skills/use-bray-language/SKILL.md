---
name: use-bray-language
description: >
  Bray feature and idiom guide. Use when writing or reviewing Bray code, designing Bray APIs, or choosing Bray
  features or idioms.
---

# Use Bray Language

Verify Bray's language surface before choosing syntax, APIs, or implementation patterns.

## Work from authoritative sources

Consult the [specification concepts](https://github.com/lejmer/bray/blob/develop/docs/language/concepts.md) only when terminology is unfamiliar. It defines shared terms, not features.

1. Identify the relevant domains and read only their routed references.
2. Follow specification links when a summary or example is insufficient.
3. If syntax or behavior remains uncertain, check compiler tests and representative Bray source.
4. Surface and resolve source conflicts or gaps before establishing a convention. Never fill them by analogy.

## Load the relevant specification chapters

- **Tokens, comments, literal spelling, or identifiers:** [references/lexical-grammar.md](references/lexical-grammar.md)
- **Source forms or parsing:** [references/syntax-grammar.md](references/syntax-grammar.md)
- **Modules, packages, products, imports, or availability:** [references/modules-and-packages.md](references/modules-and-packages.md)
- **Declarations, modifiers, names, visibility, or generics:** [references/declarations.md](references/declarations.md)
- **Type forms, traits, operator implementations, or language-integration implementations:** [references/types.md](references/types.md)
- **Binding, matching, destructuring, or guards:** [references/patterns.md](references/patterns.md)
- **Functions, parameters, arguments, methods, overloading, or effects:** [references/callables.md](references/callables.md)
- **Operator use, ranges, assignment, construction, control flow, iteration, or propagation:** [references/expressions.md](references/expressions.md)
- **Literal typing, adaptation, plain conversion, fallible conversion, or numeric conversion policy:** [references/conversions.md](references/conversions.md)
- **Ownership, borrowing, movement, copying, or consumption:** [references/ownership-and-borrowing.md](references/ownership-and-borrowing.md)
- **Requirements, guarantees, predicates, trust, or capabilities:** [references/contracts-and-trust.md](references/contracts-and-trust.md)
- **Construction, destruction, finalization, scoped use, or partial values:** [references/lifecycle.md](references/lifecycle.md)
- **Async functions, tasks, cancellation, synchronization, or atomics:** [references/async-and-concurrency.md](references/async-and-concurrency.md)
- **Compiler-known declarations, protected representations, or recognized operations:** [references/compiler-known.md](references/compiler-known.md)
- **Test products, test entries, test assertions, or test helpers:** [references/testing.md](references/testing.md)
- **Standard-library modules or APIs, I/O, or platform services:** [references/standard-library.md](references/standard-library.md)
- **Raw memory, target control, layout, FFI, or ABI:** [references/targets-layout-abi-and-raw-memory.md](references/targets-layout-abi-and-raw-memory.md)

## Verify the result

- Compare new Bray code with relevant documentation and repository examples, then search for selected and competing patterns before introducing an idiom.
- Run `bray fmt` to format the code, `bray check` to verify the code, and `bray test` to run tests.
