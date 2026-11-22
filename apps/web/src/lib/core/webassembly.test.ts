import { describe, expect, it } from 'vitest';
import { canRunWebAssembly } from './webassembly';

describe('whether the browser can run the core', () => {
	it('can where WebAssembly takes the smallest module', () => {
		expect(canRunWebAssembly()).toBe(true);
		expect(canRunWebAssembly({ WebAssembly })).toBe(true);
	});

	it('cannot where WebAssembly is switched off', () => {
		expect(canRunWebAssembly({})).toBe(false);
		expect(canRunWebAssembly({ WebAssembly: undefined })).toBe(false);
	});

	it('cannot where WebAssembly is there but refuses or breaks down', () => {
		expect(canRunWebAssembly({ WebAssembly: { validate: () => false } })).toBe(false);
		expect(
			canRunWebAssembly({
				WebAssembly: {
					validate: () => {
						throw new Error('WebAssembly is not allowed here');
					}
				}
			})
		).toBe(false);
	});

	it('cannot where reading WebAssembly itself throws', () => {
		const scope = {
			get WebAssembly(): typeof WebAssembly {
				throw new ReferenceError('WebAssembly is not defined');
			}
		};
		expect(canRunWebAssembly(scope)).toBe(false);
	});
});
