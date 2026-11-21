/**
 * The colours that are drawn from code: on the map, and in a chart that
 * takes a colour as a value. They are the design pack's, read from its
 * tokens at build time (`design/tokens/tokens.json`); `design/DESIGN.md`
 * says what each scale is for. Everything a stylesheet can colour is in
 * `src/app.css` instead, over the same tokens.
 *
 * The map is light in both appearances, so what is drawn on it takes the
 * light theme's values whatever the page looks like.
 */

import tokens from '../../../../../design/tokens/tokens.json';
import type { WindBand } from '#lib/core/index.ts';

/** A colour of the pack's light theme, by its group and name. */
function light(group: Record<string, { $value: string }>, name: string): string {
	return group[name].$value;
}

/** One step of one of the pack's data scales, in its light values. */
function step(scale: 'vario' | 'wind', index: number): string {
	return light(tokens.scale[scale].light, String(index));
}

/** The pilot's own flights on the overview map, and the marker on a flight: the pack's primary. */
export const ACCENT_ON_MAP = light(tokens.theme.light, 'primary');

/** The dark ink of the dots that mark where a track starts and ends: the track's casing. */
export const INK_ON_MAP = light(tokens.map.light, 'track-casing');

/** The pale edge around what is drawn on the map: the halo of its labels. */
export const PAPER_ON_MAP = light(tokens.map.light, 'label-halo');

/**
 * A second flight laid beside the one on screen: the pack's fourth series
 * colour, an ochre that neither the vario scale nor the wind has.
 */
export const OTHER_ON_MAP = light(tokens.series.light, '4');

/** The scored route: the same ink, let through a little so the map shows under it. */
export const ROUTE_ON_MAP = `${INK_ON_MAP}b3`;

/**
 * How fast the glider was climbing or sinking: the pack's vario scale,
 * seven steps from teal for sink through grey to coral for lift ("cool is
 * down, warm is up"), at the breaks the pack gives.
 */
export interface ClimbBand {
	/** A band holds the rates below this one, in metres per second, and not below the band before it. */
	readonly below: number;
	readonly color: string;
	/** The band in words, for the legend. */
	readonly label: string;
}

const VARIO_BREAKS: readonly number[] = tokens.scale.vario.$extensions.venti.breaks;

/** The bands from strongest sink to strongest climb. */
export const CLIMB_BANDS: readonly ClimbBand[] = [...VARIO_BREAKS, Infinity].map((below, index) => {
	const above = VARIO_BREAKS[index - 1];
	const rate = (value: number) => `${Math.abs(value)}`;
	let label: string;
	if (index === 0) label = `Sinking faster than ${rate(below)} m/s`;
	else if (below === Infinity) label = `Climbing faster than ${rate(above)} m/s`;
	else if (above < 0 && below > 0) label = `Within ${rate(below)} m/s of level`;
	else if (below <= 0) label = `Sinking ${rate(below)} to ${rate(above)} m/s`;
	else label = `Climbing ${rate(above)} to ${rate(below)} m/s`;
	return { below, color: step('vario', index + 1), label };
});

/** The band of level flight, neither climb nor sink. */
export const LEVEL_BAND = CLIMB_BANDS.findIndex((band) => band.below > 0);

/** The position in {@link CLIMB_BANDS} of the band a climb rate falls in. */
export function climbBand(metresPerSecond: number): number {
	const band = CLIMB_BANDS.findIndex((candidate) => metresPerSecond < candidate.below);
	// Only something that is not a number is below no limit: draw it as level.
	return band === -1 ? LEVEL_BAND : band;
}

/**
 * What each band of wind looks like. Which band a wind falls in is the
 * core's decision (`WindBand` in `crates/core/src/wind.rs`); the app only
 * gives each band its colour and its arrow.
 *
 * The pack's wind scale, one purple from light to dark: strength is shown as
 * a magnitude, not as a verdict. The core's four bands (below 10 km/h, to
 * 20, to 30, and above) take the step of the pack's seven that holds their
 * middle: 5 to 10, 15 to 20, 25 to 35, and 35 and above.
 */
export interface WindBandLook {
	readonly color: string;
	/** How large the band's arrow is drawn, relative to the second band's. */
	readonly arrowScale: number;
}

/** The bands from the lightest wind to the strongest. */
export const WIND_BANDS: Readonly<Record<WindBand, WindBandLook>> = {
	light: { color: step('wind', 3), arrowScale: 0.85 },
	moderate: { color: step('wind', 4), arrowScale: 1 },
	fresh: { color: step('wind', 6), arrowScale: 1.15 },
	strong: { color: step('wind', 7), arrowScale: 1.3 }
};

/** Calm air, as a dot on the map: the palest of the wind's steps. */
export const CALM_ON_MAP = step('wind', 2);

/** A station with nothing current to say, as a dot on the map. */
export const OLD_ON_MAP = step('vario', 4);

/**
 * Another pilot's glider while it flies: a green that is none of the
 * colours of wind, airspace or the pilot's own tracks. The iPhone app's
 * `LiveModel.swift` holds the same two colours.
 */
export const FLYING_ON_MAP = light(tokens.theme.light, 'good');

/**
 * A glider on the ground, or one nothing was heard of for a while: the grey
 * of a track that neither climbs nor sinks.
 */
export const QUIET_ON_MAP = step('vario', 4);

/** The numbers under a station's arrow. */
export const LABEL_ON_MAP = light(tokens.map.light, 'label');

const THERMAL_BREAKS: readonly number[] = tokens.scale.thermal.$extensions.venti.breaks;

/**
 * The step of the pack's thermal scale, its scale for a forecast climb, that
 * a climb in metres per second falls in: 1 for less than the first break
 * (0.5 m/s) to 6 for the last (3 m/s) and more, as the pack's `scaleColor`
 * picks it. The step is a number so that a stylesheet can wear it as
 * `--thermal-1` to `--thermal-6`, which follow the theme.
 */
export function thermalStep(metresPerSecond: number): number {
	const index = THERMAL_BREAKS.findIndex((limit) => metresPerSecond < limit);
	return (index === -1 ? THERMAL_BREAKS.length : index) + 1;
}
