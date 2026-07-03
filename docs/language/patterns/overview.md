# Overview

A **pattern** is a structural matching form that can refine a value, bind parts of it, and determine how ownership or access paths flow into the matched region.

Patterns are a dedicated grammar category. They use their own binding and checking rules.

Patterns are used by language constructs that decompose values, refine control flow, or introduce bindings from structured data. Pattern contexts include local destructuring, iteration patterns, union variant matching, and match expressions.

A pattern is checked against a subject type. The subject type determines which pattern forms are valid and what bindings, refinements, ownership state, and fact-context changes the pattern can produce.

## Navigation

- [Language index](../index.md)
- [Patterns index](../patterns.md)
- Next: [Core pattern forms](core-pattern-forms.md)
