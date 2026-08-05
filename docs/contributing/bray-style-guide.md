# Bray source style guide

This guide defines the canonical style for Bray source code. It applies to the standard library, examples, tests, generated
Bray source, and other Bray code maintained in this repository. It is also the default style produced by `brayfmt`.

Style does not change program meaning. The language specification determines whether source is valid, while this guide
determines how valid source should normally be written. A project can configure formatter rules where variation is useful, but
repository source follows the defaults in this guide.

## Principles

- Prefer source that is easy to scan and review over compact source.
- Organize code around domain concepts rather than implementation status or syntax categories.
- Let syntax communicate visibility, compiler recognition, and other declaration roles instead of encoding those roles in
  names.
- Use consistent layout instead of manual visual alignment.
- Keep related declarations and statements together, and use blank lines to separate changes in purpose.
- Run `bray fmt` instead of maintaining mechanical layout by hand.

## Source files and modules

### Small modules

A small module should initially live in a single `<name>.bray` source file.

Source files use `snake_case` names. A file should normally be named after the central concept it owns.

Prefer one logical module contribution per source file. Use a source module declaration for that contribution unless the file
genuinely needs to contain top-level braced module declarations.

### Splitting modules

Split a module when its source file contains distinct cohesive responsibilities. Do not wait for the size limit when a natural
conceptual split already exists.

A production source file must not exceed 800 lines unless it has a justified exemption. Test modules do not count toward this
limit, although tests should still be split when that makes them easier to navigate.

Once split, the module uses a thin `<name>.bray` root and additional source files under a same-named `<name>/` directory.

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

Do not create semantic modules solely to reduce file size. Modules define namespaces, visibility, and other semantic
boundaries. Files define physical organization.

### Thin module roots

The thin module root contains:

- the canonical Braydoc comment for the module,
- the module declaration,
- directives and metadata that genuinely apply to the whole module.

It does not contain ordinary declarations or implementations and must not become an API header. Files contributing to the
module document the declarations they own without repeating the module-level Braydoc comment.

### Concept ownership

Organize files by cohesive domain concept, not by syntax category or implementation status. A concept file should own its
public surface, implementations, internal support declarations, and closely related constants together.

Do not separate declarations from their implementations through pairs such as `<name>.bray` and `<name>_impl.bray`.

Avoid structural or vague filenames such as `_impl`, `_types`, `_functions`, `common`, `helpers`, `misc`, and `util`. Such
names are appropriate only when the word itself names a genuine domain concept.

### Source-unit independence

Each source unit carries the `using` declarations needed to understand it. Do not use the thin root as a shared import file
unless the language semantics explicitly make an import module-wide.

Do not use numbered filenames or rely on source-file ordering. Split module contributions must remain understandable and
correct independently of filesystem enumeration or compilation order.

## Naming

Use `PascalCase` for:

- structs and unions,
- traits,
- union variants,
- generic type parameters.

Use `snake_case` for:

- modules and module path components,
- functions and callable contracts,
- named constructors,
- predicates and overload families,
- fields and parameters,
- local bindings and pattern bindings,
- named implementations.

Use `SCREAMING_SNAKE_CASE` for constant declarations, constant-valued members, and generic constant parameters. Conventional
single-letter mathematical names such as `N` already follow this form.

Treat an initialism as a word inside a name. Prefer `IoError` and `Utf8Text` over `IOError` and `UTF8Text`.

Name declarations for the operation or concept they represent. Do not add prefixes such as `support_`, `helper_`, `internal_`,
`external_`, or `compiler_` merely to describe implementation role, visibility, or compiler-known status. The declaration
syntax and catalog metadata already communicate those properties.

Use short generic names such as `T` only when their meaning is obvious throughout the declaration. Prefer a descriptive name
such as `Sink`, `Element`, or `Error` when several generic roles appear together.

## File layout

Write the source module declaration before ordinary module items. Place module directives immediately before the module
declaration they modify.

