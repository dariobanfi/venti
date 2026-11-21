import { describe, expect, it } from 'vitest';
import { ordinal } from './words';

describe('ordinal', () => {
	it('names the places of a row of counted flights', () => {
		expect([1, 2, 3, 4, 11, 12, 13, 21, 22, 23, 101].map(ordinal)).toEqual([
			'1st',
			'2nd',
			'3rd',
			'4th',
			'11th',
			'12th',
			'13th',
			'21st',
			'22nd',
			'23rd',
			'101st'
		]);
	});
});
