# Bray for Rust developers, part 2: Model data and control flow

In [part 1](01-build-and-organize.md), we built a pantry inventory and learned enough Bray to make the donkey count oats.
Now we'll give it a little more responsibility and see how it handles missing oats, invalid quantities, and a pantry in
a state of panic.

We'll build a small executable that classifies stock, looks up a quantity, handles a recoverable error, and catches a
panic. Along the way, we'll compare Bray's unions and patterns with Rust's enums and `match`, and look at what `try`,
`?`, and `catch` actually do. The symbols are familiar. Their job descriptions have been rearranged.

This part assumes you know your way around Rust, including enums, pattern matching, `Option`, and `Result`. It also
assumes the Bray basics from part 1, such as named arguments, borrows, and `bray run`. You don't need the project from
part 1. We're starting a fresh one, so the oats can enjoy a second chance.

You need an installed Bray toolchain with `bray` on your path and support for your native target. Create an executable
project and enter its directory:

```sh
bray init pantry_flow --package pantry_flow
cd pantry_flow
```

We'll keep the code in `src/main.bray`. Add new declarations outside `main` and add new statements inside `main`
before its closing brace.

## Give stock a state

In Rust, we could use an enum to represent the states of an inventory item:

```rust
enum Stock {
    Empty,
    Low(i32),
    Ready(i32),
}
```

A Bray `union` serves the same purpose here. It has a closed set of variants, and a value has one active variant. This
is a tagged sum type, not the overlapping-storage `union` you may know from Rust.

Replace `src/main.bray` with:

```bray
module app;

using std.io;

union Stock
{
    Empty;
    Low(pos quantity: i32);
    Ready(pos quantity: i32);
}

func describe(pos status: &Stock) -> string
{
    return match status
    {
        case Stock.Empty { yield "Empty"; }
        case Stock.Low(_) { yield "Low stock"; }
        case Stock.Ready(_) { yield "Stock is ready"; }
    };
}

func main()
{
    let status: Stock = Stock.Low(2);

    std.io.print_line(describe(&status));
}
```

Run it:

```sh
bray run
```

The application prints:

```text
Low stock
```

`Empty` has no payload. `Low` and `Ready` each carry a quantity. As with function parameters in part 1, `pos` permits
the quantity to be supplied positionally. That's why `Stock.Low(2)` works. Without `pos`, we'd use the payload name.

### Blocks and `yield`

In Rust, a block can produce its final expression as a value:

```rust
let threshold = {
    let base = 2;
    base + 1
};
```

Bray's block expressions can produce values too, but a value-producing block uses `yield` instead of an implicit final
expression. For example:

```bray
let threshold: i32 =
{
    let base: i32 = 2;

    yield base + 1;
};
```

The block supplies `3` to `threshold`. A block in a `unit` context can finish without a `yield` and produces `unit`.
When a block must produce a non-`unit` value, every normal completion path must yield a value. Otherwise, Bray rejects
it. `yield` supplies the nearest enclosing yield-capable region. It doesn't return from the function. Bray also uses
`yield` in generators, which we'll leave for another time.

### Match every variant

Rust's `match` can select a branch and produce a value. Bray's `match` does that too, but each arm starts with `case`.
An arm body is a block expression, so its `yield` supplies that arm's value. In `describe`, the selected arm supplies
a string to the `match`, and `return` then exits the function with that string. `yield` does not exit `describe`.

The match covers `Empty`, `Low`, and `Ready`. Remove a case and Bray rejects the match as non-exhaustive, just as Rust
would reject a match that misses an enum variant. `_` in `Low(_)` and `Ready(_)` ignores the quantity. The subject is a
shared borrow, so `describe` can inspect the stock without taking ownership.

## Test a pattern without a match block

Rust has `matches!` for asking whether a value has a particular shape. Bray has no macros, so it provides a `matches`
expression instead. No exclamation mark to make the question sound more urgent than it is.

Add this to `main`, after the call to `describe`:

```bray
if status matches Low(_)
{
    std.io.print_line("Time to reorder");
}
```

Run `bray run` again. The application now prints:

```text
Low stock
Time to reorder
```

