import { describe, expect, it } from 'vitest';
import type {
	AirspaceCrossing,
	AirspaceFile,
	AirspaceImport,
	AirspaceShape,
	Credit,
	KeptAirspace
} from '#lib/core/index.ts';
import { createFormat } from '#lib/design/format.ts';
import { airspaceFeatures, airspaceLayers, AIRSPACE_COLORS, rank } from './layers';
import { AirspaceInView } from './view';
import {
	airspaces,
	checkCaveat,
	countryNames,
	creditLine,
	crossingLines,
	fileDetail,
	importSummary,
	inALine,
	keptDetail,
	label,
	leftOutWarning,
	nothingEntered
} from './words';

const text = createFormat('en-GB', 'Europe/Berlin');

/** 18 November 2026, 19:05 UTC. */
const NOW = 1_795_028_700;

function shape(given: Partial<AirspaceShape> = {}): AirspaceShape {
	return {
		id: 'file-zones:ctr-test',
		name: 'CTR TEST',
		kind: 'Class D control zone',
		lower: 'GND',
		upper: '9000 ft AMSL',
		policy: 'avoid',
		outline: [
			{ lat: 47.7, lon: 11.7 },
			{ lat: 47.7, lon: 11.8 },
			{ lat: 47.75, lon: 11.75 },
			{ lat: 47.7, lon: 11.7 }
		],
		holes: [],
		...given
	};
}

function file(given: Partial<AirspaceFile> = {}): AirspaceFile {
	return {
		storedName: '20261118-zones.txt',
		name: 'zones',
		importedUnixS: NOW,
		airspaces: 2,
		leftOut: 0,
		...given
	};
}

const OPENAIP: Credit = {
	source: 'openaip',
	provider: 'openAIP',
	notice: 'Not certified for navigation.',
	text: 'Airspace data from openAIP.',
	licence: 'CC BY-NC 4.0',
	url: 'https://www.openaip.net'
};

function kept(given: Partial<KeptAirspace> = {}): KeptAirspace {
	return {
		source: 'openaip',
		provider: 'openAIP',
		airspaces: 88,
		fetchedUnixS: NOW,
		writtenUnixS: NOW - 86_400,
		keptUnixS: NOW,
		area: { south: 45.7, west: 8.7, north: 49.7, east: 14.7 },
		saysWhatItCovers: true,
		countries: ['DE', 'AT'],
		credit: OPENAIP,
		...given
	};
}

describe('what bringing in a file did, in words', () => {
	const result = (given: Partial<AirspaceImport> = {}): AirspaceImport => ({
		airspaces: 1018,
		leftOut: 0,
		assumed: 0,
		problems: [],
		...given
	});

	it('says what the file gave', () => {
		expect(importSummary(result(), 'germany.txt', text)).toBe('germany.txt: 1,018 airspaces read.');
		expect(importSummary(result({ airspaces: 1 }), 'ctr.txt', text)).toBe(
			'ctr.txt: 1 airspace read.'
		);
	});

	it('says above all what it left out, and that the map lacks it', () => {
		expect(
			importSummary(
				result({ airspaces: 2, leftOut: 1, problems: ['Line 20 (NO SHAPE): no outline.'] }),
				'zones.txt',
				text
			)
		).toBe(
			'zones.txt: 2 airspaces read. 1 airspace could not be read and is left out: it is missing from the map.\nLine 20 (NO SHAPE): no outline.'
		);
		expect(importSummary(result({ leftOut: 3 }), 'x.txt', text)).toContain(
			'3 airspaces could not be read and are left out: they are missing from the map.'
		);
	});

	it('says where the cautious reading was taken', () => {
		expect(importSummary(result({ assumed: 1 }), 'x.txt', text)).toContain(
			'In 1 place something could not be read and the cautious reading was taken.'
		);
		expect(importSummary(result({ assumed: 4 }), 'x.txt', text)).toContain('In 4 places ');
	});
});

describe('a file in the list', () => {
	it('says how much it holds and when it was brought in', () => {
		expect(fileDetail(file(), text)).toBe('2 airspaces · brought in Wed, 18 Nov 2026');
		expect(fileDetail(file({ importedUnixS: undefined, airspaces: 1 }), text)).toBe('1 airspace');
	});

	it('keeps saying what it left out', () => {
		expect(leftOutWarning(file())).toBeUndefined();
		expect(leftOutWarning(file({ leftOut: 1 }))).toBe(
			'1 airspace in this file could not be read. It is missing from the map.'
		);
		expect(leftOutWarning(file({ leftOut: 2 }))).toBe(
			'2 airspaces in this file could not be read. They are missing from the map.'
		);
	});
});