Place `using` and `export` declarations after the source module declaration and before ordinary declarations. Keep consecutive
declarations of the same kind together without blank lines. Use one blank line between the module declaration, the import or
export group, and the first ordinary declaration.

Do not impose a universal alphabetical or declaration-kind order on ordinary declarations. Keep a public operation near its
supporting types and implementations when that makes the concept easier to follow.

## Indentation and line width

Use four spaces for each indentation level. Do not use tabs for indentation.

Indent continuation lines by one additional level. Do not align unrelated lines by inserting variable amounts of whitespace.

The default maximum line width is 120 display columns. This is a layout target rather than permission to alter source tokens.
An indivisible token, preserved comment, or other text without a legal breakpoint may exceed it.

Do not leave trailing whitespace.

## Braces

Place the opening brace of a module, declaration body, control-flow body, match case, or block expression on the next line at
the same indentation as its header.

```bray
func classify(value: i32) -> Result<Category, Error>
{
    if value < 0
    {
        return Error(invalid_value(value));
    }

    return Ok(category_for(value));
}
```

Place a closing brace on its own line at the indentation of the construct it closes. Write empty bodies with the opening and
closing braces on separate lines.

Write continuation keywords such as `else` on the line after the preceding closing brace.

```bray
if ready
{
    run();
}
else
{
    wait();
}
```

Do not write single-line braced bodies.

## Blank lines

Use one blank line between top-level declarations.

Do not put a blank line:

- immediately after an opening brace,
- immediately before a closing brace,
- between a Braydoc comment, directives, and the declaration they document,
- between adjacent match cases,
- between homogeneous field, variant, or overload-arm entries.

Inside a declaration body, use one blank line between member declarations with bodies or between groups that serve different
purposes. Keep a homogeneous run of fields or variants together.

Inside an executable block, use blank lines as semantic paragraph boundaries. Keep setup together, separate validation from
the work it guards, and separate a final returned expression from the statements that prepare it. Do not put a blank line
between near-identical statements that form one conceptual group.

Place one blank line before a standalone comment inside a block when a statement, expression, or declaration precedes it. Do
not require that blank line when the comment is the block's first content. Keep the comment attached to the statement or
expression it documents without a blank line between them.

Use at most one consecutive blank line.

## Directives and declaration headers

Write each directive on its own line immediately before the declaration it modifies. Keep consecutive directives together.

```bray
/// Runs one isolated test case.
@test
@target(testing_enabled())
func isolated_case()
{
}
```

Use the canonical modifier order defined for a declaration form. Module modifiers use `trusted` before visibility. Where the
language does not define a canonical order, keep modifiers in a stable order within the surrounding API rather than changing
the order declaration by declaration.

Keep a callable header on one line when it fits. When its parameter list must wrap, place one parameter on each line, include a
trailing comma, and put the closing parenthesis on its own line.

```bray
func transfer<Source, Destination>(
    pos source: &mut Source,
    pos destination: &mut Destination,
    count: usize,
) -> Result<usize, TransferError>
    with(Source: Reader)
    with(Destination: Writer)
    requires(blocking_execution())
{
}
```

Write each `with(...)`, `requires(...)`, and `ensures(...)` clause on its own continuation line when it follows a callable or
type header.

Keep a match arm body on the same line as its case when the body is empty or contains exactly one expression and the complete
arm fits within the configured line width. Put one space inside nonempty inline braces and no space inside empty braces. This is
the only exception to the block-brace placement rule.

```bray
case 0 { yield 48; }
case None {}
```

Use ordinary multiline block layout when an arm contains declarations, multiple expressions, or does not fit on one line.

## Lists and delimiters

Keep a parenthesized or bracketed list inline when it fits comfortably within the line-width target.

When a list wraps:

- place one entry on each line,
- indent entries by one level,
- include a trailing comma where the grammar permits one,
- place the closing delimiter on its own line.

```bray
let result: Result<usize, IoError> = transfer(
    source = source,
    destination = destination,
    count = requested,
);
```

