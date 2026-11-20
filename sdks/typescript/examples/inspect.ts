// Inspect a contract through a running Observatory API.
//
//   node examples/inspect.ts http://127.0.0.1:8080 sco_... contract.wasm

import { readFile } from "node:fs/promises";

import { ObservatoryClient } from "../src/index.ts";

const [baseUrl, apiKey, wasmPath] = process.argv.slice(2);
if (!baseUrl || !apiKey || !wasmPath) {
  console.error("usage: node examples/inspect.ts <BASE_URL> <API_KEY> <WASM>");
  process.exit(2);
}

const client = new ObservatoryClient({ baseUrl, apiKey, timeoutMs: 30_000 });
const wasm = new Uint8Array(await readFile(wasmPath));
const report = await client.inspect(wasm, true);

console.log(`artifact: ${report.wasm.artifact_sha256}`);
console.log(`exports: ${report.wasm.exports.length}`);
console.log(
  `contract spec: ${report.wasm.has_contract_spec ? "present" : "absent"}`,
);
if (report.audit) {
  console.log(`audit: ${report.audit.status} (${report.audit.findings.length} findings)`);
}

const fingerprint = await client.fingerprint(wasm);
console.log(`interface: ${fingerprint.interface_sha256 ?? "<none>"}`);
