/**
 * Countries as the server names them, by their ISO 3166-1 alpha-2 codes:
 * a flag drawn from the code and a name in English.
 *
 * The flag is the pair of regional indicator letters of the code, which
 * the reader's own system draws as a flag; no image of a flag is in the
 * app. The name is the browser's (`Intl.DisplayNames`), in English as the
 * rest of the app, and the code itself where the browser has none.
 */

/** Every code ISO 3166-1 assigns, as of 2026. */
export const COUNTRY_CODES: readonly string[] = (
	'AD AE AF AG AI AL AM AO AQ AR AS AT AU AW AX AZ ' +
	'BA BB BD BE BF BG BH BI BJ BL BM BN BO BQ BR BS BT BV BW BY BZ ' +
	'CA CC CD CF CG CH CI CK CL CM CN CO CR CU CV CW CX CY CZ ' +
	'DE DJ DK DM DO DZ EC EE EG EH ER ES ET FI FJ FK FM FO FR ' +
	'GA GB GD GE GF GG GH GI GL GM GN GP GQ GR GS GT GU GW GY HK HM HN HR HT HU ' +
	'ID IE IL IM IN IO IQ IR IS IT JE JM JO JP KE KG KH KI KM KN KP KR KW KY KZ ' +
	'LA LB LC LI LK LR LS LT LU LV LY ' +
	'MA MC MD ME MF MG MH MK ML MM MN MO MP MQ MR MS MT MU MV MW MX MY MZ ' +
	'NA NC NE NF NG NI NL NO NP NR NU NZ OM PA PE PF PG PH PK PL PM PN PR PS PT PW PY QA ' +
	'RE RO RS RU RW SA SB SC SD SE SG SH SI SJ SK SL SM SN SO SR SS ST SV SX SY SZ ' +
	'TC TD TF TG TH TJ TK TL TM TN TO TR TT TV TW TZ UA UG UM US UY UZ ' +
	'VA VC VE VG VI VN VU WF WS YE YT ZA ZM ZW'
).split(' ');

/** The flag of a country, from its two letters: the reader's system draws it. */
export function flag(code: string): string {
	const letters = code.toUpperCase();
	if (!/^[A-Z]{2}$/.test(letters)) return '';
	return String.fromCodePoint(
		...[...letters].map((letter) => 0x1f1e6 + letter.charCodeAt(0) - 'A'.charCodeAt(0))
	);
}

let names: Intl.DisplayNames | undefined;

/** A country's name in English, or its code where the browser knows none. */
export function countryName(code: string): string {
	try {
		names ??= new Intl.DisplayNames(['en'], { type: 'region', fallback: 'code' });
		return names.of(code.toUpperCase()) ?? code;
	} catch {
		return code;
	}
}

/** Every country, by its name, for the pilot to choose from. */
export function countries(): { code: string; name: string }[] {
	return COUNTRY_CODES.map((code) => ({ code, name: countryName(code) })).sort((one, other) =>
		one.name.localeCompare(other.name, 'en')
	);
}
