# Interface diff

`diff` enumerates the differences between two canonical interfaces and assigns
each change a default severity. It never claims compatibility; that is
`compat`'s job.

## Change kinds

Functions: `function_added`, `function_removed`, `function_changed`, `function_renamed`.
Types: `type_added`, `type_removed`, `type_changed`, `type_renamed`.
Events: `event_added`, `event_removed`, `event_changed`, `event_renamed`.

## Default severity rules

| Change | Severity |
| ------ | -------- |
| Function added | non-breaking |
| Function removed | breaking |
| Function signature changed | breaking |
| Function documentation only | informational |
| Type added | non-breaking |
| Type removed | breaking |
| Type renamed (identical shape) | breaking |
| Type definition changed | breaking |
| Type documentation only | informational |
| Event added | non-breaking |
| Event removed | breaking |
| Event renamed (identical shape) | breaking |
| Event definition changed | breaking |

## Determinism

Changes are sorted by kind rank, then path, then detail. Running `diff` twice on
the same inputs yields byte-identical output.

## Output

Human mode prints one line per change with a marker (`!` breaking, `+`
non-breaking, `~` informational). JSON mode emits the full `InterfaceDiff`,
including `old`/`new` renderings for signature and type changes.

`diff --markdown` renders a deterministic Markdown report grouped by severity,
suitable for pull requests and reports. Identical inputs produce identical
Markdown.

## Rename detection

A rename is reported as a single `function_renamed` change only when it is
unambiguous: exactly one removed function and one added function share an
identical signature (parameter names and types, and return type), and the
signature has at least one parameter. Signatures without parameters are too
generic to anchor a rename and are never paired. If the pairing is ambiguous (for
example, two removed functions with the same signature and one addition), the
diff does **not** guess; it reports the removals and addition separately. A
rename is breaking by default because callers use names.

This is a confidence rule, not proof of intent. The matching signature is what
raises confidence; the change detail marks the pairing as heuristic so readers
can check the old and new renderings. A removal and addition with different
signatures always stay separate.

## Type renames

A user-defined type (struct, union, enum, or error enum) rename is reported as
`type_renamed` only when exactly one removed type and one added type share the
same canonical shape and the pairing is unambiguous. The canonical shape is the
type kind plus its ordered fields, cases, and payload types (and enum values);
the type name and documentation are excluded. A struct is never paired with a
union, and an enum is never paired with an error enum even when their cases
match. Different shapes, or ambiguous pairings (for example, two removed structs
with one added struct of the same shape), are reported as separate removals and
additions. A type rename is breaking by default because consumers refer to types
by name.

## Event renames

An event rename is reported as `event_renamed` only when exactly one removed
event and one added event share the same shape and the pairing is unambiguous.
The shape covers the prefix topics, the ordered parameters (names, types, and
topic/data locations), and the data format; the event name and documentation
are excluded. Ambiguous or differing shapes are never paired. An event rename is
breaking by default because indexers and consumers filter by event name.

## What it does not do

- It does not decide compatibility. Use `compat`.
- It does not detect function renames with changed signatures; those appear as a
  removal plus an addition.
- It does not prove intent. Rename pairing is a heuristic based on structural
  identity, not a semantic guarantee.
- It does not follow renames into references. A renamed type still referenced by
  its old name in a function signature shows up as a signature change.
