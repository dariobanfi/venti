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
	/** "10:30" */
	time(unixSeconds: number): string;
	/** Hours and minutes the way pilots log airtime: "0:29", "5:42". */
	hoursMinutes(seconds: number): string;
	/** Minutes and seconds of a short stretch: "4:12". */
	minutesSeconds(seconds: number): string;
	/** Kilometres with one decimal. */
	kilometres(metres: number): string;
	/** Whole kilometres, for totals. */
	wholeKilometres(metres: number): string;
	/** Whole metres. */
	metres(metres: number): string;
	/** Whole metres with a sign, for a gain or a loss. */
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
}

/**
 * The locale the app writes in: English words, because all its text is
 * English, with the habits of the reader's region, so a pilot in Germany
 * gets a 24-hour clock and one in the United States gets AM and PM.
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
 * Formatters for a locale and time zone. Screens use {@link format}; tests
 * name both so that their results do not depend on the machine.
 */
export function createFormat(locale: string, timeZone?: string): Format {
	const date = (options: Intl.DateTimeFormatOptions) =>
		new Intl.DateTimeFormat(locale, { timeZone, ...options });
	const number = (digits: number, options: Intl.NumberFormatOptions = {}) =>
		new Intl.NumberFormat(locale, {
			minimumFractionDigits: digits,
			maximumFractionDigits: digits,
			...options
		});

	const day = date({ weekday: 'short', day: 'numeric', month: 'short', year: 'numeric' });
	const time = date({ hour: 'numeric', minute: '2-digit' });
	const clock = date({ hour: 'numeric', minute: 'numeric', second: 'numeric', hourCycle: 'h23' });
	const whole = number(0);
	const tenths = number(1);
	const signedTenths = number(1, { signDisplay: 'exceptZero' });
	const signedWhole = number(0, { signDisplay: 'exceptZero' });
	const percent = number(0, { style: 'percent' });
	const hundredths = number(2);
	const plain = new Intl.NumberFormat(locale);
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
		metres: (metres) => whole.format(metres),
		signedMetres: (metres) => signedWhole.format(metres),
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
		secondsIntoDay: (unixSeconds) => {
			const parts = Object.fromEntries(
				clock.formatToParts(at(unixSeconds)).map((part) => [part.type, Number(part.value)])
			);
			return parts.hour * 3600 + parts.minute * 60 + parts.second;
		}
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

let shared: Format | undefined;

/** The formatters for whoever is looking at the page, in their time zone. */
export function format(): Format {
	shared ??= createFormat(
		displayLocale(typeof navigator === 'undefined' ? undefined : navigator.language)
	);
	return shared;
}
