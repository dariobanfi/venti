/**
 * The sentences around groups and plans that are the screens' own. What a
 * plan or a group says about itself ("You, Mara Muster and Jonas
 * Beispiel", "3 members · kept by you") and every refusal is the core's;
 * these are the iPhone app's words from `PlansScreens.swift` and
 * `GroupsScreens.swift`, with "this site's server" where the iPhone says
 * "your server".
 */

import type { CommunityMember } from '#lib/core/index.ts';

/** What the board says to a pilot who is signed out. */
export const NEEDS_ACCOUNT =
	'Plans for a flying day are shared in groups on Venti. Sign in under Settings, Account, then join a group near you or make one.';

/** What the screens say on a site that has no server. */
export const NO_SERVER =
	'Groups and their plans are kept by Venti, and this copy of the app is served without Venti behind it.';

export const NO_GROUP =
	'You are in no group yet. In a group pilots tell each other where they mean to fly.';

export const NO_PLANS = 'Nobody in your groups has posted a plan for today or a later day.';

export const CUT_SHORT = 'Your groups hold more plans than fit here: the later days are missing.';

/** Who sees a plan and how long it is kept, under the board. */
export const WHO_SEES_A_PLAN =
	'A plan is seen by the members of its group and nobody else, and is removed two days after its day.';

/** The same under the form, before a plan is posted. */
export const WHO_SEES_THIS_PLAN =
	'The members of the group see the plan, and nobody else. It is removed two days after its day.';

/** A member as a row names them. */
export function memberName(member: Pick<CommunityMember, 'name' | 'isMe'>): string {
	return member.isMe ? `${member.name} (you)` : member.name;
}

/** What is asked before a plan is deleted: the question, what yes means, and the button that says yes. */
export const DELETE_PLAN = {
	title: 'Delete this plan?',
	message: 'It goes for everyone in the group, with who was in.',
	confirm: 'Delete the plan'
};

/** Something about a group that is asked about before it is done. */
export type Asking =
	| { about: 'leave'; asOwner: boolean; isPublic: boolean }
	| { about: 'delete' }
	| { about: 'remove'; member: Pick<CommunityMember, 'name' | 'handle'> }
	| { about: 'handOn'; member: Pick<CommunityMember, 'name' | 'handle'> };

/** The question, what saying yes means, and the words on the button that says yes. */
export function question(asking: Asking): { title: string; message: string; confirm: string } {
	switch (asking.about) {
		case 'leave':
			return {
				title: 'Leave this group?',
				message: asking.asOwner
					? 'The member who has been in longest becomes its owner; if you are the last, the group is deleted.'
					: asking.isPublic
						? 'You can join it again at any time.'
						: 'You come back in only when invited again.',
				confirm: 'Leave the group'
			};
		case 'delete':
			return {
				title: 'Delete this group?',
				message: 'The group goes for every member, with its chat and its plans.',
				confirm: 'Delete the group'
			};
		case 'remove':
			return {
				title: `Remove ${asking.member.name}?`,
				message: 'Their plans go with them.',
				confirm: 'Remove from the group'
			};
		case 'handOn':
			return {
				title: `Hand the group to ${asking.member.name}?`,
				message:
					'They become its owner. You stay in the group as a member, and cannot take it back yourself.',
				confirm: 'Hand on'
			};
	}
}
