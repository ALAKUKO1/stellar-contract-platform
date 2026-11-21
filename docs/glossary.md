# Glossary

Terms used throughout the documentation, with brief definitions and links to
authoritative sources. Terms are specific to [Stellar](https://developers.stellar.org/docs)
and its [Soroban](https://developers.stellar.org/docs) smart-contract platform.
If a term is not listed here, prefer the upstream protocol definitions over an
informal description.

## Stellar and Soroban

- **Stellar** — A public distributed ledger and payment network. Its protocol is
  versioned and implemented by the Stellar Core and Horizon software.
- **Soroban** — Stellar's smart-contract platform. Contracts are compiled to
  WebAssembly and run by the Soroban host inside Stellar Core.
- **Contract** — A WASM module deployed to the ledger that exposes callable
  functions. This project inspects and verifies contracts without executing
  them.
- **Contract ID** — The stable identifier of a deployed contract instance.
  Soroban contract IDs use the `C...` strkey form.

## Data formats

- **XDR** — External Data Representation, the binary serialization format used
  across the Stellar protocol. The schema lives in
  [`stellar/stellar-xdr`](https://github.com/stellar/stellar-xdr); this project
  consumes the `stellar-xdr` Rust crate.
- **`SCVal`** — The XDR value type (`ScVal`) that represents a Soroban contract
  value: numbers, booleans, bytes, strings, addresses, vectors, maps, and
  contract-defined types. It is the runtime value encoding, distinct from the
  interface type model.
- **`contractspecv0`** — The name of the WASM custom section that carries a
  contract's interface specification: functions, user-defined types, and events.
  It is the source for the [contract specification](contract-specifications.md).
- **`LedgerEntry`** — The XDR type describing one entry stored in the ledger,
  such as a contract instance, contract data, or account. Deployment inspection
  reads `LedgerEntry` values from an RPC fixture.
- **strkey** — Stellar's human-readable, checksummed string encoding for keys
  and identifiers (for example `G...` accounts, `C...` contracts). It is not the
  same as the raw binary value.

## Contract interface

- **Interface** — The public surface of a contract: its functions, their
  parameter and return types, user-defined types, and events. See
  [fingerprinting.md](fingerprinting.md) and [interface-diff.md](interface-diff.md).
- **UDT** — *User-Defined Type*: a struct, union, enum, or error enum declared
  in the contract specification. UDTs are referenced by name from function and
  field types.
- **Canonical ordering** — The deterministic sort applied before hashing or
  comparing an interface: name-keyed collections are sorted; sequence-keyed
  collections keep declaration order. See
  [ADR-0005](adr/0005-canonical-ordering.md).
- **Interface fingerprint** — The SHA-256 of the canonical JSON encoding of an
  interface. It identifies the interface, not the implementation. See
  [fingerprinting.md](fingerprinting.md).
- **WASM hash** — The SHA-256 of the exact WASM artifact bytes, also called the
  *artifact fingerprint*. It identifies the build, and is the value compared by
  `verify artifact`. A WASM hash and an interface fingerprint answer different
  questions and are not interchangeable.

## Ledger and networking

- **TTL** — *Time To Live*: the ledger sequence window after which a Soroban
  storage entry (in particular temporary storage) may be removed unless its TTL
  is extended.
- **Temporary storage** — Soroban storage whose entries are discarded when their
  TTL lapses. Durable data must not be stored there.
- **RPC** — The JSON-RPC interface used to read ledger state and submit
  transactions. This project ships a deterministic fixture transport; see
  [ADR-0003](adr/0003-fixture-first-rpc.md) and [rpc-errors.md](rpc-errors.md).
- **Network passphrase** — The string that identifies a Stellar network
  (`testnet`, `futurenet`, `mainnet`, or a custom network) and is mixed into
  transaction signatures.

## This project

- **Compatibility** — A verdict produced by applying an explicit policy to an
  [interface diff](interface-diff.md). It is never a bare boolean. See
  [compatibility.md](compatibility.md).
- **Envelope** — The versioned JSON wrapper around every machine-readable report,
  containing `schema_version`, `kind`, `tool`, `version`, and `data`.
- **Artifact** — A compiled `.wasm` file supplied for inspection or verification.

## See also

- [Architecture](architecture.md)
- [Contract specifications](contract-specifications.md)
- [Fingerprinting](fingerprinting.md)
- [the Stellar developer documentation](https://developers.stellar.org/docs)
- [the Stellar protocol repository](https://github.com/stellar/stellar-protocol)
