# Bray for Rust developers, part 4: Write generic APIs with traits

The donkey has a delivery to carry, but we haven't told anyone what's in it. We'll build a small labeling API that works with inventory items and delivery notices, then give an item both a short label and a detailed packing slip. The same delivery can need several descriptions (none of which makes it any lighter), so we'll also need a way to say which description we want.

This part assumes you know Rust traits and generics, and the Bray functions, control flow, and borrowing covered in [part 1](01-build-and-organize.md), [part 2](02-data-and-control-flow.md), and [part 3](03-values-and-storage.md). Start a fresh project with your installed Bray toolchain:

```sh
bray init pantry_labels --package pantry_labels
cd pantry_labels
```

## Print a label through a generic function

In Rust, we can require a trait and write a function that accepts any type implementing it:

```rust
trait Label {
    fn text(&self) -> &str;
}

fn print_label<T: Label>(value: &T) {
    println!("{}", value.text());
}
```

Bray uses `trait` too, and writes the generic constraint in a `with(...)` clause. We'll also give our trait a code so callers can use more than its text. Replace `src/main.bray` with:

```bray
module app;

using std.io;

trait Label
{
    type Code;

    func text() -> string;
    func code() -> Code;
}

struct Item
{
    name: string;
    quantity: i32;
    number: i32;
}

impl Item(Label)
{
    type Code = i32;

    func text() -> string
    {
        return self.name;
    }

    func code() -> Code
    {
        return self.number;
    }
}

func print_label<T>(pos value: &T)
    with(T: Label)
{
    std.io.print_line(value.text());
}

func main()
{
    let oats: Item =
    {
        name = "oats",
        quantity = 3,
        number = 7,
    };

    print_label<Item>(&oats);
}
```

Run the application:

```sh
bray run
```

It prints:

```text
oats
```

The trait declaration says which operations an implementation must provide. `impl Item(Label)` fills that role for `Item`, much as `impl Label for Item` would in Rust. Inside its methods, `self` is the receiver supplied by a call such as `oats.text()`. We don't write an explicit `&self` parameter. An ordinary `func` method uses shared access, while `mut func` and `consume func` declare mutable and consuming receiver modes.

The generic function can call `value.text()` because `with(T: Label)` guarantees that operation. Bray checks the body against its declared constraints. A type that merely happens to have a `text` method doesn't satisfy `Label` without an explicit trait implementation.

Notice the `<Item>` in `print_label<Item>(&oats)`. Rust can infer generic arguments in many calls, or let us supply them with `::<Item>`. Bray requires the generic arguments explicitly and uses `<Item>` directly after the name. The argument `&oats`, the return type, and the constraint don't fill them in for us.

## Add behavior that belongs to the item

Rust's `impl Item` adds inherent methods without implementing a trait. Bray uses the same spelling. Our label function needs the `Label` contract, but checking whether an item is ready to ship can be an operation on `Item` itself.

Add this implementation to `src/main.bray`:

```bray
impl Item
{
    func is_ready() -> bool
    {
        return self.quantity >= 3;
    }
}
```

Append this to `main`:

```bray
if oats.is_ready()
{
    std.io.print_line("Ready to ship");
}
```

Run `bray run`. The new line is:

```text
Ready to ship
```

Both `oats.text()` and `oats.is_ready()` use method-call syntax, but they get their behavior from different places. `text` fulfills `Label`, while `is_ready` is an inherent member of `Item`. Only the package that defines a named type can add inherent implementations for it. Those implementations contribute to the type automatically within the selected product, without an import to activate each one. Trait implementations have their own participation rules, which we'll use next.

## Label a different kind of value

Our generic printer shouldn't need a new version just because a notice is not an inventory item. In Rust, we'd implement the same trait for the notice. Add these declarations in Bray:

```bray
struct Notice
{
    message: string;
    reference: string;
}

impl NoticeLabel = Notice(Label)
{
    type Code = string;

    func text() -> string
    {
        return self.message;
    }

    func code() -> Code
    {
        return self.reference;
    }
}
```

Then append this to `main`:

