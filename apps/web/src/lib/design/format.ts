/**
 * Turns the core's plain numbers into text for the screen.
 *
 * Metric units only for now. Values and units are separate so a screen can
 * set them in different sizes. The iPhone app's `Format.swift` is the same
 * list, built on the system's formatters as this is on the browser's.
 */

export interface Format {
	/** "Sat, 4 Jul 2026" */
	day(unixSeconds: number): string;
	/** "Sat, 4 Jul", for lists already grouped by year. */
	dayInYear(unixSeconds: number): string;
	/** The time of day as the reader's region writes it: "10:30", "00:05", "9:30 AM". */
	time(unixSeconds: number): string;
	/** Hours and minutes the way pilots log airtime: "0:29", "5:42". */
	hoursMinutes(seconds: number): string;
	/** Minutes and seconds of a short stretch: "4:12". */
	minutesSeconds(seconds: number): string;
	/** Kilometres with one decimal. */
	kilometres(metres: number): string;
	/** Whole kilometres, for totals. */
	wholeKilometres(metres: number): string;
	/** Whole metres of a height, as the design pack writes them: "2410", "−35". */
	metres(metres: number): string;
	/** Whole metres with a sign, for a gain or a loss: "+1140", "−135". */
	signedMetres(metres: number): string;
	/** Metres per second with one decimal and always a sign, except for zero. */
	climb(metresPerSecond: number): string;
	/** Kilometres per hour, whole, from metres per second. */
	kilometresPerHour(metresPerSecond: number): string;
	/** A glide ratio with one decimal. */
	glideRatio(ratio: number): string;
	/** A share of a whole as a whole percentage: "24%". */
	percent(share: number): string;
	/** League points, with two decimals. */
	points(points: number): string;
	/** A number as it is, without trailing zeros. */
	number(value: number): string;
	/** A whole number. */
	count(count: number): string;
	/** "1 flight", "12 flights" */
	flights(count: number): string;
	/** The time of day as seconds since midnight, where the reader is. */
	secondsIntoDay(unixSeconds: number): number;
	/** A calendar month in one letter, for the foot of a bar: "J". */
	monthLetter(year: number, month: number): string;
	/** A calendar month in words: "July 2026". */
	monthName(year: number, month: number): string;
	/** Whole degrees Celsius. */
	temperature(celsius: number): string;
	/** Names in a row: "A", "A and B", "A, B and C". */
	list(items: readonly string[]): string;
	/** A day by its weekday and its number in the month: "Fri 20". */
	weekdayAndDay(unixSeconds: number): string;
	/** A day's weekday: "Saturday", or short, "Sat". */
	weekday(unixSeconds: number, short?: boolean): string;
	/** The hour of the day where the reader is, for the head of a column: "9", "14". */
	hourOfDay(unixSeconds: number): string;
	/**
	 * A day of the calendar by its weekday and date: "Saturday, 21 Nov". It
	 * is a date as written at a place, not a moment, so it reads the same
	 * in every time zone.
	 */
	calendarDay(year: number, month: number, day: number): string;
	/**
	 * A place in degrees with four decimals, which is to about ten metres:
	 * "47.6642° N, 11.7967° E".
	 */
	place(lat: number, lon: number): string;
}

/**
 * The locale the app writes dates and times in: English words, because all
 * its text is English, with the habits of the reader's region, so a pilot
 * in Germany gets a 24-hour clock and one in the United States gets AM and
 * PM.
 */
export function displayLocale(preferred: string | undefined): string {
	try {
		const region = new Intl.Locale(preferred ?? 'en-GB').maximize().region;
		return region ? `en-${region}` : 'en-GB';
	} catch {
		return 'en-GB';
	}
}

/**
 * How numbers are written: the English way, with a point before the decimals
 * and a comma between thousands, whatever the reader's region. The core words
 * its own numbers that way ("10.0 km NE"), and a page must not write one
 * number with a comma and the next with a point: for a reader in Germany it
 * did. This changes with the app's language, when the app is translated.
 */
const NUMBERS = 'en';

/**
 * Formatters for a locale and time zone. Screens use {@link format}; tests
 * name both so that their results do not depend on the machine. The locale
 * decides how dates and times are written, not numbers: see {@link NUMBERS}.
 */
