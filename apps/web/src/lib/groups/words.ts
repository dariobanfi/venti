/**
 * The sentences around groups and plans that are the screens' own. What a
 * plan or a group says about itself ("You, Mara Muster and Jonas
 * Beispiel", "3 members · kept by you") and every refusal is the core's;
 * these are the iPhone app's words from `PlansScreens.swift` and
 * `GroupsScreens.swift`, with "this site's server" where the iPhone says
 * "your server".
 */

import type { CommunityMember, CommunityWords } from '#lib/core/index.ts';

/** What the screens say on a site that has no server. */
export const NO_SERVER =
	'Groups and their plans are kept by Venti, and this copy of the app is served without Venti behind it.';

export const NO_GROUP =
	'You are in no group yet. In a group pilots tell each other where they mean to fly.';

export const NO_PLANS = 'Nobody in your groups has posted a plan for today or a later day.';

export const CUT_SHORT = 'Your groups hold more plans than fit here: the later days are missing.';

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

/**
 * The question, what saying yes means, and the words on the button that
 * says yes. Leaving a group is asked in the core's words, once they are
 * there.
 */
export function question(
	asking: Asking,
	said?: CommunityWords
): { title: string; message: string; confirm: string } {
	switch (asking.about) {
		case 'leave':
			return {
				title: said?.leaveQuestion ?? '',
				message:
					(asking.asOwner
						? said?.leaveMeansOwner
						: asking.isPublic
							? said?.leaveMeansPublic
							: said?.leaveMeansPrivate) ?? '',
				confirm: said?.leaveConfirm ?? ''
			};
		case 'delete':
			return {
				title: 'Delete this group?',
				message: 'The group goes for every member, with its messages and plans.',
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
