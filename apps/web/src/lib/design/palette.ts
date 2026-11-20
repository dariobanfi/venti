/**
 * The colours that are drawn from code. `docs/design.md` explains the
 * choices. Everything a stylesheet can colour is in `src/app.css` instead.
 */

import type { WindBand } from '#lib/core/index.ts';

/**
 * The accent as drawn on the map. The map is light in both appearances, so
 * this is the light value whatever the page looks like.
 */
export const ACCENT_ON_MAP = '#6250D6';

/** The near-black of the dots that mark where a track starts and ends. */
export const INK_ON_MAP = '#1F1F1D';

/** The scored route: the same ink, let through a little so the map shows under it. */
export const ROUTE_ON_MAP = 'rgb(31 31 29 / 0.7)';

/**
 * How fast the glider was climbing or sinking, in five bands.
 *
 * A track is coloured by band: blue where it sank, grey where it held its
 * height, orange and red where it climbed. Two hues with a neutral middle
 * stay distinguishable for colour-blind pilots; the palette was checked for
 * that. The iPhone app's `Palette.swift` holds the same table.
 */
export interface ClimbBand {
	/** A band holds the rates below this one, in metres per second, and not below the band before it. */
	readonly below: number;
	readonly color: string;
	/** The band in words, for the legend. */
	readonly label: string;
}

/** The bands from strongest sink to strongest climb. */
export const CLIMB_BANDS: readonly ClimbBand[] = [
	{ below: -2.0, color: '#184F95', label: 'Sinking faster than 2 m/s' },
	{ below: -0.5, color: '#3987E5', label: 'Sinking 0.5 to 2 m/s' },
	{ below: 0.5, color: '#85847F', label: 'Within 0.5 m/s of level' },
	{ below: 2.0, color: '#EB6834', label: 'Climbing 0.5 to 2 m/s' },
	{ below: Infinity, color: '#B8242B', label: 'Climbing faster than 2 m/s' }
];

/** The position in {@link CLIMB_BANDS} of the band a climb rate falls in. */
export function climbBand(metresPerSecond: number): number {
	const band = CLIMB_BANDS.findIndex((candidate) => metresPerSecond < candidate.below);
	// Only something that is not a number is below no limit: draw it as level.
	return band === -1 ? 2 : band;
}

/**
 * What each band of wind looks like. Which band a wind falls in is the
 * core's decision (`WindBand` in `crates/core/src/wind.rs`); the app only
 * gives each band its colour and its arrow.
 *
 * One hue from light to dark: strength is shown as a magnitude, not as a
 * verdict. Whether 25 km/h is too much depends on the pilot, the wing and
 * the place, and the app does not decide that from one number. The iPhone
 * app's `WindBand.swift` holds the same colours.
 */
export interface WindBandLook {
	readonly color: string;
	/** How large the band's arrow is drawn, relative to the second band's. */
	readonly arrowScale: number;
}

/** The bands from the lightest wind to the strongest. */
export const WIND_BANDS: Readonly<Record<WindBand, WindBandLook>> = {
	light: { color: '#86B6EF', arrowScale: 0.85 },
	moderate: { color: '#3987E5', arrowScale: 1 },
	fresh: { color: '#1C5CAB', arrowScale: 1.15 },
	strong: { color: '#0D366B', arrowScale: 1.3 }
};

/** Calm air, as a dot on the map: the palest of the wind's blues. */
export const CALM_ON_MAP = WIND_BANDS.light.color;

/** A station with nothing current to say, as a dot on the map. */
export const OLD_ON_MAP = '#A9A8A2';

/**
 * Another pilot's glider while it flies: a green that is none of the
 * colours of wind, airspace or the pilot's own tracks. The iPhone app's
 * `LiveModel.swift` holds the same two colours.
 */
export const FLYING_ON_MAP = '#157F4B';

/**
 * A glider on the ground, or one nothing was heard of for a while: the grey
 * of a track that neither climbs nor sinks.
 */
export const QUIET_ON_MAP = '#85847F';

/** The numbers under a station's arrow. */
export const LABEL_ON_MAP = '#0B0B0B';
