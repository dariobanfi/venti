// Removes from the core's WebAssembly module what only the phones use.
//
//   node scripts/strip-core.mjs <module.wasm>
//
// The core is one crate for every platform, and its interface for Swift and
// Kotlin (UniFFI) is compiled into the browser's module too: several
// hundred exported functions that no page ever calls. Each keeps alive what
// it reaches, which is everything the phones have and the browser does not:
// the cockpit, the recorder, the assistant. Taking those exports out lets
// the optimiser drop all of it. `scripts/build-core.sh` runs this.

import { readFileSync, writeFileSync } from 'node:fs';
import binaryen from 'binaryen';

/** How UniFFI names what it exports for the crate `venti-core`. */
const PHONES_ONLY = /^(ffi|uniffi)_venti_core_/;

const path = process.argv[2];
if (!path) {
	console.error('usage: node scripts/strip-core.mjs <module.wasm>');
	process.exit(2);
}

const before = readFileSync(path);
// The module says itself which WebAssembly features it uses. The optimiser
// keeps to those, so the result runs wherever the original did.
const module = binaryen.readBinary(before);

let removed = 0;
for (let index = module.getNumExports() - 1; index >= 0; index -= 1) {
	const { name } = binaryen.getExportInfo(module.getExportByIndex(index));
	if (PHONES_ONLY.test(name)) {
		module.removeExport(name);
		removed += 1;
	}
}

// Small before fast, like the Rust profile the module was built with.
binaryen.setOptimizeLevel(2);
binaryen.setShrinkLevel(1);
module.optimize();
if (!module.validate()) {
	console.error('The stripped module is not valid WebAssembly.');
	process.exit(1);
}

const after = module.emitBinary();
writeFileSync(path, after);
module.dispose();

const kilobytes = (bytes) => `${Math.round(bytes.length / 1024)} KB`;
console.log(
	`${path}: ${removed} exports for the phones removed, ${kilobytes(before)} to ${kilobytes(after)}`
);
