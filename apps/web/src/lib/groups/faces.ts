/**
 * Who is in on a plan, as a row of round faces: each pilot's initials,
 * the pilot of this browser first. The core names them (`pilotsIn`, with
 * the pilot of this browser as "You"); a pilot has no picture yet, so the
 * face is the initials of the name.
 */

/** The initials of a name: the first letters of its first and last words, "Mara Muster" → "MM". */
export function initials(name: string): string {
	const words = name
		.trim()
		.split(/\s+/)
		.filter((word) => /\p{L}|\p{N}/u.test(word));
	if (words.length === 0) return '?';
	const first = (word: string) => [...word.replace(/^[^\p{L}\p{N}]+/u, '')][0] ?? '';
	const letters =
		words.length === 1 ? first(words[0]) : first(words[0]) + first(words[words.length - 1]);
	return letters.toLocaleUpperCase();
}

/** One face of the row. */
export interface Face {
	/** The name as the core says it, "You" for the pilot of this browser. */
	name: string;
	initials: string;
	isMe: boolean;
}

/**
 * The faces of who is in, at most `room` of them, and how many more there
 * are. `myName` is the name the pilot of this browser is shown by, for
 * their own face's initials.
 */
export function facesOf(
	pilotsIn: readonly string[],
	isIn: boolean,
	myName: string | undefined,
	room = 5
): { faces: Face[]; more: number } {
	const faces = pilotsIn.map((name, index) => {
		const isMe = isIn && index === 0;
		return { name, isMe, initials: initials(isMe && myName ? myName : name) };
	});
	const shown = faces.length > room ? faces.slice(0, room - 1) : faces;
	return { faces: shown, more: faces.length - shown.length };
}
