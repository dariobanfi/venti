/**
 * The pilot's own position, from the browser, for as long as the map shows
 * it.
 */

export interface Fix {
	lat: number;
	lon: number;
}

/** Why the position cannot be shown. */
export type PositionProblem = 'denied' | 'not-found' | 'not-offered';

/** A problem in words for the pilot. */
export function problemText(problem: PositionProblem): string {
	switch (problem) {
		case 'denied':
			return 'Venti is not allowed to use your position. You can allow it in your browser’s settings for this site.';
		case 'not-found':
			return 'Your position could not be found. Try again under open sky.';
		case 'not-offered':
			return 'This browser does not give your position to web pages.';
	}
}

/**
 * Asks the browser for the position and keeps asking as it changes. The
 * browser asks the pilot for permission the first time. Returns a function
 * that stops it.
 *
 * `onProblem` is called once and ends the watch: a position that cannot be
 * had is not going to appear a moment later.
 */
export function watchPosition(
	geolocation: Geolocation | undefined,
	onFix: (fix: Fix) => void,
	onProblem: (problem: PositionProblem) => void
): () => void {
	if (!geolocation) {
		onProblem('not-offered');
		return () => {};
	}
	const watch = geolocation.watchPosition(
		(position) => onFix({ lat: position.coords.latitude, lon: position.coords.longitude }),
		(error) => {
			geolocation.clearWatch(watch);
			onProblem(error.code === error.PERMISSION_DENIED ? 'denied' : 'not-found');
		},
		// A phone's satellite fix rather than a guess from its network, no
		// older than a few seconds: the pilot may be moving.
		{ enableHighAccuracy: true, maximumAge: 5_000, timeout: 20_000 }
	);
	return () => geolocation.clearWatch(watch);
}
