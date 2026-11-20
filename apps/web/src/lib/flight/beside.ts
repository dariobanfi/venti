/**
 * A second flight laid beside the one on screen: which one it is, as the
 * address names it, and where its glider was at a moment of the first.
 *
 * How the two lie to each other in time and what there is to compare is
 * the core's (`compare_flights`); this file only looks the other glider up
 * in its own copy of the track, as the iPhone's `ComparedFlight` does.
 */

import type { FlightRef } from '#lib/core/community-source.ts';
import type { FlightComparison, FlightView, TrackPoint } from '#lib/core/index.ts';
import type { Format } from '#lib/design/format.ts';
import { nearestIndex } from './chart';

/** The flight laid beside, as a screen shows it. */
export interface Beside {
	flight: FlightView;
	/** Whose it is: "Mara Muster", "Your flight of Sat, 15 Aug 2026". */
	name: string;
	comparison: FlightComparison;
}

/** The flight an address names to lay beside: `own:<id>` or `public:<id>`. */
export function besideIn(query: Pick<URLSearchParams, 'get'>): FlightRef | undefined {
	const named = /^(own|public):(.+)$/.exec(query.get('beside') ?? '');
	if (!named) return undefined;
	return named[1] === 'own' ? { own: named[2] } : { public: named[2] };
}

/** The value an address names it by: `own:<id>` or `public:<id>`. */
export function besideValue(ref: FlightRef): string {
	return 'own' in ref ? `own:${ref.own}` : `public:${ref.public}`;
}

/**
 * Where the other glider was at a moment of the flight on screen, given in
 * seconds since its first fix. Nothing when it was not in the air then.
 */
export function otherAt(beside: Beside, seconds: number): TrackPoint | undefined {
	const theirs = seconds - beside.comparison.offsetS;
	const { track, durationS } = beside.flight;
	if (theirs < 0 || theirs > durationS || track.length === 0) return undefined;
	return track[nearestIndex(track, theirs)];
}

/** Metres over the ground between two places, along the earth's surface. */
export function metresApart(one: TrackPoint, other: TrackPoint): number {
	const radians = Math.PI / 180;
	const [lat1, lat2] = [one.lat * radians, other.lat * radians];
	const dLat = lat2 - lat1;
	const dLon = (other.lon - one.lon) * radians;
	const a = Math.sin(dLat / 2) ** 2 + Math.cos(lat1) * Math.cos(lat2) * Math.sin(dLon / 2) ** 2;
	return 2 * 6_371_000 * Math.asin(Math.min(1, Math.sqrt(a)));
}

/**
 * What to say of the other glider at a moment: how high and how far away
 * ("1,840 m · 320 m away"), or that it was not in the air then.
 */
export function otherInWords(beside: Beside, point: TrackPoint, text: Format): string {
	const other = otherAt(beside, point.seconds);
	if (!other) return 'Not in the air then';
	const apart = metresApart(point, other);
	const away =
		apart < 1000
			? `${text.metres(Math.round(apart / 10) * 10)} m away`
			: `${text.kilometres(apart)} km away`;
	return `${text.metres(other.altitudeM)} m · ${away}`;
}
