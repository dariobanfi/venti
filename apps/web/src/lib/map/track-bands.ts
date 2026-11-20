/**
 * Cuts a track into runs of one climb band each, for colouring. The iPhone
 * app's `TrackBands.swift` does the same.
 */

import { climbBand } from '#lib/design/palette.ts';

export interface Run {
	/** The band's position in `CLIMB_BANDS`. */
	band: number;
	/**
	 * The first and last position in the track. A run ends on the point the
	 * next one starts on, so the coloured pieces join without a gap.
	 */
	from: number;
	to: number;
}

/**
 * The step from one point to the next takes the band of the point it
 * arrives at. A track of fewer than two points has no runs.
 */
export function runs(track: readonly { climbMs: number }[]): Run[] {
	if (track.length < 2) return [];

	const runs: Run[] = [];
	let from = 0;
	let band = climbBand(track[1].climbMs);
	for (let index = 2; index < track.length; index += 1) {
		const next = climbBand(track[index].climbMs);
		if (next !== band) {
			runs.push({ band, from, to: index - 1 });
			from = index - 1;
			band = next;
		}
	}
	runs.push({ band, from, to: track.length - 1 });
	return runs;
}
