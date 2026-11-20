#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Static inspection only: never execute the module or touch the filesystem.
    let _ = observatory_wasm::inspect(data);
});
