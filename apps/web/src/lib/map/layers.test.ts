import { describe, expect, it } from 'vitest';
import { aerialOn, keepAerial, keepLayersOff, layersOff } from './layers';

function memory() {
	const kept = new Map<string, string>();
	return {
		getItem: (key: string) => kept.get(key) ?? null,
		setItem: (key: string, value: string) => void kept.set(key, value)
	};
}

describe('the aerial view', () => {
	it('is off until the pilot asks for it, and remembered by the browser', () => {
		const storage = memory();
		expect(aerialOn(storage)).toBe(false);
		keepAerial(storage, true);
		expect(aerialOn(storage)).toBe(true);
		keepAerial(storage, false);
		expect(aerialOn(storage)).toBe(false);
	});

	it('is off where nothing can be read', () => {
		expect(aerialOn(undefined)).toBe(false);
		const refusing = {
			getItem: () => {
				throw new Error('denied');
			},
			setItem: () => {
				throw new Error('denied');
			}
		};
		expect(aerialOn(refusing)).toBe(false);
		expect(() => keepAerial(refusing, true)).not.toThrow();
	});
});

describe('the layers switched off on the map', () => {
	it('are remembered by the browser, in the chips’ order', () => {
		const storage = memory();
		expect(layersOff(storage)).toEqual([]);
		keepLayersOff(storage, ['pilots', 'wind']);
		expect(layersOff(storage)).toEqual(['wind', 'pilots']);
	});

	it('are none where nothing can be read, or what is there is no list of layers', () => {
		expect(layersOff(undefined)).toEqual([]);
		const storage = memory();
		storage.setItem('venti.map-layers-off', '{"wind":true}');
		expect(layersOff(storage)).toEqual([]);
		storage.setItem('venti.map-layers-off', 'not json');
		expect(layersOff(storage)).toEqual([]);
		const refusing = {
			getItem: () => {
				throw new Error('denied');
			},
			setItem: () => {
				throw new Error('denied');
			}
		};
		expect(layersOff(refusing)).toEqual([]);
		expect(() => keepLayersOff(refusing, ['wind'])).not.toThrow();
	});
});
