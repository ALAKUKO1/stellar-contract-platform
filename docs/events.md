# Events

`events` decodes a fixture of concrete emitted events and filters it. This is
different from the *declared* events in the contract specification.

## Fixture format

```json
{
  "events": [
    {
      "contractId": "C...",
      "ledger": 123,
      "txHash": "abcd",
      "type": "contract",
      "topics": ["<base64 ScVal>", "<base64 ScVal>"],
      "data": "<base64 ScVal>"
    }
  ]
}
```

A bare array is also accepted. `topics` and `data` are optional; `type` defaults
to `contract`. Topics and data are decoded as Stellar `ScVal` XDR and rendered to
JSON.

## Event types

Stellar has three event types, and `events` normalizes all of them to the same
`EventRecord` shape. `event_type` is always lowercase, whether it came from a
fixture (`type` is trimmed and lowercased) or from raw XDR (the enum name is
lowercased). Normalization is deterministic: the same input always yields the
same record.

| Type | Emitted by | `contract_id` | Typical topics |
| ---- | ---------- | ------------- | -------------- |
| `system` | The host, for protocol-level events. | Usually absent. | Protocol-defined symbols. |
| `contract` | A contract, through `contractevents`. | Present for the emitting contract. | Application events such as `transfer`. |
| `diagnostic` | The host, when diagnostics are enabled. | Present when the event is tied to a contract, otherwise absent. | Diagnostic kind, for example `fn_call`, `log`, or `error`. |

Diagnostic events are not application events: they describe host execution (call
traces, logs, errors) and may appear without a contract id. Their topics, data,
and rendered JSON are decoded and emitted exactly like the other types, so
consumers can rely on one record shape. A diagnostic event is distinguished only
by `event_type: "diagnostic"`; no fields are reinterpreted or dropped.


## Raw `ContractEvent` XDR

A base64 `ContractEvent` blob can be passed directly instead of a fixture
document. It can also appear as an `xdr` field on a fixture entry (`eventXdr` is
accepted as an alias):

```json
{
  "events": [
    {
      "xdr": "<base64 ContractEvent>",
      "ledger": 123,
      "txHash": "abcd"
    }
  ]
}
```

The contract id, event type, topics, and data are normalized exactly like a
fixture entry. `ledger` and `txHash` are optional metadata that only a fixture
can provide; a bare blob leaves them `null`. Malformed base64 or XDR returns a
structured decode error.

## Filtering

Filters are applied in this order: contract, then type, then topic, then limit.
An omitted filter is a no-op.

| Flag | Behavior |
| ---- | -------- |
| `--contract <ID>` | Exact contract id match. Events without a contract id, including diagnostic events emitted outside a contract, never match. |
| `--topic <TEXT>` | Substring match against extracted topic text (symbols/strings) or rendered topics. Works for diagnostic topics such as `fn_call` and `log`. |
| `--type <TYPE>` | Case-insensitive event type match. `--type diagnostic` selects diagnostic events regardless of their contract id. |
| `--limit <N>` | Applied last. |

Diagnostic events are filtered by the same rules as system and contract events.
The only behavior worth calling out is the contract filter: it matches the
`contract_id` field exactly, so a diagnostic event with no contract id is kept
only when no contract filter is supplied. Type and topic filters work on the
normalized fields and do not special-case diagnostic events.

## Limitations

- Declared events (specification) and emitted events (fixture) are distinct.
  Declared events describe *shape*; fixtures contain *instances*.
- Topics that are not symbols or strings cannot be text-filtered, but are still
  matched against their rendered JSON.
- Historical event retrieval over RPC is not implemented yet; fixtures are the
  supported offline path.
