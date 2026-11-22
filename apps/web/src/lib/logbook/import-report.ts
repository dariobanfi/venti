/**
 * What happened to a batch of imported files, and what to tell the pilot
 * about it. The wording follows the iPhone app's (`LogbookModel.swift`);
 * on the website a file brought in is an upload to the account.
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
 * What the pilot should be told, if anything. A clean import needs no
 * message: the new flights appear in the list.
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
					? '1 file could not be brought in'
					: `${failures.length} files could not be brought in`,
			lines
		};
	}
	if (added.length === 0 && alreadyThere.length > 0) {
		return {
			title: 'Already on your account',
			lines: [
				alreadyThere.length === 1
					? 'This flight was brought in before. Nothing was added.'
					: 'These flights were brought in before. Nothing was added.'
			]
		};
	}
	return undefined;
}

/** What deleting a flight means, to be read before it is done. */
export function deletionWarning(count = 1): string {
	const files = count === 1 ? 'It is' : 'They are';
	return `${files} deleted from your Venti account and your other devices at once. This cannot be undone.`;
}

/**
 * The flight to open straight after an import: the new one, if the import
 * brought exactly one and nothing went wrong.
 */
export function flightToShow(report: ImportReport): string | undefined {
	return report.failures.length === 0 && report.added.length === 1 ? report.added[0] : undefined;
}