export function createFormat(locale: string, timeZone?: string): Format {
	const date = (options: Intl.DateTimeFormatOptions) =>
		new Intl.DateTimeFormat(locale, { timeZone, ...options });
	const number = (digits: number, options: Intl.NumberFormatOptions = {}) =>
		new Intl.NumberFormat(NUMBERS, {
			minimumFractionDigits: digits,
			maximumFractionDigits: digits,
			...options
		});

	const day = date({ weekday: 'short', day: 'numeric', month: 'short', year: 'numeric' });
	// The region's own short time of day, which is what the iPhone asks its
	// system for: "00:05" and "09:30" on a 24-hour clock, "12:05 AM" and
	// "9:30 AM" on a 12-hour one. Asking for the hour as a number and the
	// minute in two digits instead writes "0:05" just after midnight on a
	// 24-hour clock, which no region does and which reads like an airtime.
	const time = date({ timeStyle: 'short' });
	const weekdayAndDay = date({ weekday: 'short', day: 'numeric' });
	const weekdayLong = date({ weekday: 'long' });
	const weekdayShort = date({ weekday: 'short' });
	const clock = date({ hour: 'numeric', minute: 'numeric', second: 'numeric', hourCycle: 'h23' });
	const whole = number(0);
	const tenths = number(1);
	const signedTenths = number(1, { signDisplay: 'exceptZero' });
	// Heights as the design pack writes them: no mark for thousands ("2410
	// m"), and a loss with the true minus sign, as wide as the plus.
	const height = number(0, { useGrouping: false });
	const signedHeight = number(0, { useGrouping: false, signDisplay: 'exceptZero' });
	const trueMinus = (text: string) => text.replace('-', '\u2212');
	const percent = number(0, { style: 'percent' });
	const hundredths = number(2);
	const tenThousandths = number(4);
	const plain = new Intl.NumberFormat(NUMBERS);
	const at = (unixSeconds: number) => new Date(unixSeconds * 1000);
	// The core counts months in UTC. The middle of a month is in that month
	// in every time zone.
	const monthLetter = new Intl.DateTimeFormat(locale, { month: 'narrow', timeZone: 'UTC' });
	const monthName = new Intl.DateTimeFormat(locale, {
		month: 'long',
		year: 'numeric',
		timeZone: 'UTC'
	});
	const midMonth = (year: number, month: number) => new Date(Date.UTC(year, month - 1, 15));
	const calendarDay = new Intl.DateTimeFormat(locale, {
		weekday: 'long',
		day: 'numeric',
		month: 'short',
		year: 'numeric',
		timeZone: 'UTC'
	});
	// Always the English "and": the app's words are English whatever the region.
	const list = new Intl.ListFormat('en-GB', { type: 'conjunction' });
	const secondsIntoDay = (unixSeconds: number) => {
		const parts = Object.fromEntries(
			clock.formatToParts(at(unixSeconds)).map((part) => [part.type, Number(part.value)])
		);
		return parts.hour * 3600 + parts.minute * 60 + parts.second;
	};

	return {
		day: (unixSeconds) => day.format(at(unixSeconds)),
		dayInYear: (unixSeconds) => withoutYear(day.formatToParts(at(unixSeconds))),
		time: (unixSeconds) => time.format(at(unixSeconds)),
		hoursMinutes: (seconds) => {
			const minutes = Math.round(seconds / 60);
			return `${Math.floor(minutes / 60)}:${String(minutes % 60).padStart(2, '0')}`;
		},
		minutesSeconds: (seconds) =>
			`${Math.floor(seconds / 60)}:${String(Math.round(seconds) % 60).padStart(2, '0')}`,
		kilometres: (metres) => tenths.format(metres / 1000),
		wholeKilometres: (metres) => whole.format(metres / 1000),
		metres: (metres) => trueMinus(height.format(metres)),
		signedMetres: (metres) => trueMinus(signedHeight.format(metres)),
		climb: (metresPerSecond) => signedTenths.format(metresPerSecond),
		kilometresPerHour: (metresPerSecond) => whole.format(metresPerSecond * 3.6),
		glideRatio: (ratio) => tenths.format(ratio),
		percent: (share) => percent.format(share),
		points: (points) => hundredths.format(points),
		number: (value) => plain.format(value),
		count: (count) => whole.format(count),
		flights: (count) => (count === 1 ? '1 flight' : `${whole.format(count)} flights`),
		monthLetter: (year, month) => monthLetter.format(midMonth(year, month)),
		monthName: (year, month) => monthName.format(midMonth(year, month)),
		// Minus zero would read "-0".
		temperature: (celsius) => whole.format(Math.round(celsius) || 0),
		list: (items) => list.format(items),
		weekdayAndDay: (unixSeconds) => weekdayAndDay.format(at(unixSeconds)),
		weekday: (unixSeconds, short = false) =>
			(short ? weekdayShort : weekdayLong).format(at(unixSeconds)),
		hourOfDay: (unixSeconds) => String(Math.floor(secondsIntoDay(unixSeconds) / 3600)),
		// Noon of that date in UTC is that date in UTC, whatever the reader's zone.
		place: (lat, lon) =>
			`${tenThousandths.format(Math.abs(lat))}° ${lat < 0 ? 'S' : 'N'}, ` +
			`${tenThousandths.format(Math.abs(lon))}° ${lon < 0 ? 'W' : 'E'}`,
		calendarDay: (year, month, day) =>
			withoutYear(calendarDay.formatToParts(new Date(Date.UTC(year, month - 1, day, 12)))),
		secondsIntoDay
	};
}

/**
 * A formatted day with its year taken out, together with the punctuation
 * that joined the year to the rest. Asking the browser for a day without a
 * year instead gives "Sat 4 Jul" beside "Sat, 4 Jul 2026" in British
 * English: two styles in one app.
 */
function withoutYear(parts: Intl.DateTimeFormatPart[]): string {
	const year = parts.findIndex((part) => part.type === 'year');
	if (year === -1) return parts.map((part) => part.value).join('');
	const joiner = year === parts.length - 1 ? year - 1 : year + 1;
	return parts
		.filter((part, index) => index !== year && !(index === joiner && part.type === 'literal'))
		.map((part) => part.value)
		.join('');
}

/** A compass direction in one or two letters: "N", "SW". */
export function compassPoint(degrees: number): string {
	const points = ['N', 'NE', 'E', 'SE', 'S', 'SW', 'W', 'NW'];
	return points[Math.floor((((degrees % 360) + 360 + 22.5) % 360) / 45)];
}

/**
 * How long ago something was measured: "just now", "25 minutes ago",
 * "3 hours ago", "2 days ago".
 */
export function age(minutes: number): string {
	if (minutes < 1) return 'just now';
	if (minutes === 1) return '1 minute ago';
	if (minutes < 90) return `${minutes} minutes ago`;
	if (minutes < 36 * 60) return `${Math.floor((minutes + 30) / 60)} hours ago`;
	return `${Math.floor((minutes + 720) / 1440)} days ago`;
}

let shared: Format | undefined;

/** The formatters for whoever is looking at the page, in their time zone. */
export function format(): Format {
	shared ??= createFormat(
		displayLocale(typeof navigator === 'undefined' ? undefined : navigator.language)
	);
	return shared;
}
