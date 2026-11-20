/**
 * Who can see a flight, in words for a list to choose from. The iPhone app
 * offers the same three answers in the same words (`AccountModel.swift`).
 * What each setting lets the server do is the server's rule; these only
 * describe it.
 */

import type { FlightVisibility } from '#lib/core/index.ts';

export interface VisibilityChoice {
	visibility: FlightVisibility;
	title: string;
	/** What the setting means. */
	explanation: string;
}

/** The three answers, the most private first. */
export const VISIBILITY_CHOICES: readonly VisibilityChoice[] = [
	{
		visibility: 'private',
		title: 'Only me',
		explanation: 'The flight is kept on Venti for your devices. Nobody else can see it.'
	},
	{
		visibility: 'unlisted',
		title: 'Anyone with the link',
		explanation: 'Whoever has the link can see the flight. It is listed nowhere.'
	},
	{
		visibility: 'public',
		title: 'Everyone',
		explanation: 'Anyone can see the flight, and it is listed on your page.'
	}
];
