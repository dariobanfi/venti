import { describe, expect, it } from 'vitest';
import { keepLayersOff, layersOff } from './layers';

function memory() {
	const kept = new Map<string, string>();
	return {
		getItem: (key: string) => kept.get(key) ?? null,
		setItem: (key: string, value: string) => void kept.set(key, value)
	};
}

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
