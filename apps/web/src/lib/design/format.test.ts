import { describe, expect, it } from 'vitest';
import { age, compassPoint, createFormat, displayLocale } from './format';

/** The sample flight's take-off: 4 July 2026, 10:30:00 UTC. */
const TAKE_OFF = 1783161000;

describe('dates and times', () => {
	const british = createFormat('en-GB', 'Europe/Berlin');
	const american = createFormat('en-US', 'America/Denver');

	it('writes a day with its weekday, and without the year inside a year', () => {
		expect(british.day(TAKE_OFF)).toBe('Sat, 4 Jul 2026');
		expect(british.dayInYear(TAKE_OFF)).toBe('Sat, 4 Jul');
		expect(american.day(TAKE_OFF)).toBe('Sat, Jul 4, 2026');
		expect(american.dayInYear(TAKE_OFF)).toBe('Sat, Jul 4');
	});

	it('writes the time of day where the reader is, in the way of their region', () => {
		expect(british.time(TAKE_OFF)).toBe('12:30');
		// U+202F, a narrow no-break space, is what browsers put before AM and PM.
		expect(american.time(TAKE_OFF).replace(/\s/u, ' ')).toBe('4:30 AM');
	});

	it('writes the hours before ten in two digits on a 24-hour clock, and midnight as 00', () => {
		// Five past midnight in Berlin on 20 November 2026, and half past nine
		// that morning. The browser tests once read "0:05" here, and passed or
		// failed by the hour they ran at.
		const afterMidnight = Date.UTC(2026, 10, 19, 23, 5) / 1000;
		const morning = Date.UTC(2026, 10, 20, 8, 30) / 1000;

		for (const region of ['en-GB', 'en-DE', 'en-AT', 'en-CH', 'en-IT', 'en-FR']) {
			const text = createFormat(region, 'Europe/Berlin');
			expect(text.time(afterMidnight), region).toBe('00:05');
			expect(text.time(morning), region).toBe('09:30');
			expect(`${text.dayInYear(afterMidnight)}, ${text.time(afterMidnight)}`, region).toBe(
				'Fri, 20 Nov, 00:05'
			);
		}

		// A 12-hour clock has no hour to fill up: the region writes "9:30 AM".
		const denver = (unixSeconds: number) => american.time(unixSeconds).replace(/\s/u, ' ');
		expect(denver(Date.UTC(2026, 10, 20, 7, 5) / 1000)).toBe('12:05 AM');
		expect(denver(Date.UTC(2026, 10, 20, 16, 30) / 1000)).toBe('9:30 AM');
	});

	it('names a month in a letter and in words', () => {
		expect(british.monthLetter(2026, 7)).toBe('J');
		expect(british.monthLetter(2025, 12)).toBe('D');
		expect(british.monthName(2026, 7)).toBe('July 2026');
		// A month is the same month wherever the reader is.
		expect(american.monthName(2026, 1)).toBe('January 2026');
	});

	it('writes a day of the calendar with its weekday in full, the same in every time zone', () => {
		// A date as written at a place, not a moment: no zone moves it.
		expect(british.calendarDay(2026, 11, 21)).toBe('Saturday, 21 Nov');
		expect(american.calendarDay(2026, 11, 21)).toBe('Saturday, Nov 21');
		expect(createFormat('en-NZ', 'Pacific/Auckland').calendarDay(2027, 1, 1)).toBe('Friday, 1 Jan');
		expect(createFormat('en-US', 'Pacific/Honolulu').calendarDay(2027, 1, 1)).toBe('Friday, Jan 1');
	});

	it('writes a place in degrees to about ten metres', () => {
		expect(british.place(47.6642, 11.7967)).toBe('47.6642° N, 11.7967° E');
		expect(british.place(-33.86785, -70.5)).toBe('33.8679° S, 70.5000° W');
	});

	it('knows how far into the reader’s day a moment is', () => {
		expect(british.secondsIntoDay(TAKE_OFF)).toBe(12 * 3600 + 30 * 60);
		expect(american.secondsIntoDay(TAKE_OFF + 7)).toBe(4 * 3600 + 30 * 60 + 7);
	});
});

