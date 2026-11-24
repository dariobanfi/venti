import { describe, expect, it } from 'vitest';
import type { AlertView, AlertsSynced, CoreClient, FiredLine } from '#lib/core/index.ts';
import { AlertsModel, dayDraft, type Notifier, windDraft } from './model.svelte';

const FIRED: FiredLine = {
	alertId: 'a1',
	title: 'Patscherkofel north',
	line: 'Patscherkofel: 15 km/h from the north, gusts 22, at 11:20, as your alarm ‘Patscherkofel north’ asks',
	firedAtUnixS: 1_795_000_000
};

const ALARM: AlertView = {
	id: 'a1',
	name: 'Patscherkofel north',
	enabled: true,
	line: 'Wind at Patscherkofel from N to NE',
	draft: { ...windDraft({ id: 'geosphere-tawes:11126', name: 'Patscherkofel' }), fromPoint: 0 }
};

/** A core that answers as a signed-in account would, and notes what it was asked. */
function fakeCore(synced: AlertsSynced | undefined) {
	const asked: string[] = [];
	let alerts: AlertView[] = [];
	const core = {
		alertsSync: async () => {
			asked.push('sync');
			return synced;
		},
		alertsHeld: async () => ({ alerts, fired: [FIRED], firedCursor: '7' }),
		addAlert: async () => {
			asked.push('add');
			alerts = [ALARM];
			return ALARM;
		},
		changeAlert: async (id: string, draft: { enabled: boolean }) => {
			asked.push(`change ${id} ${draft.enabled}`);
			return ALARM;
		},
		removeAlert: async (id: string) => {
			asked.push(`remove ${id}`);
		}
	} as unknown as CoreClient;
	return { core, asked };
}

function notifier(permission: NotificationPermission) {
	const shown: FiredLine[] = [];
	let askedToNotify = 0;
	const it: Notifier = {
		permission: () => permission,
		ask: async () => void (askedToNotify += 1),
		show: (line) => void shown.push(line)
	};
	return { it, shown, asked: () => askedToNotify };
}

describe('alarms on the page', () => {
	it('start the form with the station or the take-off filled in', () => {
		expect(windDraft({ id: 'geosphere-tawes:11126', name: 'Patscherkofel' })).toMatchObject({
			kind: 'wind',
			name: 'Patscherkofel',
			stationId: 'geosphere-tawes:11126',
			stationName: 'Patscherkofel',
			enabled: true
		});
		expect(
			dayDraft({ id: 'venti:s1', name: 'Brauneck', position: { lat: 47.66, lon: 11.52 } })
		).toMatchObject({
			kind: 'day',
			placeName: 'Brauneck',
			siteId: 'venti:s1',
			lat: 47.66,
			lon: 11.52,
			daysAhead: [0, 1],
			atLeast: 'crossCountry'
		});
	});

	it('show the browser’s notification for what is new, where the pilot allowed it', async () => {
		const { core } = fakeCore({ kind: 'finished', newLines: [FIRED] });
		const shown = notifier('granted');
		const model = new AlertsModel(core, () => {}, shown.it);
		await model.sync();
		expect(shown.shown).toEqual([FIRED]);
		expect(model.fired).toEqual([FIRED]);
	});

	it('show no notification where the pilot did not allow it: the bubble on the picture tells', async () => {
		const { core } = fakeCore({ kind: 'finished', newLines: [FIRED, FIRED] });
		const shown = notifier('default');
		const model = new AlertsModel(core, () => {}, shown.it);
		await model.sync();
		expect(shown.shown).toEqual([]);
		expect(model.fired).toEqual([FIRED]);
	});

	it('say why a fetch stopped, in the core’s words', async () => {
		const { core } = fakeCore({
			kind: 'failed',
			message: 'This device is no longer signed in. Sign in again.',
			needsSignIn: true
		});
		const model = new AlertsModel(core, () => {}, notifier('default').it);
		await model.sync();
		expect(model.problem).toBe('This device is no longer signed in. Sign in again.');
	});

	it('ask to notify when the first alarm is saved, and fetch after every change', async () => {
		const { core, asked } = fakeCore({ kind: 'finished', newLines: [] });
		const shown = notifier('default');
		const model = new AlertsModel(core, () => {}, shown.it);
		await model.reread();
		await model.save(undefined, ALARM.draft!);
		expect(shown.asked()).toBe(1);
		expect(model.alerts).toEqual([ALARM]);
		await model.switch(ALARM, false);
		await model.remove('a1');
		expect(asked).toEqual(['add', 'sync', 'change a1 false', 'sync', 'remove a1', 'sync']);
		// Only the first one asks.
		expect(shown.asked()).toBe(1);
	});
});
