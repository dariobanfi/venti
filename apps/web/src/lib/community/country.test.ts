import { describe, expect, it } from 'vitest';
import { countries, COUNTRY_CODES, countryName, flag } from './country';

describe('countries', () => {
	it('are the 249 codes ISO 3166-1 assigns, each once', () => {
		expect(COUNTRY_CODES).toHaveLength(249);
		expect(new Set(COUNTRY_CODES).size).toBe(249);
		expect(COUNTRY_CODES.every((code) => /^[A-Z]{2}$/.test(code))).toBe(true);
	});

	it('have a flag drawn from their two letters', () => {
		expect(flag('DE')).toBe('\u{1F1E9}\u{1F1EA}');
		expect(flag('at')).toBe('\u{1F1E6}\u{1F1F9}');
		expect(flag('D')).toBe('');
	});

	it('are named in English and listed by their names', () => {
		expect(countryName('CH')).toBe('Switzerland');
		const listed = countries();
		expect(listed).toHaveLength(249);
		expect(listed.find((country) => country.code === 'DE')?.name).toBe('Germany');
		const named = listed.map((country) => country.name);
		expect(named).toEqual([...named].sort((one, other) => one.localeCompare(other, 'en')));
	});
});
