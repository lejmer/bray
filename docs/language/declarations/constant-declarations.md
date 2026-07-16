# Constant declarations

A constant declaration introduces a named compile-time value.

```bray
const max_count: usize = 1024;
const default_prefix: string = "tmp";
```

The ordinary constant declaration syntax is:

```text
const identifier ':' type-expression '=' constant-expression ';'
```

A visibility modifier can appear before `const` in declaration contexts that support visibility.

```bray
internal const page_size: usize = 4096;
```

The type annotation is required.

The initializer is checked in constant-initializer context.

The initializer must be a compile-time constant expression compatible with the declared type.

Literal adaptation uses the declared constant type as its expected type.

A module-level constant declaration introduces a declaration in the current logical module.

A block-level constant declaration introduces a constant scoped to the block expression.

A type-body constant declaration introduces a type-associated constant.

A trait constant-valued member declares a constant-valued member of the trait contract.

A trait constant-valued member without an initializer is required.

A trait constant-valued member with an initializer supplies a default value.

A trait implementation constant-valued member definition supplies the value for a required or defaulted trait constant-valued member.

Constant declarations can be referenced from constant expressions, predicate expressions, type expressions where constant arguments are accepted, and ordinary expressions.

The constant name must be unique in the ordinary lookup namespace of its declaration scope.

A constant declaration introduces an unqualified lookup name into its declaration scope.

That unqualified lookup name must not already resolve as an ordinary name from that scope.

A constant declared in a module is reached through ordinary module path resolution.

A constant declared in a type body or inherent implementation is associated with that type and can be reached through a type path according to path-expression rules.

A constant declared in a trait body or trait implementation is a constant-valued member governed by [Traits](../types/traits.md#constant-valued-members-in-traits).

A constant declaration is evaluated in compile-time constant context.

The initializer is checked as a [declaration-owned constant definition template](declaration-owned-expressions.md#constant-definition-templates-and-instances).

The value of a constant declaration is fixed for the declaration instance.

For a generic declaration, a constant declaration that depends on generic parameters is fixed for each concrete generic instantiation.

A closed non-generic constant has one instance with an empty generic substitution. A generic, trait-selected, or target-dependent
constant is evaluated separately for each exact substitution, selected implementation, and target profile required by a use.

Checking the generic definition template does not eagerly enumerate or evaluate every possible concrete constant instance.

A constant has no runtime storage identity.

Using a constant in runtime expression context materializes the constant value for that use.

The initializer is not evaluated at runtime.

A constant cannot be assigned, mutably borrowed, moved from as storage, consumed as a unique storage identity, or destroyed as a declaration.

`mut const` is not a declaration form.

The declared constant type must support constant materialization.

A value with unique runtime identity, runtime-owned resource state, finalization obligations, destructor side effects, or mutable storage identity cannot be a constant value.

## Compile-time constant expressions

A compile-time constant expression can use:

- literals,
- constants already visible in the current scope,
- const parameters visible in the current generic context,
- [target facts](../targets-layout-abi-and-raw-memory/target-profiles-and-facts.md) visible for the selected target profile,
- tuple, array, nullable, product, and union variant construction whose components are constant expressions and whose type has no runtime construction, finalization, or destructor obligation,
- unary and binary expressions whose operands are constant expressions and whose selected operation is compiler-known and valid in constant-initializer context,
- calls to const callables whose arguments are constant expressions and whose callable contract is valid in constant-initializer context,
- field access, tuple projection, and array element access over constant expressions when the selected sub-value is itself valid as a constant.

A compile-time constant expression is evaluated by the compiler using ordinary Bray expression semantics in a restricted constant-evaluation context.

Constant evaluation is not a macro system, source rewriting system, or separate compile-time language.

Normal expression typing, overload selection, result propagation, panic rules, ownership rules, borrowing rules, and evaluation order apply unless a constant-evaluation rule explicitly rejects the expression form.

A compile-time constant expression cannot read runtime storage, borrow runtime storage, assign, mutate, move from a runtime access
path, allocate storage, perform I/O, start tasks, await, suspend, catch or raise panics as runtime behavior, use runtime dynamic
dispatch, depend on address identity, or call a non-const callable.

Control-flow expressions are valid in constant-evaluation context only when their selected path can be evaluated without runtime storage, runtime effects, or runtime dispatch.

Loop expressions are not valid in constant-evaluation context.

This includes `loop`, `while`, `for`, `each`, array generator expressions, general generator iteration expressions, and boolean fold expressions.

Finite aggregate construction is still valid when every element or field initializer is itself a valid compile-time constant expression.

Only compiler-known operations and const callables explicitly defined as valid in constant-initializer context can be evaluated by a constant initializer.

For unary and binary expressions in constant-initializer context, the selected operation must be a built-in operation over built-in scalar types, `string`, `unit`, or nullable constants whose contained value is valid in constant-initializer context.

User-defined operator implementations are valid in constant-initializer context only when the selected implementation member is a const callable and all operands are valid constant expressions.

Integer-valued constant arithmetic uses contract arithmetic semantics and is exact while the constant expression is checked.

The final constant value must be representable in the declared constant type.

Floating-point constant arithmetic uses the same semantics as the selected runtime floating-point type.

Floating-point constants do not use unbounded precision.

A constant initializer that evaluates to `never`, panics, fails a contract, fails a conversion, divides by zero, overflows after conversion into the declared type, cannot prove termination, exceeds implementation resource limits, or depends on a target fact unavailable for the selected target profile is rejected.

Implementation resource limits for constant evaluation must be deterministic for a compiler invocation and must cause compile-time rejection, not runtime behavior.

Target facts can participate in constant evaluation.

A constant whose initializer reads target facts is target-dependent.

A target-dependent constant is evaluated separately for each selected target profile.

Compiled interface metadata for a target-dependent constant records its dependency on the target profile facts that affect its value.

A target-dependent constant is not evaluated once globally and reused across targets.

Constant declarations cannot be cyclic.

A constant initializer cannot reference the constant being declared, directly or through another constant initializer cycle.

## Navigation

- [Language index](../index.md)
- [Declarations index](../declarations.md)
- Previous: [Declaration bodies and requirements](declaration-bodies-and-requirements.md)
- Next: [Predicate declarations](predicate-declarations.md)