`matches` observes `status` and returns a `bool`. It evaluates the subject once and doesn't consume it. The pattern
cannot introduce bindings, so `Low(_)` is valid but `Low(quantity)` isn't. If we want the quantity, we need a binding
rather than a yes-or-no answer.

## Bind a matching value

Rust's `if let` lets us use a payload inside the successful branch. Bray's conditional `let` does the same job with
Bray's pattern spelling.

Add this after the `matches` condition:

```bray
if let Low(remaining) = status && remaining == 2
{
    std.io.print_line("Two left");
}
```

Run `bray run`. The output gains one line:

```text
Two left
```

The `remaining` binding is available to the later `&&` condition and inside the body. The condition short-circuits, so
Bray checks `remaining == 2` only after `Low` matches. The earlier `matches` expression didn't consume `status`, which is
why we can inspect it again.

## Propagate an absent value

In Rust, a function returning `Option<T>` can use `?` to return `None` early. Bray has the same sort of shortcut for a
nullable value, although it writes the type as `T?` and absence as `none`.

Add these functions to `src/main.bray`:

```bray
func find_quantity(name: string) -> i32?
{
    if name == "oats"
    {
        return 2;
    }

    return none;
}

func reorder_quantity(name: string) -> i32?
{
    let quantity: i32 = find_quantity(name = name)?;

    if quantity < 3
    {
        return 3 - quantity;
    }

    return 0;
}
```

Add these statements to `main`:

```bray
if let ?needed = reorder_quantity(name = "oats") && needed == 1
{
    std.io.print_line("Order one");
}

if reorder_quantity(name = "rice").is_absent()
{
    std.io.print_line("Rice not found");
}
```

Run `bray run`. The new lines are:

```text
Order one
Rice not found
```

For oats, `find_quantity` returns `2`, and `reorder_quantity` returns the missing amount. For rice, `find_quantity`
returns `none`. The postfix `?` propagates that absence from `reorder_quantity` before subtraction happens. The second
condition only needs to know whether a value is absent, so it uses `is_absent()` without binding a payload.

As in part 1, `if let ?needed` binds the present value. It isn't a propagation expression. The same question mark has
found another job, this time on the pattern side of the `=`.

## Return a recoverable error

In Rust, `Result<T, E>` distinguishes a successful value from an error. Bray also has `Result<T, E>`, with `Ok` and
`Error` variants. Unlike Rust's `Err`, the latter is not trying to save two characters.

We'll reject negative quantities instead of pretending that minus one oats is a useful inventory state. Add these
declarations to `src/main.bray`:

```bray
union StockError
{
    NegativeQuantity;
}

func classify(quantity: i32) -> Result<Stock, StockError>
{
    if quantity < 0
    {
        return Error(NegativeQuantity);
    }

    if quantity == 0
    {
        return Ok(Stock.Empty);
    }

    if quantity < 3
    {
        return Ok(Stock.Low(quantity));
    }

    return Ok(Stock.Ready(quantity));
}

func load_status(quantity: i32) -> Result<Stock, StockError>
{
    let status: Stock = try classify(quantity = quantity);

    return Ok(status);
}
```

Add this to `main`:

```bray
let invalid: Result<Stock, StockError> = load_status(quantity = -1);

if invalid matches Error(_)
{
    std.io.print_line("Quantity cannot be negative");
}
```

Run `bray run`. The application adds:

```text
Quantity cannot be negative
```

`classify` returns a normal `Result` value. `load_status` uses prefix `try` to unwrap `Ok` or propagate `Error`. For the
negative quantity, the error travels back to `main`, where `matches Error(_)` checks its shape. This is normal
recoverable control flow, not a panic.

For this example, `try classify(...)` plays the role of Rust's `classify(...)?`. Bray's postfix `?` is reserved for
nullable absence instead. Error conversion also differs: Bray propagates errors through the compatible built-in
conversions described in the [result propagation rules](../../language/expressions/result-and-run-result-propagation-expressions.md),
not Rust's general `From` conversion. Here, both functions use the same `StockError`, so no conversion is needed.

## Move a payload out of a union

