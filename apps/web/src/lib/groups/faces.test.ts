import { describe, expect, it } from 'vitest';
import { facesOf, initials } from './faces';

describe('initials', () => {
	it('takes the first letters of the first and the last word', () => {
		expect(initials('Mara Muster')).toBe('MM');
		expect(initials('Jonas van Beispiel')).toBe('JB');
		expect(initials('lena')).toBe('L');
		expect(initials('Élodie Ötzi')).toBe('ÉÖ');
	});

	it('passes over what is not a letter', () => {
		expect(initials('  (Mira)  Member ')).toBe('MM');
		expect(initials('— —')).toBe('?');
	});
});

describe('facesOf', () => {
	it('gives the pilot of this browser their own initials, first', () => {
		const { faces, more } = facesOf(['You', 'Mira Member'], true, 'Test Pilot');
		expect(faces).toEqual([
			{ name: 'You', initials: 'TP', isMe: true },
			{ name: 'Mira Member', initials: 'MM', isMe: false }
		]);
		expect(more).toBe(0);
	});

	it('counts those who do not fit', () => {
		const names = ['A One', 'B Two', 'C Three', 'D Four', 'E Five', 'F Six'];
		const { faces, more } = facesOf(names, false, undefined, 5);
		expect(faces.map((face) => face.initials)).toEqual(['AO', 'BT', 'CT', 'DF']);
		expect(more).toBe(2);
		expect(facesOf(names.slice(0, 5), false, undefined, 5).more).toBe(0);
	});
});
