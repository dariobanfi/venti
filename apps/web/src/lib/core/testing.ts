/**
 * Helpers for unit tests that call the real core. They run in Node, where
 * the WebAssembly module is read from disk instead of fetched.
 */

import { readFileSync } from 'node:fs';
import { initSync } from './pkg/venti_core_wasm.js';

let started = false;

/** Starts the core's WebAssembly module once for the test process. */
export function startCore(): void {
	if (started) return;
	const module = readFileSync(new URL('./pkg/venti_core_wasm_bg.wasm', import.meta.url));
	initSync({ module });
	started = true;
}

/**
 * The made-up sample flight, the one copy every app shares. It is written
 * by `cargo run -p venti-flight --example sample_flight`.
 */
export function sampleFlight(): string {
	const file = new URL('../../../../ios/Venti/Resources/sample.igc', import.meta.url);
	return readFileSync(file, 'utf8');
}

/** The sample flight on another day of July 2026, which makes it another flight. */
export function sampleFlightOn(day: number): string {
	return sampleFlight().replace('040726', `${String(day).padStart(2, '0')}0726`);
}