In Rust, matching an owned enum can move a non-`Copy` payload out of it. In Bray, an ordinary `match` observes its
subject. To move a non-copyable payload out, we use `match consume` and give up the original union value.

Add these declarations to `src/main.bray`:

```bray
struct Item
{
    name: string;
}

union Delivery
{
    Arrived(pos item: Item);
    Missing;
}

func take_item(pos delivery: Delivery) -> Item?
{
    return match consume delivery
    {
        case Arrived(item) { yield item; }
        case Missing { yield none; }
    };
}
```

Add this to `main`:

```bray
let item: Item =
{
    name = "oats",
};
let delivery: Delivery = Delivery.Arrived(item);

if let ?delivered = take_item(delivery)
{
    std.io.print_line(delivered.name);
}
```

Run `bray run`. The application adds:

```text
oats
```

`Item` has no copy contract, so the `Arrived` arm must move its payload to return it. `match consume delivery` permits
that transfer. The `Missing` arm yields `none`. An ordinary observing match would not allow the non-copyable item to be
moved out. The call to `take_item` also transfers ownership of `delivery`, so we don't use that value afterward.

The earlier `matches`, conditional `let`, and ordinary `match` examples observe their subjects. They are useful when
we only need to inspect a value. `match consume` is for the occasion when we actually want to take the oats home.

## Catch a panic

Rust distinguishes a `Result::Err` from a panic. Bray makes the same distinction. A `Result.Error` is a value that a
caller can inspect or propagate with `try`. A panic leaves normal control flow until something catches it or the run
ends. Bray's `catch` expression catches a panic from its operand and returns a `Result` containing either the value or a
`PanicReport`.

First, let's prove that `catch` doesn't turn an ordinary error into a panic. Add this to `main`:

```bray
let caught_error: Result<Result<Stock, StockError>, PanicReport> = catch load_status(quantity = -1);

if caught_error matches Ok(Error(_))
{
    std.io.print_line("The error remains a value");
}
```

Run `bray run`. The application adds:

```text
The error remains a value
```

The call completed normally by returning `Error(NegativeQuantity)`. That error is still inside the outer `Ok`. No panic
crossed the `catch` boundary, so `catch` had nothing to catch. This is nested `Result` handling, not an invitation to
invent three more layers for sport.

Now add a function to `src/main.bray` that panics:

```bray
func count_missing_oats()
{
    panic("The oats have escaped");
}
```

Add this to `main`:

```bray
let caught: Result<unit, PanicReport> = catch count_missing_oats();

if caught matches Error(_)
{
    std.io.print_line("Recovered from a panic");
}
```

Run `bray run`. The application adds:

```text
Recovered from a panic
```

The function panics while `catch` evaluates its operand. `catch` converts that panic to `Error(PanicReport)` rather than
resuming the panicked function. This resembles the role of Rust's `std::panic::catch_unwind`, though Bray's `catch` is a
language expression and returns Bray's `PanicReport`. It does not turn every `Error` into a panic or catch task
cancellation. We'll look at task outcomes in the async part.

## Check the finished program

Run the finished application once more:

```sh
bray run
```

Its output, apart from build progress, is:

```text
Low stock
Time to reorder
Two left
Order one
Rice not found
Quantity cannot be negative
oats
The error remains a value
Recovered from a panic
```

The pantry now has distinct ways to represent a stock state, absence, a recoverable error, and a panic. For an optional
exercise, change `load_status(quantity = -1)` in the `invalid` binding to `load_status(quantity = 0)`. Run the program
again. The `Quantity cannot be negative` line disappears because the result is now `Ok(Stock.Empty)`. Leave the separate
`caught_error` call unchanged, so its line still appears. Restore `-1` afterward.

## Next up

Part 3 will take a closer look at values, storage, ownership, and copying. Until then, the language reference has the
full rules for [unions](../../language/types/union-types.md), [patterns](../../language/patterns.md),
[nullable propagation](../../language/expressions/nullable-and-absence-expressions.md),
[result propagation](../../language/expressions/result-and-run-result-propagation-expressions.md), and
[catch expressions](../../language/expressions/catch-expressions.md). They contain fewer oats and considerably more
precision.
