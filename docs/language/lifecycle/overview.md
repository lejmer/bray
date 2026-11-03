# Overview

Lifecycle behavior controls how values are constructed, used, finalized, destroyed, and entered for scoped use.
The declaration kinds define distinct operations:

| Declaration | Role |
| --- | --- |
| `construct` | Create a fully initialized value. |
| `finalize` | Complete required lifecycle obligations before ownership ends. |
| `destruct` | Perform synchronous cleanup when ownership ends. |
| `enter` | Define scope entry and scoped capability behavior for a `with` expression. |
| `exit` | Define scope exit and scoped capability behavior for a `with` expression. |

The language invokes these declarations through construction, ownership, and scoped-use operations. Source code
cannot call them directly as ordinary functions or methods. A lifecycle invocation contributes the selected
operation's effects, trusted capability requirements, and contract obligations. Its access to storage follows
ownership, borrowing, and mutation-authority rules, including outstanding finalization obligations.

These rules apply to product types, union types, implementation-eligible type forms with lifecycle behavior, and
compiler-known types whose language-defined contracts include it. They also apply when a product host or exact
native-thread attachment resolves a static owner. Each materialized static instance has one cleanup owner and one
deterministic position in its instantiated lifecycle dependency graph.

## Navigation

- [Language index](../index.md)
- [Lifecycle index](../lifecycle.md)
- Next: [Lifecycle declarations](lifecycle-declarations.md)
