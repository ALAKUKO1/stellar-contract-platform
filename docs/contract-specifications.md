# Contract specifications

`spec` decodes the `contractspecv0` custom section into a normalized,
parser-independent model.

## Model

```text
ContractSpec
  functions:   SpecFunction { name, doc, inputs: [SpecParam], outputs: [SpecType] }
  structs:     SpecStruct   { name, doc, fields: [SpecField] }
  unions:      SpecUnion    { name, doc, cases: [SpecUnionCase] }
  enums:       SpecEnum     { name, doc, cases: [SpecEnumCase{name,value}] }
  error_enums: SpecErrorEnum{ ... }
  events:      SpecEvent    { name, doc, prefix_topics, params, data_format }
```

Types (`SpecType`) cover the full `SCSpecTypeDef` surface: primitives
(`val`, `bool`, `void`, `error`, `u32`, `i32`, `u64`, `i64`, `u128`, `i128`,
`u256`, `i256`, `timepoint`, `duration`, `bytes`, `string`, `symbol`,
`address`, `muxed_address`), composites (`option`, `result`, `vec`, `map`,
`tuple`, `bytes[n]`), and `udt` references.

## Ordering semantics

- Functions, structs, unions, enums, error enums, and events are canonicalized
  **by name**; declaration order is not semantic.
- Function parameters, struct fields, union cases, and enum cases preserve
  **declared order** because it is semantic for XDR encoding and the call ABI.

## Validation

`spec` also runs validation and reports issues:

- duplicate function names,
- duplicate user-defined type names,
- references to unknown types,
- more than one return type on a function,
- names longer than the XDR `SCSymbol` limit of 32 bytes,
- duplicate event names (warning).

Name-length validation covers function and parameter names, user-defined type
names and their fields/cases, and event names, topics, and parameters. Overlong
names are reported, never silently truncated.

## Documentation rendering

Human `spec` output renders the documentation strings decoded from the
specification: function docs and type docs are printed beneath their entries.
`spec --type <NAME>` additionally prints the type doc and each field or case
doc. Documentation is omitted when empty; no placeholder text is invented.
Rendering preserves declared field/case order, so output is deterministic.

The JSON output is structured and unchanged: documentation remains a `doc` field
on each model entity.

## Supported and unsupported

Supported: all `SCSpecEntry` variants in the Stellar XDR schema used by this
project (`FunctionV0`, `UdtStructV0`, `UdtUnionV0`, `UdtEnumV0`,
`UdtErrorEnumV0`, `EventV0`).

If a future XDR schema adds an entry kind, decoding peeks at the 4-byte
`ScSpecEntryKind` discriminant before decoding an entry. An unknown kind stops
decoding and returns an `unsupported` error naming the raw discriminant, its
byte offset, and how many entries were decoded before it. This keeps newer spec
streams from being reported as malformed. Entries decoded before the unknown
kind are included in the error message as that count; later entries are not
decoded. All other malformed input (a truncated discriminant or body) still
returns a `spec` error.