```bray
let notice: Notice =
{
    message = "Keep dry",
    reference = "DRY",
};

print_label<Notice>(&notice);
```

Run `bray run` to add:

```text
Keep dry
```

`NoticeLabel` is a name for the implementation. It isn't another type or a wrapper around `Notice`, and we still pass a `Notice` to `print_label`. Our earlier `impl Item(Label)` is unnamed. Both forms fulfill a trait application, but a named implementation can be imported by name or listed in an implementation overload family.

All of these declarations are in `app`, so both implementations already participate here. Across modules or packages, trait implementations participate through explicit `using` declarations or re-exports. That explains the imports from part 3, such as `using std.collection.ListReadIterable;`. Depending on the package alone doesn't silently bring all its implementations into play. Inherent implementations follow the owning-package rule from the previous section instead.

## Return the implementation's associated type

Rust traits can declare associated types. `Code` serves that role in our Bray trait: `Item` chooses an integer code, while `Notice` chooses a string. The caller supplies the implementing type, and its implementation determines the associated type.

Let's retrieve a code without forcing every label to use the same type. Add this function:

```bray
func label_code<T>(pos value: &T) -> T(Label).Code
    with(T: Label)
{
    return value.code();
}
```

Append these statements to `main`:

```bray
let item_code: i32 = label_code<Item>(&oats);
let notice_code: string = label_code<Notice>(&notice);

if item_code == 7
{
    std.io.print_line("Item code: seven");
}

std.io.print_line(notice_code);
```

Run `bray run`. The application adds:

```text
Item code: seven
DRY
```

`T(Label).Code` is Bray's qualified access to the associated type, comparable to Rust's `<T as Label>::Code`. With `T` set to `Item`, it means `i32`. With `T` set to `Notice`, it means `string`. The result annotation doesn't choose the implementation or its associated type. Those choices are already determined by the explicit generic argument and the exact trait application.

## Use a trait view

Sometimes a caller needs to hide the concrete type and expose only a trait's operations. Rust uses trait objects such as `&dyn Trait` for this. Bray provides trait views, with `&view Label` giving shared borrowed access through the `Label` contract.

Append this to `main`:

```bray
let label_view: &view Label = &oats;

std.io.print_line(label_view.text());
```

Run `bray run`. The new line is:

```text
oats
```

`print_label<Item>` retains `Item` as its concrete type. The `label_view` binding hides that type and dispatches `text()` through the selected implementation. The view borrows `oats`, so the storage and access dependencies from part 3 still apply. It neither copies the item nor takes ownership. A bare `view Label` is unsized, so views need an access or storage form such as `&view Label`, `&mut view Label`, or `box view Label`.

The available methods matter here. `text()` returns the fixed type `string`, so it works through this view. `code()` exposes the unfixed associated type `Code`, so it is not available through `&view Label`. Rust lets a trait object fix an associated type with syntax such as `dyn Label<Code = i32>`. Bray's design is different: if a related type must be exposed through a view, make it an input to a generic trait application. We'll use a generic trait shortly. Methods with their own generic parameters or with `Self` outside the receiver are also excluded from a view, as are static functions.

## Give two functions one call name

Rust doesn't let us declare two free functions with the same name and different parameter types. We'd normally choose separate names or introduce a trait. Bray also requires separate declaration names, but lets us group those declarations into an explicit overload family.

We already know how to get text from an item. We'll now let the same call name accept either an item or text that is ready to print. Add:

```bray
func label_item(pos item: &Item) -> string
{
    return item.text();
}

func label_text(pos text: string) -> string
{
    return text;
}

overload label =
{
    label_item,
    label_text,
}
```

Append these calls to `main`:

```bray
std.io.print_line(label(&oats));
std.io.print_line(label("Fragile"));
```

Run `bray run`. The new lines are:

```text
oats
Fragile
```

The supplied argument selects the arm: `&Item` matches `label_item`, and `string` matches `label_text`. Both functions keep their own names, so we can still call `label_item(&oats)` directly. Declaring two functions named `label` would not create this family.

