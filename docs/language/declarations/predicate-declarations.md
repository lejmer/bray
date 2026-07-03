# Predicate declarations

A predicate declaration introduces a named contract-level relation.

```bray
predicate fits(length: usize, capacity: usize) =
    length <= capacity;
```

The declaration name follows `predicate`.

Generic parameters, when present, are written after the predicate name and before the predicate parameter list.

Predicate parameters use names and type annotations.

Predicate parameters do not use callable parameter modifiers or defaults.

An ordinary predicate declaration has an `=` tail with a single predicate expression body.

A trusted predicate declaration uses the `trusted` modifier and has no ordinary body.

```bray
trusted predicate valid_raw_slice<T>(pointer: RawPointer<T>, length: usize);
```

A trusted predicate declaration is an opaque trusted relation.

Trusted predicate calls establish trusted obligations according to the contract and trust rules.

Predicate bodies are checked in predicate-expression context.

Predicate-expression rules are defined in [Predicate expressions](../expressions/predicate-expressions.md).

Predicate declarations can be module-level declarations.

Trait predicate members declare predicate requirements or default predicate behavior for a trait contract.

Trait implementation predicate members define predicate fulfillments for the implemented trait application.

## Navigation

- [Language index](../index.md)
- [Declarations index](../declarations.md)
- Previous: [Constant declarations](constant-declarations.md)
- Next: [Overload declarations](overload-declarations.md)
