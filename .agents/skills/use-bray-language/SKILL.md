---
name: use-bray-language
description: >
  Bray language feature and idiom guide. Trigger: writing or reviewing Bray code, designing a Bray API, or exploring
  which Bray feature or idiom to use in a given situation.
---

# Use Bray Language

Use Bray's own language model deliberately. Verify the relevant language surface before choosing syntax, an API shape, or an implementation pattern.

## Work from authoritative sources

Use the [specification concepts](https://github.com/lejmer/bray/blob/develop/docs/language/concepts.md) as the vocabulary key for terms used throughout the language specification. It defines shared terms rather than a feature surface, so consult it when terminology is unfamiliar.

1. Identify the language domains involved in the task.
2. Use the routing table below and read only the reference files needed for those domains.
3. When the compact summary or example does not settle your question, use the links to the Bray language specification to learn more.
4. Check compiler tests and representative Bray source before relying on memory when syntax or behavior remains uncertain.
5. Derive Bray behavior from the specification instead of filling gaps by analogy.
6. If the sources disagree or leave a gap, surface the uncertainty and resolve it before encoding a new convention.

Do not read every reference by default. Read another reference only when the task crosses into that domain.

## Load the relevant specification chapters

- **Tokens, comments, literals, or identifiers:** [references/lexical-grammar.md](references/lexical-grammar.md)
- **Source forms or parsing:** [references/syntax-grammar.md](references/syntax-grammar.md)
- **Modules, packages, products, imports, or availability:** [references/modules-and-packages.md](references/modules-and-packages.md)
- **Declarations, modifiers, names, visibility, or generics:** [references/declarations.md](references/declarations.md)
- **Type forms, traits, implementations, or conversions:** [references/types.md](references/types.md)
- **Binding, matching, destructuring, or guards:** [references/patterns.md](references/patterns.md)
- **Functions, parameters, arguments, methods, overloading, or effects:** [references/callables.md](references/callables.md)
- **Operators, assignment, construction, control flow, iteration, or propagation:** [references/expressions.md](references/expressions.md)
- **Ownership, borrowing, movement, copying, or consumption:** [references/ownership-and-borrowing.md](references/ownership-and-borrowing.md)
- **Requirements, guarantees, predicates, trust, or capabilities:** [references/contracts-and-trust.md](references/contracts-and-trust.md)
- **Construction, destruction, finalization, or partial values:** [references/lifecycle.md](references/lifecycle.md)
- **Async functions, tasks, cancellation, synchronization, or atomics:** [references/async-and-concurrency.md](references/async-and-concurrency.md)
- **Compiler-known declarations, protected representations, or recognized operations:** [references/compiler-known.md](references/compiler-known.md)
- **Standard-library modules, APIs, I/O, or platform services:** [references/standard-library.md](references/standard-library.md)
- **Raw memory, target control, layout, FFI, or ABI:** [references/targets-layout-abi-and-raw-memory.md](references/targets-layout-abi-and-raw-memory.md)

## Choose a Bray-native approach

Before writing or approving code:

1. List the Bray features that could express the requirement.
2. Prefer the most direct Bray construct and established repository idiom.
3. Check whether the proposed pattern bypasses a more direct Bray feature.
4. Preserve Bray's visibility, ownership, lifecycle, contract, effect, and expression semantics instead of approximating them with superficially similar syntax.
5. Use explicit low-level forms only when the task actually needs their semantics.

## Verify the result

- Compare new Bray code with the relevant language documentation and representative repository examples.
- Search for existing uses of the selected feature and for competing patterns before introducing a new idiom.
- Run `bray fmt` to format the code, `bray check` to verify the code, and `bray test` to run tests.
