/**
 * The text of a flight file, from its bytes.
 *
 * Flight files are plain ASCII by specification, but recorders write pilot
 * and glider names in whatever encoding they like. Bytes that are not valid
 * UTF-8 are read as Western European text, the usual other choice, so that
 * an umlaut in a name stays an umlaut.
 */
export function flightFileText(bytes: ArrayBuffer | Uint8Array): string {
	try {
		return new TextDecoder('utf-8', { fatal: true }).decode(bytes);
	} catch {
		return new TextDecoder('windows-1252').decode(bytes);
	}
}
