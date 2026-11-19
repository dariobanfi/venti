import { describe, expect, it } from 'vitest';
import { type Fix, type PositionProblem, problemText, watchPosition } from './position';

/** A browser's position service that the test drives by hand. */
class FakeGeolocation {
	watching = false;
	#success: PositionCallback = () => {};
	#failure: PositionErrorCallback = () => {};

	watchPosition(success: PositionCallback, failure: PositionErrorCallback): number {
		this.watching = true;
		this.#success = success;
		this.#failure = failure;
		return 7;
	}

	clearWatch(watch: number): void {
		if (watch === 7) this.watching = false;
	}

	find(latitude: number, longitude: number): void {
		this.#success({ coords: { latitude, longitude } } as GeolocationPosition);
	}

	fail(code: number): void {
		this.#failure({ code, PERMISSION_DENIED: 1 } as GeolocationPositionError);
	}
}

function watch(geolocation: FakeGeolocation | undefined) {
	const fixes: Fix[] = [];
	const problems: PositionProblem[] = [];
	const stop = watchPosition(
		geolocation as unknown as Geolocation | undefined,
		(fix) => fixes.push(fix),
		(problem) => problems.push(problem)
	);
	return { fixes, problems, stop };
}

describe('the pilot’s position', () => {
	it('is reported every time the browser finds it, until the watch is stopped', () => {
		const geolocation = new FakeGeolocation();
		const { fixes, stop } = watch(geolocation);

		geolocation.find(47.69, 11.76);
		geolocation.find(47.7, 11.77);
		expect(fixes).toEqual([
			{ lat: 47.69, lon: 11.76 },
			{ lat: 47.7, lon: 11.77 }
		]);

		stop();
		expect(geolocation.watching).toBe(false);
	});

	it('is a problem with its own words when the pilot said no', () => {
		const geolocation = new FakeGeolocation();
		const { problems } = watch(geolocation);

		geolocation.fail(1);

		expect(problems).toEqual(['denied']);
		expect(geolocation.watching).toBe(false);
		expect(problemText('denied')).toContain('not allowed');
	});

	it('is a different problem when it simply cannot be found', () => {
		const geolocation = new FakeGeolocation();
		const { problems } = watch(geolocation);

		geolocation.fail(2);

		expect(problems).toEqual(['not-found']);
		expect(problemText('not-found')).toContain('could not be found');
	});

	it('is a problem at once in a browser that offers no positions', () => {
		const { problems, stop } = watch(undefined);

		expect(problems).toEqual(['not-offered']);
		expect(problemText('not-offered')).toContain('does not give');
		stop();
	});
});
