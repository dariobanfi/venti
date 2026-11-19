/**
 * The colours that are drawn from code. `docs/design.md` explains the
 * choices. Everything a stylesheet can colour is in `src/app.css` instead.
 */

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
 * How strong the wind is, in four bands of its mean speed.
 *
 * One hue from light to dark: strength is shown as a magnitude, not as a
 * verdict. Whether 25 km/h is too much depends on the pilot, the wing and
 * the place, and the app does not decide that from one number. The iPhone
 * app's `WindBand.swift` holds the same table.
 */
export interface WindBand {
	/** A band holds mean winds below this one, in km/h, and not below the band before it. */
	readonly below: number;
	readonly color: string;
	/** How large the band's arrow is drawn, relative to the second band's. */
	readonly arrowScale: number;
}

/** The bands from the lightest wind to the strongest. */
export const WIND_BANDS: readonly WindBand[] = [
	{ below: 10, color: '#86B6EF', arrowScale: 0.85 },
	{ below: 20, color: '#3987E5', arrowScale: 1 },
	{ below: 30, color: '#1C5CAB', arrowScale: 1.15 },
	{ below: Infinity, color: '#0D366B', arrowScale: 1.3 }
];

/** The position in {@link WIND_BANDS} of the band a mean wind falls in. */
export function windBand(metresPerSecond: number): number {
	const band = WIND_BANDS.findIndex((candidate) => metresPerSecond * 3.6 < candidate.below);
	// Only something that is not a number is below no limit: draw it as the lightest.
	return band === -1 ? 0 : band;
}

/** A station with nothing current to say, as a dot on the map. */
export const OLD_ON_MAP = '#A9A8A2';

/** The numbers under a station's arrow. */
export const LABEL_ON_MAP = '#0B0B0B';
