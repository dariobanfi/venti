import { describe, expect, it } from 'vitest';
import type { CoreClient, FoehnView, TodayAnswer } from '#lib/core/index.ts';
import { NO_FOEHN, TodayModel } from './model.svelte.ts';

// Made-up answers of the core: the model only passes on what it is handed.

const WALLBERG = { lat: 47.6642, lon: 11.7967 };
const HIRSCHBERG = { lat: 47.657, lon: 11.697 };

function listing(...places: { lat: number; lon: number }[]): TodayAnswer {
	return {
		listed: {
			takeoffs: places.map((position, index) => ({
				site: { id: `site-${index}`, name: `Take-off ${index}`, position },
				isKept: false,
				distanceM: 1000
			})),
			keptTitle: 'Yours',
			nearTitle: 'Near',
			keptMore: '',
			credits: []
		},
		nowUnixS: 1_795_028_700,
		nearProblem: undefined
	} as unknown as TodayAnswer;
}

function view(line: boolean): FoehnView {
	return {
		title: 'Foehn',
		todayLine: line
			? { text: 'South foehn likely', level: 'likely', pairId: 'bolzano-innsbruck' }
			: undefined,
		rows: [],
		explanation: '',
		caution: '',
		chartTitle: '',
		unit: 'hPa',
		limits: [],
		empty: '',
		credits: []
	};
}

/** A core that lists take-offs and answers about foehn as the test says. */
class Core {
	takeoffs = listing(WALLBERG, HIRSCHBERG);
	foehnAnswer: () => Promise<FoehnView | undefined> = async () => view(true);
	askedFoehn: { lat: number; lon: number }[][] = [];

	todayTakeoffs = async () => this.takeoffs;
	todayOrders = async () => [];
	todayOutlook = async () => {
		throw new Error('No forecast in this test.');
	};
	foehn = (places: { lat: number; lon: number }[]) => {
		this.askedFoehn.push(places);
		return this.foehnAnswer();
	};
}

/** Lets what the model started without waiting come to its end. */
const settled = () => new Promise((done) => setTimeout(done, 0));

function today(core = new Core()) {
	return { core, model: new TodayModel(core as unknown as CoreClient) };
}

describe('foehn on Today', () => {
	it('is asked for with the places of the take-offs listed, and has the core’s line', async () => {
		const { core, model } = today();
		expect(model.foehn).toEqual(NO_FOEHN);
		model.show(WALLBERG, false);
		await settled();
		expect(core.askedFoehn).toEqual([[WALLBERG, HIRSCHBERG]]);
		expect(model.foehn.phase).toBe('loaded');
		expect(model.foehn.view?.todayLine?.text).toBe('South foehn likely');
	});

	it('has the view and no line when the core gives none', async () => {
		const { core, model } = today();
		core.foehnAnswer = async () => view(false);
		model.show(WALLBERG, false);
		await settled();
		expect(model.foehn.view?.title).toBe('Foehn');
		expect(model.foehn.view?.todayLine).toBeUndefined();
	});

	it('has nothing at all on a site without a server', async () => {
		const { core, model } = today();
		core.foehnAnswer = async () => undefined;
		model.show(WALLBERG, false);
		await settled();
		expect(model.foehn).toEqual(NO_FOEHN);
	});

	it('says why there is no answer, shows no line, and asks again from the view', async () => {
		const { core, model } = today();
		core.foehnAnswer = async () => {
			throw new Error('Venti could not be reached.');
		};
		model.show(WALLBERG, false);
		await settled();
		expect(model.foehn).toEqual({
			phase: 'failed',
			view: undefined,
			problem: 'Venti could not be reached.'
		});
		// The take-offs are there all the same.
		expect(model.state.phase).toBe('loaded');

		core.foehnAnswer = async () => view(true);
		model.foehnAgain();
		// Why stays on the screen until the new answer is there.
		expect(model.foehn).toMatchObject({ phase: 'loading', problem: 'Venti could not be reached.' });
		await settled();
		expect(model.foehn).toMatchObject({ phase: 'loaded', problem: undefined });
		expect(core.askedFoehn).toHaveLength(2);
	});

	it('is asked for again when Today is, and lets go of the line of other take-offs', async () => {
		const { core, model } = today();
		model.show(WALLBERG, false);
		await settled();
		core.takeoffs = listing(HIRSCHBERG);
		core.foehnAnswer = async () => view(false);
		model.show(HIRSCHBERG, true);
		expect(model.foehn).toEqual(NO_FOEHN);
		await settled();
		expect(core.askedFoehn).toEqual([[WALLBERG, HIRSCHBERG], [HIRSCHBERG]]);
		expect(model.foehn.view?.todayLine).toBeUndefined();
	});
});
