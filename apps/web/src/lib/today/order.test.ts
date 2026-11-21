import { describe, expect, it } from 'vitest';
import type { OutlookDay, SiteOutlook, SiteView, Thermals } from '#lib/core/index.ts';
import { thermalStep } from '#lib/design/palette.ts';
import type { TodayPlace } from './model.svelte.ts';
import { arranged, firstDay, standings } from './order.ts';

/** A day as the core gives it, with only what the order looks at filled in. */
function day(verdict: OutlookDay['verdict'], thermals?: Thermals): OutlookDay {
	return {
		startUnixS: 0,
		name: 'Today',
		verdict,
		status: '',
		headline: '',
		why: '',
		hours: [],
		windKmh: '',
		gustKmh: '',
		windFrom: '',
		cloudBaseM: undefined,
		thermals,
		thermalWords: '',
		thermalLine: '',
		climbMs: undefined,
		thermalTopM: undefined,
		groundM: undefined,
		model: 'ICON-D2',
		runUnixS: 0,
		note: '',
		nearby: ''
	};
}

function place(name: string, distanceM: number, isKept = false, ...days: OutlookDay[]): TodayPlace {
	const outlook =
		days.length > 0
			? ({
					days,
					model: '',
					runUnixS: 0,
					fetchedUnixS: 0,
					note: '',
					nearbyNote: '',
					layersNote: '',
					credits: []
				} as SiteOutlook)
			: undefined;
	return { site: { id: name, name } as SiteView, isKept, distanceM, outlook, problem: undefined };
}

describe('how the take-offs stand, for the core to order', () => {
	it('says of each take-off on each day what the core’s order asks for', () => {
		const places = [
			place('own', 40_000, true, day('notFlyable')),
			place('near', 900, false, day('flyable', 'big'), day('marginal', 'local')),
			place('none yet', 100)
		];
		expect(standings(places)).toEqual([
			[
				{ id: 'own', isKept: true, distanceM: 40_000, verdict: 'notFlyable', thermals: undefined },
				{ id: 'near', isKept: false, distanceM: 900, verdict: 'flyable', thermals: 'big' },
				{ id: 'none yet', isKept: false, distanceM: 100, verdict: undefined, thermals: undefined }
			],
			[
				// A day a forecast does not reach has no verdict.
				{ id: 'own', isKept: true, distanceM: 40_000, verdict: undefined, thermals: undefined },
				{ id: 'near', isKept: false, distanceM: 900, verdict: 'marginal', thermals: 'local' },
				{ id: 'none yet', isKept: false, distanceM: 100, verdict: undefined, thermals: undefined }
			]
		]);
	});

	it('asks about one day while no forecast has arrived', () => {
		expect(standings([place('a', 1), place('b', 2)])).toHaveLength(1);
	});
});

describe('the core’s order laid over the take-offs', () => {
	const own = place('own', 40_000, true, day('notFlyable'));
	const near = place('near', 900, false, day('flyable', 'big'));
	const late = place('late', 100);

	it('lists them as the core says and names its best with the day', () => {
		const found = arranged([near, own], { ids: ['own', 'near'], best: 'near' }, 0);
		expect(found.listed).toEqual([own, near]);
		expect(found.best).toEqual({ place: near, day: near.outlook!.days[0] });
	});

	it('keeps them as listed until the core has answered, with no best', () => {
		expect(arranged([own, near], undefined, 0)).toEqual({ listed: [own, near], best: undefined });
	});

	it('puts a take-off the order does not name last, and has no best without its day', () => {
		const found = arranged([late, near, own], { ids: ['own', 'near'], best: 'near' }, 1);
		expect(found.listed).toEqual([own, near, late]);
		expect(found.best).toBeUndefined();
	});
});

describe('the day the screen opens on', () => {
	it('is the first with flying hours left at any take-off', () => {
		const hour = {} as OutlookDay['hours'][number];
		const over = day('notFlyable');
		const tomorrow = { ...day('flyable'), hours: [hour] };
		expect(firstDay([place('a', 1, false, over, tomorrow), place('b', 2)])).toBe(1);
		expect(firstDay([place('a', 1, false, over)])).toBe(0);
	});
});

describe('the thermal scale', () => {
	it('has six steps at the pack’s breaks of 0.5, 1, 1.5, 2 and 3 m/s', () => {
		expect([0, 0.4, 0.5, 0.9, 1, 1.5, 1.9, 2, 2.9, 3, 5].map(thermalStep)).toEqual([
			1, 1, 2, 2, 3, 4, 4, 5, 5, 6, 6
		]);
	});
});
