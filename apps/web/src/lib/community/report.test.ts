import { describe, expect, it } from 'vitest';
import { overLength, reportedFlight, reportedPilot, reportedTrack } from './report';

describe('what can be reported', () => {
	it('is a flight, a pilot’s page or a live track, each with a form of its own name', () => {
		expect(reportedFlight('fbyw03atd9qdt5bfe177ra1k94')).toEqual({
			subject: { kind: 'flight', id: 'fbyw03atd9qdt5bfe177ra1k94' },
			title: 'Report this flight'
		});
		expect(reportedPilot('mara')).toEqual({
			subject: { kind: 'pilot', handle: 'mara' },
			title: 'Report this pilot'
		});
		expect(reportedTrack('11111111111111111111111111')).toEqual({
			subject: { kind: 'liveTrack', id: '11111111111111111111111111' },
			title: 'Report this track'
		});
	});
});

describe('the pilot’s own words in a report', () => {
	it('are counted for the form only once they are more than a report has room for', () => {
		expect(overLength('', 1000)).toBeUndefined();
		expect(overLength('x'.repeat(1000), 1000)).toBeUndefined();
		expect(overLength('x'.repeat(1001), 1000)).toEqual({ has: 1001, atMost: 1000 });
	});

	it('are counted as a reader counts, and without the space around them', () => {
		// The space at both ends is not kept, and a letter outside the
		// first plane is one letter.
		expect(overLength(`  ${'x'.repeat(1000)}\n`, 1000)).toBeUndefined();
		expect(overLength('𝒳'.repeat(1000), 1000)).toBeUndefined();
		expect(overLength('𝒳'.repeat(1001), 1000)).toEqual({ has: 1001, atMost: 1000 });
	});
});
