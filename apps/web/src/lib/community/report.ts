/**
 * What can be reported to whoever runs the server, and what its form is
 * called. The reasons, the notice and every refusal are the core's; the
 * titles are the iPhone app's, from `ReportSheet.swift`.
 */

import type { ReportSubject } from '#lib/core/index.ts';

/** Something a pilot reports, with what the form about it is called. */
export interface Reported {
	subject: ReportSubject;
	/** The form's title, and the words on what opens it. */
	title: string;
}

/** A flight someone shared, by its address on the server. */
export function reportedFlight(id: string): Reported {
	return { subject: { kind: 'flight', id }, title: 'Report this flight' };
}

/** A pilot's page, by the pilot's handle. */
export function reportedPilot(handle: string): Reported {
	return { subject: { kind: 'pilot', handle }, title: 'Report this pilot' };
}

/** A pilot's picture, the one they show now, by the pilot's handle; `title` is the core's "Report picture". */
export function reportedPicture(handle: string, title: string): Reported {
	return { subject: { kind: 'picture', handle }, title };
}

/** A flight shared while it is under way, by its track's identifier. */
export function reportedTrack(id: string): Reported {
	return { subject: { kind: 'liveTrack', id }, title: 'Report this track' };
}

/** A group the pilot is in, by its identifier. */
export function reportedGroup(id: string): Reported {
	return { subject: { kind: 'group', id }, title: 'Report this group' };
}

/** A plan in a group the pilot is in. */
export function reportedPlan(groupId: string, id: string): Reported {
	return { subject: { kind: 'plan', groupId, id }, title: 'Report this plan' };
}

/** An open plan, which everyone can read, by its identifier. */
export function reportedOpenPlan(id: string): Reported {
	return { subject: { kind: 'openPlan', id }, title: 'Report this plan' };
}

/** A flying site that pilots made, by its identifier. */
export function reportedSite(id: string): Reported {
	return { subject: { kind: 'site', id }, title: 'Report this site' };
}

/** One revision of such a site. */
export function reportedSiteRevision(id: string, revision: number): Reported {
	return { subject: { kind: 'siteRevision', id, revision }, title: 'Report this revision' };
}

/**
 * How many characters the pilot's words have, when that is more than a
 * report has room for; nothing while they fit. This is only what the form
 * shows while the pilot types: whether a report is too long is the core's
 * to say, which counts after cleaning the text as the server does.
 */
export function overLength(
	words: string,
	atMost: number
): { has: number; atMost: number } | undefined {
	// Characters as a reader counts them, not the units a script stores them in.
	const has = [...words.trim()].length;
	return has > atMost ? { has, atMost } : undefined;
}
