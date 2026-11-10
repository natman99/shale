---
tags: reference
---

# Markdown Tour

Live-preview styling, one construct per line. Links: [[Welcome]], [[Another Note]],
[[alias|an aliased link]] and [[A Note That Does Not Exist]].

## Text

**strong**, *emphasis*, ~~strikethrough~~, ==highlight==, `inline code`,
#tags and a [external link](https://example.com).

## Lists and tasks

- a bullet
- [ ] an open task
- [x] a done task

## Quote

> A quoted line, drawn dim and italic, with **bold** inside it.

## Callouts

> [!info] A callout with a title
> The quote markers and the `[!info]` header are hidden, and the box
> spans every line of the block.

> [!shiplog] dad

> [!warning]- A folded callout shows its type name as its title.

> [!error] and

> [!abstract] and

> [!tip] and

> [!example] and

> [!question] and

> [!note] and

## Image

![[demo.png|300]]

## Table

| Note | Links to |
| ---- | :------- |
| [[Welcome]] | [[Another Note]] |
| [[Another Note]] | #tags |

---
A thematic break above, and a code block below:

```rust
fn main() {
    println!("fenced blocks keep their own background");
}
```

## Saved query

`![[…]]` over a `.shq` file draws the answer that query computes, right here. The program
stays in `queries/`; this note borrows its rows. A row naming a note is that note: click it
to open the file.

![[Creature ratings]]
