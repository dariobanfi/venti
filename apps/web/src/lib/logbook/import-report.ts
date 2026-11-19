/**
 * What happened to a batch of imported files, and what to tell the pilot
 * about it. The wording is the iPhone app's (`LogbookModel.swift`).
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
	/** Identifiers of the flights the logbook already held. */
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
					? '1 file could not be imported'
					: `${failures.length} files could not be imported`,
			lines
		};
	}
	if (added.length === 0 && alreadyThere.length > 0) {
		return {
			title: 'Already in your logbook',
			lines: [
				alreadyThere.length === 1
					? 'This flight was imported before. Nothing was added.'
					: 'These flights were imported before. Nothing was added.'
			]
		};
	}
	return undefined;
}

/**
 * What deleting a flight means, to be read before it is done. With an
 * account the flight also goes from the server, and from there from the
 * pilot's other devices.
 */
export function deletionWarning(signedIn: boolean): string {
	return signedIn
		? 'Its file is removed from this browser and, with the next sync, from your Venti server and your other devices. This cannot be undone.'
		: 'Its file is removed from this browser. This cannot be undone.';
}

/**
 * The flight to open straight after an import: the new one, if the import
 * brought exactly one and nothing went wrong.
 */
export function flightToShow(report: ImportReport): string | undefined {
	return report.failures.length === 0 && report.added.length === 1 ? report.added[0] : undefined;
}