describe('airspace kept from the server', () => {
	it('says how much, how old by its provider’s word, and when it was fetched', () => {
		expect(keptDetail(kept(), text)).toBe(
			'88 airspaces · as of Tue, 17 Nov 2026 · fetched Wed, 18 Nov 2026'
		);
		expect(keptDetail(kept({ writtenUnixS: undefined }), text)).toBe(
			'88 airspaces · fetched Wed, 18 Nov 2026'
		);
	});

	it('names the countries it holds', () => {
		expect(countryNames(['DE', 'AT'], text)).toBe('Germany and Austria');
		expect(countryNames(['CH'], text)).toBe('Switzerland');
		// A code the browser does not know stays as it is.
		expect(countryNames(['not a code'], text)).toBe('not a code');
	});

	it('is credited on the map in a line, with the provider’s notice in two words', () => {
		expect(creditLine([OPENAIP], text)).toBe('Airspace: openAIP, not certified');
		expect(creditLine([{ ...OPENAIP, notice: undefined }], text)).toBe('Airspace: openAIP');
		expect(creditLine([], text)).toBe('');
	});
});

describe('what the maps draw from, in a line', () => {
	it('names the server’s sources and counts the files', () => {
		expect(inALine([], [], text)).toBe('None');
		expect(inALine([kept()], [], text)).toBe('openAIP, 88 airspaces');
		expect(inALine([kept()], [file()], text)).toBe('openAIP, 88 airspaces · 1 file');
		expect(inALine([], [file(), file()], text)).toBe('2 files');
	});

	it('counts airspaces in the reader’s way', () => {
		expect(airspaces(1, text)).toBe('1 airspace');
		expect(airspaces(58980, text)).toBe('58,980 airspaces');
	});
});

describe('a flight checked against airspace', () => {
	/** The sample flight's take-off: 4 July 2026, 10:30 UTC. */
	const START = 1_783_161_000;

	function crossing(given: Partial<AirspaceCrossing> = {}): AirspaceCrossing {
		return {
			id: 'file-zones:ctr-test',
			name: 'CTR TEST',
			kind: 'Class D control zone',
			lower: 'GND',
			upper: '9000 ft AMSL',
			policy: 'avoid',
			entry: {
				trackIndex: 720,
				seconds: 720,
				position: { lat: 47.7, lon: 11.75 },
				altitudeM: 1612
			},
			exit: { trackIndex: 795, seconds: 795, position: { lat: 47.7, lon: 11.74 }, altitudeM: 1498 },
			seconds: 75,
			lowestM: 1498.4,
			highestM: 1612.2,
			uncertain: undefined,
			origin: 'file',
			source: 'zones',
			dataUnixS: NOW,
			...given
		};
	}

	it('says of a stretch which airspace, when, between which heights and for how long', () => {
		expect(crossingLines(crossing(), START, text)).toEqual({
			name: 'CTR TEST',
			airspace: 'Class D control zone · GND to 9000 ft AMSL',
			// Twelve minutes after a take-off at 12:30 where the reader is.
			stretch: '12:42 · 1,498 to 1,612 m',
			duration: '1:15 min'
		});
	});

	it('does not leave an airspace without a name nameless', () => {
		expect(crossingLines(crossing({ name: '' }), START, text).name).toBe('Unnamed airspace');
	});

	it('says so when nothing was entered, and when nothing could be checked', () => {
		expect(nothingEntered({ cover: 'whole', crossings: [crossing()] })).toBeUndefined();
		expect(nothingEntered({ cover: 'whole', crossings: [] })).toBe(
			'The flight entered none of the airspace the app has.'
		);
		expect(nothingEntered({ cover: 'partly', crossings: [] })).toBe(
			'The flight entered none of the airspace the app has.'
		);
		expect(nothingEntered({ cover: 'none', crossings: [] })).toBe(
			'Not checked: the app has no airspace where this flight was flown.'
		);
	});

	it('always says what the check is worth', () => {
		const limits =
			'Checked against the airspace the app has today, not the airspace of the day of the flight, and without knowing what was active. A help for your own review, nothing more.';

		expect(checkCaveat('whole')).toBe(limits);
		expect(checkCaveat('partly')).toBe(
			`Part of this flight lies where the app has no airspace; of that part nothing is known. ${limits}`
		);
		// Nothing was checked: the pilot is told how to change that.
		expect(checkCaveat('none')).toBe(
			'Bring in an airspace file under Settings, Airspace, and the flight is checked against what the app then has.'
		);
	});
});

