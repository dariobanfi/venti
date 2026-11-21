import { describe, expect, it } from 'vitest';
import type { OutlookDay, SiteOutlook, SiteView, Thermals } from '#lib/core/index.ts';
import { thermalStep } from '#lib/design/palette.ts';
import type { TodayPlace } from './model.svelte.ts';
import { bestOn, inOrder, rank } from './order.ts';

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
		thermalTopM: undefined
	};
}

function place(name: string, distanceM: number, today?: OutlookDay): TodayPlace {
	const outlook = today
		? ({
				days: [today],
				model: '',
				runUnixS: 0,
				fetchedUnixS: 0,
				note: '',
				credits: []
			} as SiteOutlook)
		: undefined;
	return { site: { id: name, name } as SiteView, distanceM, outlook, problem: undefined };
}

describe('the order of the take-offs', () => {
	it('puts flyable days by their thermals first, then marginal, not flyable and none', () => {
		const places = [
			place('none yet', 100),
			place('not flyable', 200, day('notFlyable')),
			place('marginal', 300, day('marginal', 'local')),
			place('weak', 400, day('flyable', 'weak')),
			place('no estimate', 450, day('flyable')),
			place('local', 500, day('flyable', 'local')),
			place('cross-country', 600, day('flyable', 'crossCountry')),
			place('big', 700, day('flyable', 'big'))
		];
		expect(inOrder(places, 0).map((one) => one.site.name)).toEqual([
			'big',
			'cross-country',
			'local',
			'weak',
			'no estimate',
			'marginal',
			'not flyable',
			'none yet'
		]);
		expect(bestOn(places, 0)?.place.site.name).toBe('big');
	});

	it('takes the nearer of two equals, and a day the forecast does not reach as none', () => {
		const far = place('far', 900, day('flyable', 'crossCountry'));
		const near = place('near', 100, day('flyable', 'crossCountry'));
		expect(inOrder([far, near], 0).map((one) => one.site.name)).toEqual(['near', 'far']);
		expect(rank(undefined)).toBe(6);
		expect(bestOn([far, near], 1)).toBeUndefined();
	});
});

describe('the thermal scale', () => {
	it('has six steps at the pack’s breaks of 0.5, 1, 1.5, 2 and 3 m/s', () => {
		expect([0, 0.4, 0.5, 0.9, 1, 1.5, 1.9, 2, 2.9, 3, 5].map(thermalStep)).toEqual([
			1, 1, 2, 2, 3, 4, 4, 5, 5, 6, 6
		]);
	});
});
