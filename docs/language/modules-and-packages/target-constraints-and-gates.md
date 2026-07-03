# Target constraints and gates

## Target constraints

Target constraints are product constraints supplied by the package and build layer.

The compiler checks a product only when the selected target profile satisfies that product's target constraints.

Target facts exposed to Bray source are ordinary compiler-known facts of the selected target profile.

When a product's target constraints are not satisfied, the product is rejected before module bodies are checked.

## Target-gated module contributions

A module contribution can be gated by the selected target profile with `@target(...)`.

`@target(...)` attaches to a file-scoped module declaration or a block module declaration.

```bray
@target(std.target.atomic.u64)
module counters;

using std.atomic;

func add(pos counter: &std.atomic.AtomicU64, amount: u64) -> u64
{
    return std.atomic.add(counter, amount, ordering = std.atomic.Ordering.acq_rel);
}
```

A fallback module contribution can use the negated target fact:

```bray
@target(!std.target.atomic.u64)
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

The expression can reference compiler-known target facts under `std.target`, literals, compiler-known target-fact enum values, and
built-in boolean, comparison, field-access, and grouping expressions that are valid in constant-evaluation context.

The expression cannot reference declarations contributed by the source graph being selected.

The expression cannot call user code.

The expression must evaluate to `bool`.

If the operand is not a valid compile-time boolean expression for the selected target profile, the product is rejected.

Only one `@target(...)` directive can apply to a module declaration.

`@target(...)` enables the module contribution when its operand evaluates to `true` for the selected target profile.

`@target(...)` does not change module identity, module visibility, trusted-module state, declaration visibility, path resolution,
or runtime behavior.

## Navigation

- [Language index](../index.md)
- [Modules and packages index](../modules-and-packages.md)
- Previous: [Test products and entries](test-products-and-entries.md)
- Next: [Module declarations](module-declarations.md)
