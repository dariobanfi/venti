import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type {
	AgreementOutcome,
	AgreementPage,
	AgreementRead,
	AgreementSaid,
	AgreementState
} from '#lib/core/index.ts';
import { type AgreementCore, ConnectModel } from './model.svelte.ts';

// Made-up answers of the core: the page only shows what it is handed, so
// the words here are stand-ins and no test of the core's.

/** 18 November 2026, 19:05 UTC. */
const NOW = Date.UTC(2026, 10, 18, 19, 5) / 1000;
const ID = `venti_oar_${'0'.repeat(52)}`;
const BACK = 'https://assistant.example/oauth/callback?code=venti_oac_secret&state=s&iss=venti';

function closed(state: AgreementState, title: string, message: string): AgreementPage {
	return {
		state,
		title,
		message,
		mayReadLabel: '',
		mayRead: [],
		rows: [],
		warning: undefined,
		account: undefined,
		withdraw: '',
		endsUnixS: 0
	};
}

const ENDED = closed('ended', 'Ended', 'Start again.');

function waiting(state: 'asking' | 'signInFirst', endsUnixS = NOW + 900): AgreementPage {
	return {
		state,
		title: 'Let it read?',
		message: 'A program asks.',
		mayReadLabel: 'It may read',
		mayRead: ['Your flights.'],
		rows: [{ label: 'It may not', text: 'Change anything.' }],
		warning: undefined,
		account: state === 'asking' ? 'You answer as @anna.' : undefined,
		withdraw: 'You can withdraw this.',
		endsUnixS
	};
}

const SAID: AgreementSaid = {
	words: {
		loading: 'Loading…',
		allow: 'Allow',
		refuse: 'No',
		answering: 'One moment…',
		signIn: 'Sign in',
		retry: 'Try again'
	},
	ended: ENDED
};

/** A core that answers what the test says, and notes what it was asked. */
class Core implements AgreementCore {
	read: AgreementRead = { page: waiting('asking'), nowUnixS: NOW };
	outcome: AgreementOutcome = { redirectTo: BACK, state: 'answered', title: '', message: '' };
	asked: string[] = [];
	answered: { id: string; agree: boolean }[] = [];
	/** Holds the next answer back until called. */
	hold: Promise<void> | undefined;

	async agreementSaid() {
		return SAID;
	}

	async agreementRead(id: string) {
		this.asked.push(id);
		return this.read;
	}

	async agreementAnswer(id: string, agree: boolean) {
		this.answered.push({ id, agree });
		await this.hold;
		return this.outcome;
	}
}

function opened(core = new Core()) {
	const left: string[] = [];
	let signedOut = 0;
	const model = new ConnectModel(
		core,
		ID,
		() => (signedOut += 1),
		(to) => left.push(to)
	);
	return { core, model, left, signedOut: () => signedOut };
}

beforeEach(() => void vi.useFakeTimers());
afterEach(() => void vi.useRealTimers());

