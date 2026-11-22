/**
 * Whether this browser can run the core at all. The core is WebAssembly,
 * and a browser can have it switched off: Chrome takes `WebAssembly` away
 * where JavaScript optimisation is not allowed for a site, which an
 * organisation's policy can set, and Safari's Lockdown Mode does the same.
 * The page asks before it starts the worker, and the worker asks again
 * before it loads the module, since it has a global of its own.
 */

/** The smallest module there is: the magic number and version 1, nothing in it. */
const EMPTY_MODULE = new Uint8Array([0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]);

/** The name of the error the worker sends when it has no WebAssembly. */
export const NO_WEBASSEMBLY = 'NoWebAssembly';

/**
 * Whether `scope` (the page's or the worker's global) has a WebAssembly
 * that takes the smallest module. Nothing it does throws.
 */
export function canRunWebAssembly(
	scope: { WebAssembly?: Pick<typeof WebAssembly, 'validate'> } = globalThis
): boolean {
	try {
		const wasm = scope.WebAssembly;
		if (typeof wasm !== 'object' || wasm === null || typeof wasm.validate !== 'function') {
			return false;
		}
		return wasm.validate(EMPTY_MODULE) === true;
	} catch {
		return false;
	}
}

/** The error for a browser without WebAssembly. Its message is for a log, never for the pilot. */
export function noWebAssembly(): Error {
	const error = new Error('This browser has no WebAssembly.');
	error.name = NO_WEBASSEMBLY;
	return error;
}
