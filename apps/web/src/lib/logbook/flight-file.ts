/**
 * Flight files on their way into the page, dropped by the pilot, and on
 * their way out again.
 */

/**
 * Hands the pilot a file to keep: the browser saves it with its downloads
 * under `name`.
 */
export function saveFile(name: string, text: string): void {
	// Not a kind of file the browser would rather show than save.
	const address = URL.createObjectURL(new Blob([text], { type: 'application/octet-stream' }));
	const link = document.createElement('a');
	link.href = address;
	link.download = name;
	document.body.append(link);
	link.click();
	link.remove();
	// The browser has taken the file by the time the page next gets a turn.
	setTimeout(() => URL.revokeObjectURL(address), 10_000);
}

/** The files among what was dragged onto the page. Text and links are not files. */
export function droppedFiles(transfer: DataTransfer | null): File[] {
	return transfer ? [...transfer.files] : [];
}

/** Whether what is being dragged over the page includes files at all. */
export function carriesFiles(transfer: DataTransfer | null): boolean {
	return transfer?.types.includes('Files') ?? false;
}
