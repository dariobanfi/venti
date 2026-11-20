/**
 * The sentences around groups and plans that are the screens' own. What a
 * plan or a group says about itself ("You, Mara Muster and Jonas
 * Beispiel", "3 members · kept by you") and every refusal is the core's;
 * these are the iPhone app's words from `PlansScreens.swift` and
 * `GroupsScreens.swift`, with "this site's server" where the iPhone says
 * "your server".
 */

import type { GroupMember } from '#lib/core/index.ts';

/** What the board says to a pilot who is signed out. */
export const NEEDS_ACCOUNT =
	'Plans for a flying day are shared in groups on Venti, and a group is seen by its members only. Sign in under Settings, Account, then make a group or join one with the code its keeper gives you.';

/** What the screens say on a site that has no server. */
export const NO_SERVER =
	'Groups and their plans are kept by Venti, and this copy of the app is served without Venti behind it.';

export const NO_GROUP =
	'You are in no group yet. A group is a few pilots who tell each other where they mean to fly; only its members see it.';

export const NO_PLANS = 'Nobody in your groups has posted a plan for today or a later day.';

export const CUT_SHORT = 'Your groups hold more plans than fit here: the later days are missing.';

/** Who sees a plan and how long it is kept, under the board. */
export const WHO_SEES_A_PLAN =
	'A plan is seen by the members of its group and nobody else, and is removed two days after its day.';

/** The same under the form, before a plan is posted. */
export const WHO_SEES_THIS_PLAN =
	'The members of the group see the plan, and nobody else. It is removed two days after its day.';

export const WHAT_A_GROUP_IS =
	'A group is seen by its members and nobody else; it cannot be searched for. Whoever makes it keeps it, and hands out its invite code.';

/** Under an invite code, the one time it is shown. */
export const ABOUT_THE_CODE =
	'Whoever has this code and an account on Venti can come into the group. It is shown this once: Venti keeps no copy that can be read. A new code can be made at any time and ends this one.';

/** A member as a row names them. */
export function memberName(member: Pick<GroupMember, 'name' | 'isMe'>): string {
	return member.isMe ? `${member.name} (you)` : member.name;
}

/** Something about a group or a plan that is asked about before it is done. */
export type Asking =
	| { about: 'newCode' }
	| { about: 'leave'; asKeeper: boolean }
	| { about: 'delete' }
	| { about: 'remove'; member: Pick<GroupMember, 'name' | 'handle'> }
	| { about: 'handOver'; member: Pick<GroupMember, 'name' | 'handle'> }
	| { about: 'deletePlan' };

/** The question, what saying yes means, and the words on the button that says yes. */
export function question(asking: Asking): { title: string; message: string; confirm: string } {
	switch (asking.about) {
		case 'newCode':
			return {
				title: 'Make a new invite code?',
				message: 'The code handed out before stops working. Members stay in the group.',
				confirm: 'Make a new code'
			};
		case 'leave':
			return {
				title: 'Leave this group?',
				message: asking.asKeeper
					? 'You will no longer see its plans, and yours are removed. The member who has been in longest keeps the group; if you are the last, the group is deleted.'
					: 'You will no longer see its plans, and yours are removed.',
				confirm: 'Leave the group'
			};
		case 'delete':
			return {
				title: 'Delete this group?',
				message: 'The group goes for every member, with all its plans.',
				confirm: 'Delete the group'
			};
		case 'remove':
			return {
				title: `Remove ${asking.member.name}?`,
				message:
					'Their plans go with them, and their word on the plans of others. The invite code stops working too, because they know it: nobody comes in until you make a new one.',
				confirm: 'Remove from the group'
			};
		case 'handOver':
			return {
				title: `Hand the keeping to ${asking.member.name}?`,
				message:
					'They will rename the group, make its invite codes, remove members and can delete it. You stay in the group as a member, and cannot take the keeping back yourself.',
				confirm: 'Make keeper'
			};
		case 'deletePlan':
			return {
				title: 'Delete this plan?',
				message: 'It goes for everyone in the group, with who was in.',
				confirm: 'Delete the plan'
			};
	}
}