describe('numbers', () => {
	const format = createFormat('en-GB', 'UTC');

	it('writes airtime as hours and minutes, to the nearest minute', () => {
		expect(format.hoursMinutes(0)).toBe('0:00');
		expect(format.hoursMinutes(1744)).toBe('0:29');
		expect(format.hoursMinutes(1771)).toBe('0:30');
		expect(format.hoursMinutes(5 * 3600 + 42 * 60)).toBe('5:42');
		expect(format.hoursMinutes(3599)).toBe('1:00');
		expect(format.hoursMinutes(123 * 3600)).toBe('123:00');
	});

	it('writes a forecast climb with one decimal and no sign', () => {
		expect(format.forecastClimb(1.8)).toBe('1.8');
		expect(format.forecastClimb(0)).toBe('0.0');
	});

	it('writes distances in kilometres', () => {
		expect(format.kilometres(18402.3)).toBe('18.4');
		expect(format.kilometres(1234567)).toBe('1,234.6');
		expect(format.wholeKilometres(18402.3)).toBe('18');
	});

	it('writes heights in whole metres without a mark for thousands, as the design pack does', () => {
		expect(format.metres(2117)).toBe('2117');
		expect(format.metres(-35)).toBe('\u221235');
		expect(format.metres(786.4)).toBe('786');
	});

	it('writes a change in temperature per 100 m with two decimals and a sign', () => {
		expect(format.perHundredMetres(-0.654)).toBe('−0.65');
		expect(format.perHundredMetres(0.2)).toBe('+0.20');
		expect(format.perHundredMetres(0)).toBe('0.00');
	});

	it('gives a climb rate a sign unless it is zero', () => {
		expect(format.climb(2.4)).toBe('+2.4');
		expect(format.climb(-1.94)).toBe('\u22121.9');
		expect(format.climb(0)).toBe('0.0');
		expect(format.climb(0.04)).toBe('0.0');
	});

	it('writes a short stretch as minutes and seconds', () => {
		expect(format.minutesSeconds(252)).toBe('4:12');
		expect(format.minutesSeconds(9)).toBe('0:09');
		expect(format.minutesSeconds(3852)).toBe('64:12');
	});

	it('gives a height gained or lost a sign', () => {
		expect(format.signedMetres(853.2)).toBe('+853');
		expect(format.signedMetres(-1240)).toBe('\u22121240');
		expect(format.signedMetres(0)).toBe('0');
	});

	it('writes speeds in whole kilometres per hour', () => {
		expect(format.kilometresPerHour(10)).toBe('36');
		expect(format.kilometresPerHour(3.4)).toBe('12');
	});

	it('writes a glide ratio with one decimal and a share as a percentage', () => {
		expect(format.glideRatio(8.26)).toBe('8.3');
		expect(format.percent(0.243)).toBe('24%');
		expect(format.percent(1)).toBe('100%');
	});

	it('writes league points with two decimals and a factor as it is', () => {
		expect(format.points(12.0)).toBe('12.00');
		expect(format.points(1234.567)).toBe('1,234.57');
		expect(format.number(1)).toBe('1');
		expect(format.number(1.4)).toBe('1.4');
	});

	it('counts flights in words', () => {
		expect(format.flights(0)).toBe('0 flights');
		expect(format.flights(1)).toBe('1 flight');
		expect(format.flights(1250)).toBe('1,250 flights');
		expect(format.count(1250)).toBe('1,250');
	});
});

describe('a compass direction', () => {
	it('is the nearest of eight points', () => {
		expect(compassPoint(0)).toBe('N');
		expect(compassPoint(22)).toBe('N');
		expect(compassPoint(23)).toBe('NE');
		expect(compassPoint(90)).toBe('E');
		expect(compassPoint(225)).toBe('SW');
		expect(compassPoint(337.4)).toBe('NW');
		expect(compassPoint(337.6)).toBe('N');
		expect(compassPoint(359.9)).toBe('N');
	});

	it('wraps around for directions outside the circle', () => {
		expect(compassPoint(-90)).toBe('W');
		expect(compassPoint(450)).toBe('E');
	});
});

describe('the locale the app writes in', () => {
	it('is English with the reader’s regional habits', () => {
		expect(displayLocale('de-DE')).toBe('en-DE');
		expect(displayLocale('en-US')).toBe('en-US');
		expect(displayLocale('fr')).toBe('en-FR');
		expect(displayLocale('it-CH')).toBe('en-CH');
	});

	it('falls back to British English when the browser says nothing usable', () => {
		expect(displayLocale(undefined)).toBe('en-GB');
		expect(displayLocale('not a locale')).toBe('en-GB');
	});

	it('gives a pilot in Germany a 24-hour clock', () => {
		expect(createFormat(displayLocale('de-DE'), 'Europe/Berlin').time(TAKE_OFF + 4 * 3600)).toBe(
			'16:30'
		);
	});
});

describe('what the wind screens write', () => {
	const text = createFormat('en-GB', 'Europe/Berlin');

	it('says how long ago something was measured', () => {
		expect([0, 1, 25, 89].map(age)).toEqual([
			'just now',
			'1 minute ago',
			'25 minutes ago',
			'89 minutes ago'
		]);
	});

	it('counts in hours from an hour and a half, and in days from a day and a half', () => {
		expect([90, 185, 36 * 60 - 1].map(age)).toEqual(['2 hours ago', '3 hours ago', '36 hours ago']);
		expect([36 * 60, 3 * 1440].map(age)).toEqual(['2 days ago', '3 days ago']);
	});

	it('writes temperatures in whole degrees, and never minus zero', () => {
		expect(text.temperature(-7.1)).toBe('-7');
		expect(text.temperature(3.5)).toBe('4');
		expect(text.temperature(-0.2)).toBe('0');
	});

	it('writes numbers the English way, whatever the reader’s region', () => {
		// The core words its own numbers with a point: "10.0 km NE". A reader
		// in Germany once got "2,5 km" beside it.
		for (const region of ['en-GB', 'en-DE', 'en-AT', 'en-CH', 'en-IT', 'en-FR', 'en-US']) {
			const text = createFormat(region, 'Europe/Berlin');
			expect(text.kilometres(2500), region).toBe('2.5');
			expect(text.metres(3100), region).toBe('3100');
			expect(text.place(47.6642, 11.7967), region).toBe('47.6642° N, 11.7967° E');
		}
	});

	it('joins names with an English “and”, whatever the reader’s region', () => {
		const german = createFormat('en-DE', 'Europe/Berlin');

		expect(german.list(['Deutscher Wetterdienst'])).toBe('Deutscher Wetterdienst');
		expect(german.list(['Deutscher Wetterdienst', 'GeoSphere Austria'])).toBe(
			'Deutscher Wetterdienst and GeoSphere Austria'
		);
		expect(german.list(['A', 'B', 'C'])).toBe('A, B and C');
	});
});
