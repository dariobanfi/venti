import { describe, expect, it } from 'vitest';
import { createFormat } from '#lib/design/format.ts';
import { CREDIT, site } from './testing';
import { NO_SITES, type SitesState } from './watch';
import { asOf, creditLine, detail, marks, nothingListed, withLinks } from './words';

const text = createFormat('en-GB', 'Europe/Berlin');

describe('a site in a line', () => {
	it('says what a named site is, how high it lies and the wind it works in', () => {
		const named = site({
			name: 'Brauneck Süd',
			isNamed: true,
			elevationM: 1540,
			windInWords: 'S to SW'
		});

		expect(detail(named, text)).toBe('Take-off · 1540 m · wind S to SW');
	});

	it('does not repeat what an unnamed site is called already', () => {
		expect(detail(site({ elevationM: 1100 }), text)).toBe('1100 m');
		expect(detail(site(), text)).toBe('No more is known');
	});
});

describe('what a source marks a site as', () => {
	it('is nothing where the source says nothing', () => {
		expect(marks(site())).toEqual([]);
		// "Not official" and "for paragliders" are not marks of their own.
		expect(marks(site({ official: false, paragliding: true }))).toEqual([]);
	});

	it('names an official site, a discouraged one and one not for paragliders', () => {
		const marked = marks(site({ official: true, discouraged: true, paragliding: false }));

		expect(marked.map((mark) => mark.says)).toEqual([
			'Marked as an official site',
			'Marked as unsafe or discouraged',
			'Marked as not for paragliders'
		]);
		// Only what speaks against flying there is set apart as a warning.
		expect(marked.map((mark) => mark.warns)).toEqual([false, true, false]);
	});
});

describe('whose sites they are', () => {
	it('is a line for the map', () => {
		expect(creditLine([CREDIT], text)).toBe('Sites: Testkarte');
		expect(creditLine([CREDIT, { ...CREDIT, source: 'other', provider: 'Verein' }], text)).toBe(
			'Sites: Testkarte and Verein'
		);
		expect(creditLine([], text)).toBe('');
	});

	it('comes with the date of the data', () => {
		expect(asOf(1_795_028_700, text)).toBe('As of Wed, 18 Nov 2026.');
		expect(asOf(undefined, text)).toBe('');
	});

	it('offers the addresses in a notice as links, without the full stop after one', () => {
		const notice =
			'Available under the Open Database License, https://opendatacommons.org/licenses/odbl/1-0/. Anyone can correct it: https://www.openstreetmap.org/fixthemap';

		expect(withLinks(notice)).toEqual([
			{ text: 'Available under the Open Database License, ' },
			{
				text: 'https://opendatacommons.org/licenses/odbl/1-0/',
				href: 'https://opendatacommons.org/licenses/odbl/1-0/'
			},
			{ text: '. Anyone can correct it: ' },
			{
				text: 'https://www.openstreetmap.org/fixthemap',
				href: 'https://www.openstreetmap.org/fixthemap'
			}
		]);
	});

	it('leaves a notice without addresses, and other kinds of address, as words', () => {
		expect(withLinks('Not certified.')).toEqual([{ text: 'Not certified.' }]);
		expect(withLinks('See javascript:alert(1) or http://example.org')).toEqual([
			{ text: 'See javascript:alert(1) or http://example.org' }
		]);
		expect(withLinks('')).toEqual([]);
	});
});

describe('an empty list of sites', () => {
	const close: SitesState = { ...NO_SITES, close: true };

	it('says how to get one while the map shows too much', () => {
		expect(nothingListed(NO_SITES)).toBe(
			'Zoom the map in on an area, and the sites its source knows there are listed.'
		);
	});

	it('says that the server is being asked', () => {
		expect(nothingListed(close)).toBe('Looking for flying sites…');
	});

	it('says why the server did not answer', () => {
		const problem = 'The server could not be reached for flying sites.';

		expect(nothingListed({ ...close, problem })).toBe(problem);
	});

	it('tells an area without sites from an area the server does not hold', () => {
		expect(nothingListed({ ...close, known: true, isWhole: true })).toBe(
			'The source knows none on this part of the map. A place without a sign may still be a site.'
		);
		expect(nothingListed({ ...close, known: true, isWhole: false })).toBe(
			'The server has no flying sites for this part of the map.'
		);
	});
});
