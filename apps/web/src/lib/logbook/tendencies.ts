/**
 * The screen "How you fly": what the pilot's flights on the account say
 * about how they fly. Every word of an observation is the core's
 * (`ownTendencies`, over the figures the server keeps of each flight);
 * this holds the screen's own few words and how wide a share's bar is
 * drawn.
 */

/** While the server's figures are asked for. */
export const COUNTING = 'Counting your flights';

/** With nothing to say yet: the line `docs/design.md` gives. */
export const NOTHING_YET =
	'Nothing to say yet. The observations begin once the logbook holds ten thermals.';

/** The words of the link to the flight the core names. */
export const SHOWS_IT_MOST = 'The flight that shows it most';

/** How much of its track a share's bar fills, in percent: the core's share, kept within 0 and 100. */
export function barPercent(share: number): number {
	if (!Number.isFinite(share)) return 0;
	return Math.round(Math.min(Math.max(share, 0), 1) * 1000) / 10;
}
