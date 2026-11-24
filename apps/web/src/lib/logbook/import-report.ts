import { format } from '#lib/design/format.ts';

/**
 * What happened to a batch of imported files, and what to tell the pilot
 * about it. The wording is the phones' (`LogbookModel.swift`,
 * `ImportReport.kt`); on the website an imported file is an upload to the
 * account, so it says "on your account" where the phones name the device.
 */

export interface ImportFailure {
	/** The name of the file. */
	file: string;
	/** Why it was not imported, in words fit to show. */
	reason: string;
}

export interface ImportReport {
	/** Identifiers of the flights that were added. */
	added: string[];
	/** Addresses of the flights the account already held. */
	alreadyThere: string[];
	failures: ImportFailure[];
}

/** A message for the pilot, shown as a dialog. */
export interface Notice {
	title: string;
	/** The body, one entry to a line. */
	lines: string[];
}

export function emptyReport(): ImportReport {
	return { added: [], alreadyThere: [], failures: [] };
}

/** The most failures named one by one. The rest are counted. */
const FAILURES_SHOWN = 4;

/**
 * What the pilot should be told, if anything. One new flight needs no
 * message: it opens. A batch says in a sentence what it did.
 */
export function importNotice(report: ImportReport): Notice | undefined {
	const { added, alreadyThere, failures } = report;
	if (failures.length > 0) {
		const lines = failures
			.slice(0, FAILURES_SHOWN)
			.map((failure) => `${failure.file}: ${failure.reason}`);
		if (failures.length > FAILURES_SHOWN) {
			lines.push(`and ${failures.length - FAILURES_SHOWN} more.`);
		}
		return {
			title:
				failures.length === 1
					? '1 file could not be imported'
					: `${failures.length} files could not be imported`,
			lines
		};
	}
	if (added.length === 0 && alreadyThere.length > 0) {
		return {
			title: 'Already on your account',
			lines: [
				alreadyThere.length === 1
					? 'This flight was imported before. Nothing was added.'
					: 'These flights were imported before. Nothing was added.'
			]
		};
	}
	if (added.length > 1 || (added.length > 0 && alreadyThere.length > 0)) {
		const before =
			alreadyThere.length === 0
				? ''
				: alreadyThere.length === 1
					? ' 1 was on your account already.'
					: ` ${format().count(alreadyThere.length)} were on your account already.`;
		return {
			title:
				added.length === 1 ? '1 flight added' : `${format().count(added.length)} flights added`,
			lines: [`${added.length === 1 ? 'It is' : 'They are'} on your account.${before}`]
		};
	}
	if (added.length === 0 && alreadyThere.length === 0) {
		return { title: 'No flights found', lines: ['There was no flight file among what you chose.'] };
	}
	return undefined;
}

/** What deleting a flight means, to be read before it is done. */
export function deletionWarning(flights = 1): string {
	const files = flights === 1 ? 'Its file is' : 'Their files are';
	return `${files} removed from Venti and from your other devices. This cannot be undone.`;
}

/**
 * The flight to open straight after an import: the new one, if the import
 * added exactly one and nothing went wrong.
 */
export function flightToShow(report: ImportReport): string | undefined {
	return report.failures.length === 0 && report.added.length === 1 ? report.added[0] : undefined;
}
