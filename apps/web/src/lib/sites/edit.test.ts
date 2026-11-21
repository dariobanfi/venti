import { readFileSync } from 'node:fs';
import { beforeAll, describe, expect, it } from 'vitest';
import type { CoreClient, SiteDraftView, SiteSent, SiteView } from '#lib/core/index.ts';
import {
	siteCorrecting,
	siteDraftCheck,
	siteDraftOf,
	siteDraftRebased,
	siteDraftWindInWords,
	sitePage
} from '#lib/core/pkg/venti_core_wasm.js';
import { startCore } from '#lib/core/testing.ts';
import { SiteDraft } from './edit.svelte';

beforeAll(startCore);

/** The development mode's made-up take-off of a partner, as the server answers for it. */
function kogel(): SiteView {
	const recorded = readFileSync(
		new URL('../../../../../crates/server/tests/golden/site.json', import.meta.url)
	);
	return sitePage(200, recorded).site;
}

/**
 * The core as a draft asks it, over the real core's own functions. The
 * server is the test's: what it answers to a correction, and how the site
 * stands when it is read again.
 */
function core(server: { answers: SiteSent[]; standsNow: SiteView; sent: SiteDraftView[] }) {
	return {
		siteDraftCheck: async (draft: SiteDraftView, comment: string) => ({
			faults: siteDraftCheck(draft, comment),
			windInWords: siteDraftWindInWords(new Float64Array(draft.windFromDeg))
		}),
		changeSite: async (draft: SiteDraftView) => {
			server.sent.push(draft);
			return server.answers.shift()!;
		},
		sitePage: async () => ({
			site: server.standsNow,
			credit: undefined,
			isLasting: true,
			canBeCorrected: true
		}),
		siteDraftRebased: async (base: SiteView, edited: SiteDraftView, now: SiteView) =>
			siteDraftRebased(base, edited, now)
	} as unknown as CoreClient;
}

const CHANGED: SiteSent = {
	kind: 'refused',
	words:
		'Someone else changed this site while you were editing it. Look at how it stands now, then make your change again.',
	inTheWay: undefined,
	changedMeanwhile: true
};

describe('a correction that someone else was quicker than', () => {
	it('is carried onto the site as it stands by the core, and can be sent again', async () => {
		const base = kogel();
		// Meanwhile the site was renamed and its height changed, as its next revision.
		const standsNow: SiteView = {
			...base,
			name: 'Musterkogel Nord',
			elevationM: 1850,
			byPilots: base.byPilots && { ...base.byPilots, revision: base.byPilots.revision + 1 }
		};
		const server = {
			answers: [CHANGED, { kind: 'written' as const, site: standsNow, words: 'Saved.' }],
			standsNow,
			sent: [] as SiteDraftView[]
		};
		const editing = new SiteDraft(core(server), siteDraftOf(base), base, siteCorrecting(base));
		// The words of the form are those of the site's tier, as the core says them.
		expect(editing.correcting?.asPartner).toBe(true);
		editing.set({ elevationM: 1871, fee: 'None' });

		expect(await editing.send()).toBeUndefined();

		// What the pilot entered stays; what the pilot left alone is as the site stands now.
		expect(editing.draft).toMatchObject({
			name: 'Musterkogel Nord',
			elevationM: 1871,
			fee: 'None',
			revision: standsNow.byPilots?.revision
		});
		// The core's words, what the others changed, and the field both changed.
		expect(editing.refusal?.words).toMatch(
			/^Someone else changed this site while you were editing it\./
		);
		expect(editing.changedMeanwhile).toContain('Name: “Musterkogel West” → “Musterkogel Nord”');
		// The core's sentence for it stands at the field.
		expect(editing.clashes.map((clash) => clash.field)).toEqual(['height']);
		expect(editing.clashed('height')).toMatch(
			/^Height: you entered 1871 m, it now stands at 1850 m\./
		);
		expect(editing.clashed('name')).toBeUndefined();
		expect(editing.standsNow).toBe(standsNow);

		// Sent again, it stands on the newest revision and is taken.
		const written = await editing.send();
		expect(written?.words).toBe('Saved.');
		expect(server.sent[1]).toMatchObject({ revision: standsNow.byPilots?.revision, fee: 'None' });
		expect(server.sent[1].name).toBe('Musterkogel Nord');

		// The mark of a clash goes when the pilot changes something.
		editing.set({ elevationM: 1850 });
		expect(editing.clashes).toEqual([]);
	});
});
