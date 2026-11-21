import { describe, expect, it } from 'vitest';
import { ariaSort, sortBy, sorted } from './sort';

const rows = [
	{ name: 'B', km: 10 },
	{ name: 'A', km: 30 },
	{ name: 'C', km: 10 }
];
const value = (row: (typeof rows)[number], key: 'name' | 'km') => row[key];

describe('a table whose headings sort', () => {
	it('keeps the order the rows came in until a heading is clicked', () => {
		expect(sorted(rows, undefined, value)).toEqual(rows);
	});

	it('starts with the largest, words from A, and turns round on a second click', () => {
		const byKm = sortBy(undefined, 'km');
		expect(byKm).toEqual({ key: 'km', descending: true });
		// Equal values keep their order.
		expect(sorted(rows, byKm, value).map((row) => row.name)).toEqual(['A', 'B', 'C']);
		const byName = sortBy(byKm, 'name', ['name']);
		expect(sorted(rows, byName, value).map((row) => row.name)).toEqual(['A', 'B', 'C']);
		const back = sortBy(byName, 'name', ['name']);
		expect(sorted(rows, back, value).map((row) => row.name)).toEqual(['C', 'B', 'A']);
		expect(ariaSort(back, 'name')).toBe('descending');
		expect(ariaSort(back, 'km')).toBe('none');
	});
});
