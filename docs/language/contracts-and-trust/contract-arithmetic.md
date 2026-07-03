# Contract arithmetic

Integer-valued contract expressions use exact mathematical integer semantics.

They do not use machine overflow semantics.

```bray
predicate range_fits(offset: usize, count: usize, length: usize) =
    offset + count <= length;
```

The expression `offset + count` means exact integer addition over the values represented by `usize`.

Runtime checks generated from contract expressions must preserve contract semantics.

They can do this through checked arithmetic or equivalent rewriting.

Contract arithmetic must not silently wrap.

The final result of a contract expression must still be valid for the predicate operation being checked.

## Navigation

- [Language index](../index.md)
- [Contracts and trust index](../contracts-and-trust.md)
- Previous: [Predicate-expression callable calls](predicate-expression-callable-calls.md)
- Next: [Fact context](fact-context.md)