Use a trailing comma in multiline comma-separated lists. A trailing comma in a short inline list is acceptable when it
expresses a single-element tuple or another grammatical distinction.

Write struct construction fields and overload arms one per line with trailing commas.

```bray
return Options
{
    radix = Radix.Decimal,
    precision = 0,
    width = 0,
};
```

## Spaces and punctuation

Use one space:

- on both sides of binary operators and `=`,
- after a comma,
- after a colon in a type annotation or named entry,
- on both sides of `->`,
- between a keyword and the expression or declaration that follows it.

Do not use spaces:

- before commas, colons, or semicolons,
- immediately inside parentheses or brackets,
- around member access `.`,
- around range operators such as `..`,
- between a prefix operator and its operand,
- immediately inside generic delimiters.

```bray
let borrowed: &mut Value = &mut values[index];
let remaining: &[u8] = &source[start..end];
let result: Result<usize, IoError> = left + right;
```

Write generic lists as `Map<Key, Value>` and generic constant declarations as `Buffer<Element, const N: usize>`.

## Statements and expressions

Write one sequenced expression or local declaration per line.

Use semicolons where the grammar requires a sequenced expression or declaration terminator. Do not add a semicolon after a
block-shaped expression used directly as a block item.

Prefer early returns when they keep the successful path clear. Avoid unnecessary nested conditionals when combining conditions
or using `else if` preserves evaluation order, scope, lifecycle behavior, and comments.

Write `match` cases without blank lines between them.

```bray
match consume result
{
    case Ok(value)
    {
        return value;
    }
    case Error(error)
    {
        return fallback(error);
    }
}
```

Use an unqualified union variant when the expected type makes the union unambiguous, such as `return Ok(value);`. Use an
explicit path such as `Result.Ok(value)` when context does not determine the union or the qualification materially improves
clarity. The leading-dot form remains available where its expected-type shorthand is useful, but it is not the default spelling
when an unqualified variant is clear.

## Comments and Braydoc

Use comments to explain intent, constraints, invariants, and decisions that the code does not make clear. Do not narrate the
code mechanically.

Put one space after `//` in an ordinary line comment. Keep a trailing line comment on the same line only when it is short and
directly documents that line. Otherwise, place it on the preceding line at the indentation of the code it documents.

Do not manually wrap comment text merely to make the formatter alter it. Comment spelling is preserved exactly, and a comment
without a safe breakpoint may exceed the line-width target.

Place a declaration's Braydoc comment immediately before its directives and declaration. The canonical module Braydoc comment
belongs in the thin module root.

Document public APIs in terms of purpose, observable behavior, caller obligations, and failure conditions. Do not expose
internal storage or implementation mechanics unless they materially affect callers.

## Construction APIs

Use a primary constructor when an operation exists to produce a value of one concrete type. Use a named constructor when the
type has an alternate construction path that benefits from a descriptive name. Do not wrap a constructor in a global factory
function solely to avoid constructor syntax.

Express fallibility through the constructor's result type. Do not add a `try_` prefix merely because construction can fail.

Keep an operation as a function when its primary meaning is acquisition, conversion, or another domain action rather than the
construction of its returned representation. Process-global stream access and text decoding are examples of operations that
should remain functions.

Use named arguments when they distinguish several similar values or make a call's policy clear. Positional arguments remain
appropriate when their meaning is obvious from the operation and surrounding expression.

## Formatter boundary

`brayfmt` owns mechanical source layout. It does not rename declarations, reorganize modules, reorder conceptually significant
declarations, or make semantic API decisions.

An explicitly enabled syntax rewrite may change syntax only when it can prove that program behavior, scope, lifecycle behavior,
control flow, and comment ownership remain unchanged. If the proof is unavailable, the formatter leaves the source unchanged.

Malformed source and parser recovery regions remain byte-exact rather than being partially reformatted.

The formatter preserves the first LF or CRLF line-ending style found in an existing source file. New source should use LF unless
a project requires CRLF. Every nonempty source file ends with one line ending.
