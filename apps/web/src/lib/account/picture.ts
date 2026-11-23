/**
 * The pilot's picture as the browser sends it. The server cuts the middle
 * square out of what it is sent and keeps 256 by 256 pixels of it, and
 * takes at most 8 MB (docs/api.md, "PUT and DELETE /v1/account/picture").
 * A photograph from a phone is often larger than that and always far
 * larger than needed, so one that is large is drawn anew in the browser,
 * at most `LONGEST_SIDE` pixels along its longer side, before it is sent:
 * a few hundred kilobytes. A small picture goes as it is. The core says
 * which kinds it takes and refuses the rest in its words.
 */

/** The kinds the server reads, for the file picker. */
export const PICTURE_KINDS = 'image/jpeg,image/png,image/webp';

/** How long the longer side of a picture that is drawn anew is, in pixels. */
export const LONGEST_SIDE = 1024;

/** The bytes above which a picture is drawn anew whatever its size in pixels. */
export const LARGEST_SENT = 2 * 1024 * 1024;

/** The size a picture of `width` by `height` is drawn at, or nothing when it may go as it is. */
export function shrunkSize(
	width: number,
	height: number,
	bytes: number
): { width: number; height: number } | undefined {
	const longest = Math.max(width, height);
	if (longest <= LONGEST_SIDE && bytes <= LARGEST_SENT) return undefined;
	const scale = Math.min(1, LONGEST_SIDE / longest);
	return {
		width: Math.max(1, Math.round(width * scale)),
		height: Math.max(1, Math.round(height * scale))
	};
}

/**
 * The bytes and kind to send for a file the pilot chose: the file as it
 * is, or, when it is large, the picture drawn anew as a WebP (a JPEG where
 * the browser writes no WebP). The browser turns a photograph upright as
 * its EXIF says while it draws it. A file the browser cannot draw goes as
 * it is, for the core and the server to say what is wrong with it.
 */
export async function pictureToSend(file: File): Promise<{ bytes: Uint8Array; mediaType: string }> {
	const asItIs = async () => ({
		bytes: new Uint8Array(await file.arrayBuffer()),
		mediaType: file.type
	});
	let image: ImageBitmap;
	try {
		image = await createImageBitmap(file, { imageOrientation: 'from-image' });
	} catch {
		return asItIs();
	}
	try {
		const size = shrunkSize(image.width, image.height, file.size);
		if (!size) return asItIs();
		const canvas = document.createElement('canvas');
		canvas.width = size.width;
		canvas.height = size.height;
		const context = canvas.getContext('2d');
		if (!context) return asItIs();
		context.drawImage(image, 0, 0, size.width, size.height);
		const blob = (await toBlob(canvas, 'image/webp')) ?? (await toBlob(canvas, 'image/jpeg'));
		if (!blob) return asItIs();
		return { bytes: new Uint8Array(await blob.arrayBuffer()), mediaType: blob.type };
	} finally {
		image.close();
	}
}

/** A canvas as a file of a kind, or nothing where the browser does not write that kind. */
function toBlob(canvas: HTMLCanvasElement, kind: string): Promise<Blob | undefined> {
	return new Promise((resolve) =>
		canvas.toBlob((blob) => resolve(blob && blob.type === kind ? blob : undefined), kind, 0.9)
	);
}
