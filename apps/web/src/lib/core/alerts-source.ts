/**
 * The pilot's alarms, as the worker talks to the server about them: "tell
 * me when the wind at the Patscherkofel is from the north at 10 to 20
 * km/h", "tell me when Saturday becomes a cross-country day at my
 * take-off". The server checks them where the data arrives and keeps what
 * fired; the browser fetches it.
 *
 * Everything a pilot reads is the core's (`crates/core/src/alerts.rs`): the
 * words of the screens, the sixteen points, the line a draft makes, the faults of a draft by field,
 * the requests, what the answers mean, and the lines of what fired. This
 * file sends the requests in between, with the session token, and runs
 * `Core.alertsSync`, which keeps the list and what fired in the browser so
 * that both show without a network.
 */

import { alone, type Send, sendToThisOrigin } from './account-source';
import {
	alertAddRequest,
	alertChangeRequest,
	alertCheck,
	type AlertDraft,
	type AlertFaultView,
	type AlertFormWords,
	alertFormWords,
	alertLine,
	alertPoints,
	alertRemoved,
	alertRemoveRequest,
	alertSaved,
	type AlertView,
	type FiredLine
} from './pkg/venti_core_wasm.js';
import type { CoreSession } from './session';
import { StaleStoreError } from './store';

/** The alarms and what they fired, as the browser last fetched them. */
export interface AlertsHeld {
	alerts: AlertView[];
	/** What fired, the newest first. */
	fired: FiredLine[];
	/**
	 * The cursor of the last answer of what fired that the browser fetched,
	 * for the marker of what is new (`unreadSeenRequest`); none signed out
	 * and before the first fetch.
	 */
	firedCursor: string | undefined;
}

/** How a run of fetching the alarms ended. */
export type AlertsSynced =
	| {
			kind: 'finished';
			/** What fired since the run before: what a notification says. */
			newLines: FiredLine[];
	  }
	| {
			kind: 'failed';
			/** Why it stopped, in the core's words. */
			message: string;
			/** Whether the session has ended and the pilot has to sign in again. */
			needsSignIn: boolean;
	  };

/** How often a run starts over when the store keeps changing under it. */
const ATTEMPTS = 3;

export class AlertsSource {
	#session: CoreSession;
	#send: Send;

	constructor(session: CoreSession, send: Send = sendToThisOrigin) {
		this.#session = session;
		this.#send = send;
	}

	/** The sixteen points of the compass, short, from the north clockwise. */
	points(): string[] {
		return alertPoints();
	}

	/** Every word of the alarms' screens. */
	formWords(): AlertFormWords {
		return alertFormWords();
	}

	/** What a draft waits for, in one line, as it changes. */
	line(draft: AlertDraft): string {
		return alertLine(draft);
	}

	/** What is wrong with a draft, field by field. Empty when it can be sent. */
	check(draft: AlertDraft): AlertFaultView[] {
		return alertCheck(draft);
	}

	/** The alarms and what they fired as the browser holds them. Empty when signed out. */
	async held(nowUnixS: number, utcOffsetS: number): Promise<AlertsHeld> {
		return {
			alerts: await this.#session.ask((core) => core.alertsHeld()),
			fired: await this.#session.ask((core) => core.alertsFiredHeld(nowUnixS, utcOffsetS)),
			firedCursor: (await this.#session.ask((core) => core.alertsFiredCursor())) ?? undefined
		};
	}

	/**
	 * Fetches the alarms and what fired since the last run into the
	 * browser. Nothing in a browser that is not signed in. Throws when the
	 * server cannot be reached; what was fetched until then is kept.
	 */
	async sync(utcOffsetS: number): Promise<AlertsSynced | undefined> {
		if ((await this.#session.token()) === undefined) return undefined;
		return alone(async () => {
			for (let attempt = 1; ; attempt += 1) {
				const run = await this.#session.beginAlertsSync();
				try {
					let step = await run.start(0);
					while (step.kind === 'send') {
						const answer = await this.#send(step.request, await this.#session.token());
						step = await run.receive(answer.status, answer.body, answer.retryAfterS);
					}
					if (step.kind === 'finished') {
						return { kind: 'finished', newLines: await run.newLines(utcOffsetS) };
					}
					// The server no longer takes the token: it is of no use to anyone.
					if (step.needsSignIn) await this.#session.forgetToken();
					return { kind: 'failed', message: step.message, needsSignIn: step.needsSignIn };
				} catch (error) {
					// Another tab changed the store; start over from what is there.
					if (!(error instanceof StaleStoreError) || attempt === ATTEMPTS) throw error;
				} finally {
					run.end();
				}
			}
		});
	}

	/** Sets a new alarm. Throws, in the core's words, when the draft or the server refuses. */
	async add(draft: AlertDraft): Promise<AlertView> {
		const answer = await this.#send(alertAddRequest(draft), await this.#session.token());
		return alertSaved(answer.status, answer.body);
	}

	/** Changes an alarm, by the identifier the list gives. */
	async change(id: string, draft: AlertDraft): Promise<AlertView> {
		const answer = await this.#send(alertChangeRequest(id, draft), await this.#session.token());
		return alertSaved(answer.status, answer.body);
	}

	/** Removes an alarm. One that is gone already is removed. */
	async remove(id: string): Promise<void> {
		const answer = await this.#send(alertRemoveRequest(id), await this.#session.token());
		// Throws the server's refusal in the core's words.
		alertRemoved(answer.status, answer.body);
	}
}