describe('an airspace on the map', () => {
	it('is named with its floor and ceiling under it, an en dash between them', () => {
		expect(label(shape())).toBe('CTR TEST\nGND – 9000 ft AMSL');
		expect(label(shape({ name: '' }))).toBe('GND – 9000 ft AMSL');
	});

	it('is a polygon with its holes, longitude first', () => {
		const hole = [
			{ lat: 47.71, lon: 11.74 },
			{ lat: 47.71, lon: 11.76 },
			{ lat: 47.72, lon: 11.75 },
			{ lat: 47.71, lon: 11.74 }
		];
		const [feature] = airspaceFeatures([shape({ holes: [hole] })]).features;

		expect(feature.geometry).toMatchObject({ type: 'Polygon' });
		const rings = (feature.geometry as GeoJSON.Polygon).coordinates;
		expect(rings).toHaveLength(2);
		expect(rings[0][0]).toEqual([11.7, 47.7]);
		expect(rings[1][0]).toEqual([11.74, 47.71]);
		expect(feature.properties).toEqual({
			id: 'file-zones:ctr-test',
			policy: 2,
			label: 'CTR TEST\nGND – 9000 ft AMSL'
		});
	});

	it('is drawn by what the core says of it: magenta to avoid, amber for caution, grey when free', () => {
		expect(AIRSPACE_COLORS).toEqual({ avoid: '#D55181', caution: '#C98500', free: '#85847F' });
		expect([rank('free'), rank('caution'), rank('avoid')]).toEqual([0, 1, 2]);

		const layers = airspaceLayers('noto-sans-regular');
		// The least restrictive first, the names on top of all.
		expect(layers.map((layer) => layer.id)).toEqual([
			'airspace-edge-0',
			'airspace-wash-1',
			'airspace-edge-1',
			'airspace-wash-2',
			'airspace-edge-2',
			'airspace-names'
		]);
		const colour = (id: string) => {
			const paint = layers.find((layer) => layer.id === id)!.paint as Record<string, unknown>;
			return paint['line-color'] ?? paint['fill-color'];
		};
		expect(colour('airspace-edge-2')).toBe('#D55181');
		expect(colour('airspace-wash-2')).toBe('#D55181');
		expect(colour('airspace-edge-1')).toBe('#C98500');
		expect(colour('airspace-edge-0')).toBe('#85847F');
	});

	it('is washed only once the pilot has zoomed to a valley, the edges alone before', () => {
		const layers = airspaceLayers('noto-sans-regular');
		const wash = (id: string) =>
			(layers.find((layer) => layer.id === id)!.paint as Record<string, unknown>)['fill-opacity'];

		expect(wash('airspace-wash-2')).toEqual(['interpolate', ['linear'], ['zoom'], 8, 0, 10, 0.1]);
		expect(wash('airspace-wash-1')).toEqual(['interpolate', ['linear'], ['zoom'], 8, 0, 10, 0.06]);
	});

	it('is not drawn below zoom 7, and named only from zoom 9', () => {
		const layers = airspaceLayers('noto-sans-regular');

		for (const layer of layers) {
			expect(layer.minzoom, layer.id).toBe(layer.id === 'airspace-names' ? 9 : 7);
		}
		// Airspace open to a paraglider is drawn, but not named.
		expect((layers.at(-1) as { filter?: unknown }).filter).toEqual(['>', ['get', 'policy'], 0]);
	});
});

describe('when a map asks for its airspace', () => {
	const valley = { south: 47.6, west: 11.6, north: 47.8, east: 11.9 };

	it('asks for a quarter more than it shows, once', () => {
		const view = new AirspaceInView();

		const area = view.wanted(valley, 10, 1)!;
		expect(area.south).toBeCloseTo(47.55);
		expect(area.north).toBeCloseTo(47.85);
		expect(area.west).toBeCloseTo(11.525);
		expect(area.east).toBeCloseTo(11.975);

		// With that drawn, a small move inside it asks for nothing.
		view.drawn(area, 1);
		expect(view.wanted(valley, 10, 1)).toBeUndefined();
		expect(
			view.wanted({ south: 47.58, west: 11.55, north: 47.78, east: 11.85 }, 10, 1)
		).toBeUndefined();
	});

	it('asks again when the map has moved out of what it has', () => {
		const view = new AirspaceInView();
		view.drawn(view.wanted(valley, 10, 1)!, 1);

		expect(view.wanted({ ...valley, north: 48.2 }, 10, 1)).toBeDefined();
	});

	it('asks again when the pilot’s airspace has changed', () => {
		const view = new AirspaceInView();
		view.drawn(view.wanted(valley, 10, 1)!, 1);

		expect(view.wanted(valley, 10, 2)).toBeDefined();
	});

	it('asks for nothing while it is zoomed too far out to draw any', () => {
		const view = new AirspaceInView();

		expect(view.wanted({ south: 44, west: 5, north: 51, east: 18 }, 6.9, 1)).toBeUndefined();
		expect(view.wanted({ south: 44, west: 5, north: 51, east: 18 }, 7, 1)).toBeDefined();
	});
});
