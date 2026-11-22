import { describe, expect, it } from 'vitest';
import {
	aerialOn,
	airspaceOn,
	keepAirspace,
	candidatesOn,
	keepCandidates,
	forecastOn,
	keepAerial,
	keepForecast,
	keepLayersOff,
	keepRadar,
	keepSlopes,
	layersOff,
	radarOn,
	slopesOn
} from './layers';

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

describe('the forecast layer', () => {
	it('is off until the pilot switches it on, and remembered by the browser', () => {
		const storage = memory();
		expect(forecastOn(storage)).toBe(false);
		keepForecast(storage, true);
		expect(forecastOn(storage)).toBe(true);
		keepForecast(storage, false);
		expect(forecastOn(storage)).toBe(false);
		expect(forecastOn(undefined)).toBe(false);
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

describe('airspace on the map tab', () => {
	it('is off until the pilot switches it on, and kept apart from the layers switched off', () => {
		const storage = memory();
		expect(airspaceOn(storage)).toBe(false);
		keepAirspace(storage, true);
		expect(airspaceOn(storage)).toBe(true);
		keepLayersOff(storage, ['airspace', 'wind']);
		expect(layersOff(storage)).toEqual(['wind']);
		storage.setItem('venti.map-layers-off', '["airspace"]');
		expect(layersOff(storage)).toEqual([]);
		expect(airspaceOn(storage)).toBe(true);
		keepAirspace(storage, false);
		expect(airspaceOn(storage)).toBe(false);
		expect(airspaceOn(undefined)).toBe(false);
	});
});

describe('the slopes layer', () => {
	it('is off until the pilot switches it on, and remembered by the browser apart from the others', () => {
		const storage = memory();
		expect(slopesOn(storage)).toBe(false);
		keepSlopes(storage, true);
		expect(slopesOn(storage)).toBe(true);
		expect(radarOn(storage)).toBe(false);
		expect(forecastOn(storage)).toBe(false);
		keepSlopes(storage, false);
		expect(slopesOn(storage)).toBe(false);
		expect(slopesOn(undefined)).toBe(false);
	});
});

describe('the radar layer', () => {
	it('is off until the pilot switches it on, and remembered by the browser apart from the forecast', () => {
		const storage = memory();
		expect(radarOn(storage)).toBe(false);
		keepRadar(storage, true);
		expect(radarOn(storage)).toBe(true);
		expect(forecastOn(storage)).toBe(false);
		keepRadar(storage, false);
		expect(radarOn(storage)).toBe(false);
	});

	it('is off where nothing can be read', () => {
		expect(radarOn(undefined)).toBe(false);
		const refusing = {
			getItem: () => {
				throw new Error('denied');
			},
			setItem: () => {
				throw new Error('denied');
			}
		};
		expect(radarOn(refusing)).toBe(false);
		expect(() => keepRadar(refusing, true)).not.toThrow();
	});
});

describe('the layer of take-offs without a site', () => {
	it('is off until the pilot switches it on, and remembered apart from the radar', () => {
		const storage = memory();
		expect(candidatesOn(storage)).toBe(false);
		keepCandidates(storage, true);
		expect(candidatesOn(storage)).toBe(true);
		expect(radarOn(storage)).toBe(false);
		keepCandidates(storage, false);
		expect(candidatesOn(storage)).toBe(false);
		expect(candidatesOn(undefined)).toBe(false);
	});
});