describe('the page on which a pilot answers a program', () => {
	it('shows nothing before the request was read, then what is asked', async () => {
		const { core, model } = opened();
		expect(model.page).toBeUndefined();
		await model.read();
		expect(core.asked).toEqual([ID]);
		expect(model.words).toEqual(SAID.words);
		expect(model.page).toEqual(waiting('asking'));
		expect(model.reading).toBe(false);
	});

	it('shows each state the core hands it as it is', async () => {
		const { core, model } = opened();
		for (const page of [
			waiting('signInFirst'),
			ENDED,
			closed('answered', 'Already answered', 'Once.'),
			closed('failed', 'Could not be asked', 'Busy.')
		]) {
			core.read = { page, nowUnixS: NOW };
			await model.read();
			expect(model.page).toEqual(page);
		}
	});

	it('goes to the program at once when the answer was taken, and keeps the address nowhere', async () => {
		const { core, model, left } = opened();
		await model.read();
		await model.answer(true);
		expect(core.answered).toEqual([{ id: ID, agree: true }]);
		expect(left).toEqual([BACK]);
		// Nothing the screen draws from holds the address or its code.
		const drawn = JSON.stringify([model.page, model.words, model.said, model.answering]);
		expect(drawn).not.toContain('venti_oac_');
		expect(drawn).not.toContain('assistant.example');
		// The buttons stay as they are while the browser leaves.
		expect(model.answering).toBe('allow');
		await model.answer(false);
		expect(core.answered).toHaveLength(1);
	});

	it('refuses the same way: the server’s address, taken and not kept', async () => {
		const { core, model, left } = opened();
		core.outcome = { ...core.outcome, redirectTo: `${BACK}&error=access_denied` };
		await model.read();
		await model.answer(false);
		expect(core.answered).toEqual([{ id: ID, agree: false }]);
		expect(left).toEqual([`${BACK}&error=access_denied`]);
	});

	it('says why an answer was not taken and lets the pilot answer again', async () => {
		const { core, model, left } = opened();
		await model.read();
		core.outcome = { redirectTo: undefined, state: 'asking', title: '', message: 'Withdraw one.' };
		await model.answer(true);
		expect(left).toEqual([]);
		expect(model.page?.state).toBe('asking');
		expect(model.said).toBe('Withdraw one.');
		expect(model.answering).toBeUndefined();

		core.outcome = { redirectTo: BACK, state: 'answered', title: '', message: '' };
		await model.answer(true);
		expect(model.said).toBeUndefined();
		expect(left).toEqual([BACK]);
	});

	it('shows what the answer says became of the request', async () => {
		for (const state of ['ended', 'answered', 'failed'] as const) {
			const { core, model, left } = opened();
			await model.read();
			core.outcome = { redirectTo: undefined, state, title: 'What became', message: 'Of it.' };
			await model.answer(true);
			expect(left).toEqual([]);
			expect(model.page).toEqual(closed(state, 'What became', 'Of it.'));
			expect(model.answering).toBeUndefined();
			// Nothing is left to answer.
			await model.answer(true);
			expect(core.answered).toHaveLength(1);
		}
	});

	it('asks to sign in again when the session no longer holds, with what was asked still shown', async () => {
		const { core, model, signedOut } = opened();
		await model.read();
		core.outcome = { redirectTo: undefined, state: 'signInFirst', title: '', message: 'Again.' };
		await model.answer(true);
		expect(model.page).toMatchObject({ state: 'signInFirst', account: undefined });
		expect(model.page?.mayRead).toEqual(['Your flights.']);
		expect(model.said).toBe('Again.');
		expect(signedOut()).toBe(1);

		// Read anew, signed in again: the request waits, and the line is gone.
		core.read = { page: waiting('asking'), nowUnixS: NOW };
		await model.read();
		expect(model.page?.state).toBe('asking');
		expect(model.said).toBeUndefined();
	});

	it('ends by itself at the request’s end, counted from the server’s clock', async () => {
		const { core, model } = opened();
		core.read = { page: waiting('signInFirst', NOW + 60), nowUnixS: NOW };
		await model.read();
		vi.advanceTimersByTime(59_000);
		expect(model.page?.state).toBe('signInFirst');
		vi.advanceTimersByTime(1_000);
		expect(model.page).toEqual(ENDED);
	});

	it('lets go of an answer that arrives after the end, unless the server took it', async () => {
		const { core, model, left } = opened();
		core.read = { page: waiting('asking', NOW + 60), nowUnixS: NOW };
		await model.read();
		let release = () => {};
		core.hold = new Promise((done) => (release = done));
		core.outcome = { redirectTo: undefined, state: 'asking', title: '', message: 'Late.' };
		const answering = model.answer(true);
		vi.advanceTimersByTime(60_000);
		expect(model.page).toEqual(ENDED);
		release();
		await answering;
		expect(model.page).toEqual(ENDED);
		expect(model.said).toBeUndefined();
		expect(model.answering).toBeUndefined();
		expect(left).toEqual([]);
	});

	it('is ended at once when the end has passed, and waits for the server without its clock', async () => {
		const { core, model } = opened();
		core.read = { page: waiting('asking', NOW - 5), nowUnixS: NOW };
		await model.read();
		vi.advanceTimersByTime(0);
		expect(model.page).toEqual(ENDED);

		const other = opened();
		other.core.read = { page: waiting('asking', NOW + 60), nowUnixS: 0 };
		await other.model.read();
		vi.advanceTimersByTime(24 * 3600_000);
		expect(other.model.page?.state).toBe('asking');
	});

	it('stops its clock when the page is left', async () => {
		const { core, model } = opened();
		core.read = { page: waiting('asking', NOW + 60), nowUnixS: NOW };
		await model.read();
		model.close();
		vi.advanceTimersByTime(120_000);
		expect(model.page?.state).toBe('asking');
	});
});