Selection must leave exactly one applicable arm. Bray doesn't rank candidates by which seems more specific, and the expected result type doesn't break a tie. Default arguments don't fill gaps during overload selection either: calls through the family must explicitly supply every parameter of the selected arm. A direct call to an arm can use that function's defaults. We'll deliberately create an ambiguous call in the exercise at the end.

## Choose between two implementations of a generic trait

The donkey is still waiting for its packing slip. A short label can just name the contents, but the slip should say whether packing is complete. In Rust, a trait such as `Render<Style>` could have separate implementations for `Item` with different `Style` arguments. Bray can express those implementations too, with an additional requirement: multiple applications of the same generic trait for one subject must be named and grouped in an explicit implementation overload family.

Add these declarations:

```bray
struct Brief
{
}

struct Detailed
{
}

trait Render<Style>
{
    func render() -> string;
}

impl ItemBriefRender = Item(Render<Brief>)
{
    func render() -> string
    {
        return self.name;
    }
}

impl ItemDetailedRender = Item(Render<Detailed>)
{
    func render() -> string
    {
        if self.is_ready()
        {
            return "Packed and ready to ship";
        }

        return "Still needs packing";
    }
}

overload Item(Render) =
{
    ItemBriefRender,
    ItemDetailedRender,
}
```

Append these calls to `main`:

```bray
std.io.print_line(oats(Render<Brief>).render());
std.io.print_line(oats(Render<Detailed>).render());
```

Run `bray run`. The application adds:

```text
oats
Packed and ready to ship
```

The marker types `Brief` and `Detailed` are inputs to the trait application. We don't construct either one. In `oats(Render<Brief>).render()`, the qualifier identifies the exact trait application and therefore the implementation to use. `Render<Detailed>` identifies the other one. The family groups named implementations, not alternative definitions of `Item`. There is still one `Item` type and one owned value in `oats`.

An unqualified `oats.render()` can't choose between these two arms. They have the same receiver mode and accept the same arguments, so both match. A `string` result annotation won't help (both descriptions are strings, however firmly we annotate them). In general, explicit method arguments, receiver matching, static constraints, and target availability can distinguish arms. Result types and associated-type outputs cannot.

There is also a separate coherence rule: at most one participating implementation can satisfy an exact subject and trait application. A second implementation of `Item(Render<Brief>)` would conflict even if we added it to the family. Overload families allow distinct applications such as `Render<Brief>` and `Render<Detailed>`. They don't provide specialization or permission to duplicate an exact implementation.

## Check the complete labels and packing slip

Run the completed application:

```sh
bray run
```

Apart from build progress, the full output is:

```text
oats
Ready to ship
Keep dry
Item code: seven
DRY
oats
oats
Fragile
oats
Packed and ready to ship
```

For an optional exercise, add this function and include `label_number` as a third arm of `overload label`:

```bray
func label_number(pos item: &Item) -> i32
{
    return item.number;
}
```

Then replace the existing `std.io.print_line(label(&oats));` call with:

```bray
let description: string = label(&oats);

std.io.print_line(description);
```

Run `bray check`. The call is ambiguous because both `label_item` and `label_number` accept `&Item`. The compiler also reports that the two arms have conflicting call signatures. The expected `string` result does not eliminate the integer-returning arm. Remove `label_number` from the overload family, change the call to `label_item(&oats)`, and run `bray run` to restore the output. The separately named `label_number` function can stay: the conflict comes from grouping it with an arm that accepts the same arguments.

You can also try `let slip: string = oats.render();` in `main`. `bray check` must reject that call because both rendering implementations match. Remove the added statement afterward. The two explicit `Render<...>` calls remain valid.

## Next up

Part 5 will manage resource lifetimes, including cleanup when control flow exits early. Our functions have so far printed labels without owning an external resource. Writing them to a file gives us a reason to look at what happens when that resource must be closed.

The language reference covers [generic functions](../../language/callables/generic-functions.md), [traits and associated members](../../language/types/traits.md), [implementations](../../language/types/implementations.md), [trait views](../../language/types/type-forms.md#trait-view-type-form), [function overloads](../../language/callables/function-overloading.md), and [implementation overload families](../../language/types/implementations.md#trait-implementation-overload-families).
