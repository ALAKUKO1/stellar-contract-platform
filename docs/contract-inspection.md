# Contract inspection

`inspect` answers "what is inside this artifact?" without executing it.

## Reported fields

| Field | Meaning |
| ----- | ------- |
| `size_bytes` | Artifact size. |
| `version` | WASM binary version. |
| `artifact_sha256` | SHA-256 of the artifact bytes; the artifact identity. |
| `type_count` | Number of declared types. |
| `function_count` | Number of declared functions. |
| `table_count` / `global_count` | Declared tables and globals. |
| `start` | Start function index, when the module declares one. |
| `data_segment_count` | Number of data segments. |
| `data_segments` | Bounded per-segment sizes; contents are never retained. |
| `element_segment_count` | Number of element segments. |
| `element_segments` | Bounded per-segment item counts. |
| `tables` | Element type, initial size, and maximum size. |
| `memories` | Initial/maximum pages and shared flag. |
| `imports` | Module, name, and kind, in module order. |
| `exports` | Name and kind, in module order. |
| `custom_sections` | Name and payload size, in module order. |
| `code_section_bytes` | Size of the code section. |
| `has_contract_spec` | Whether `contractspecv0` is present. |
| `contract_spec_bytes` | Size of that section, if present. |
| `truncated` | Whether a list hit a configured limit. |

## Safety and limits

- Inspection never executes the module.
- Input is bounded to 64 MiB (`MAX_WASM_BYTES`).
- Import/export/custom-section, table, data, and element lists are bounded;
  `truncated` reports clipping. Counts are still read from section headers, so
  they stay exact when a summary list is clipped.
- Malformed input returns a structured error (`wasm error`).

## Ordering

Imports, exports, and custom sections are reported in module order, which is
meaningful for WASM. Counts are exact except when `truncated` is true.

## Security heuristics

`inspect --security` merges bounded artifact heuristics (for example: no code
section, no `contractspecv0`, truncated listing) with interface heuristics. See
[security.md](security.md). These are not an audit.
