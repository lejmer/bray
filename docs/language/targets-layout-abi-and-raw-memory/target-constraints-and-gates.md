# Target constraints and gates

Target constraints are product constraints supplied by the package and build layer.

The compiler checks a product only when the selected target profile satisfies that product's target constraints.

When a product's target constraints are not satisfied, the product is rejected before module bodies are checked.

A module contribution can be gated by the selected target profile with `@target(...)`.

`@target(...)` attaches to a source-unit module declaration or a block module declaration.

```bray
@target(target.atomic.u64)
module counters;

using std.atomic;

func add(pos counter: &std.atomic.AtomicU64, amount: u64) -> u64
{
    return std.atomic.add(counter, amount, ordering = std.atomic.Ordering.acq_rel);
}
```

A fallback module contribution can use the negated target fact:

```bray
@target(!target.atomic.u64)
module counters;

struct Counter
{
    value: u64;
}

func add(pos counter: &mut Counter, amount: u64) -> u64
{
    let old = counter.value;
    counter.value = old + amount;
    return old;
}
```

The operand of `@target(...)` is an ordinary compile-time boolean expression evaluated in target-selection context.

Target-selection context uses ordinary constant-expression syntax and semantics.

The expression can reference compiler-known target facts under `target`, literals, compiler-known target-fact enum values, and
built-in boolean, comparison, field-access, and grouping expressions that are valid in constant-evaluation context.

The expression cannot reference declarations contributed by the source graph being selected.

The expression cannot call user code.

The expression must evaluate to `bool`.

If the operand is not a valid compile-time boolean expression for the selected target profile, the product is rejected.

Only one `@target(...)` directive can apply to a module declaration.

`@target(...)` enables the module contribution when its operand evaluates to `true` for the selected target profile.

`@target(...)` does not change module identity, module visibility, trusted-module state, declaration visibility, path resolution, or runtime behavior.

## Target-conditional declarations

A **target-conditional declaration** is a compiler-known or recognized standard-library declaration whose availability depends on target facts.

The owning language rule defines each target-conditional declaration's availability rule as a compile-time boolean expression over target facts.

Before normal source checking, the compiler evaluates availability rules for the selected target profile and forms the available compiler-known and recognized standard-library surface for that product.

Using a target-unavailable declaration is a compile-time error.

A target-unavailable declaration inside a target-disabled module contribution is not used by that product.

Availability is checked during name resolution, type checking, trait and implementation checking, contract checking, layout checking, ABI checking, overload resolution, conversion selection, operator selection, const evaluation, and generic instantiation.

A generic declaration that uses a target-conditional declaration must be valid for the selected target profile wherever the generic body is checked or instantiated.

A target-gated module contribution can prove target availability for declarations inside that contribution.

If a public declaration's signature, contract, layout, ABI, constant value, implementation participation, overload participation, or availability depends on target facts, compiled interface metadata records the relevant target fact dependencies.

Compiled interface metadata for a target-dependent public surface is valid only for target profiles whose recorded target facts match for the purposes of that public surface.

## Navigation

- [Language index](../index.md)
- [Targets, layout, ABI, and raw memory index](../targets-layout-abi-and-raw-memory.md)
- Previous: [Target profiles and facts](target-profiles-and-facts.md)
- Next: [Layout contracts](layout-contracts.md)
