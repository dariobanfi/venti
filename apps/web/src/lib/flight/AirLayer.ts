/**
 * The flight in the air, drawn into the flight map's own WebGL context as
 * one custom layer of MapLibre's: the track at its height as a crossed
 * ribbon in the climb colours on its dark casing, the curtain down to the
 * ground, the thermals' columns, and the marked moment's line down to the
 * ground. It is a 3D layer, so the terrain hides what lies behind it.
 *
 * Everything is built once, by `air.ts`, into buffers; a frame only sets
 * the matrix and the ribbon's width and draws. The words beside the flight
 * (a thermal's climb, the deepest glide's loss) and the marked moment's dot
 * are elements of the page, which the layer moves to where their places lie
 * on the screen after each frame.
 */

import type { CustomLayerInterface, CustomRenderMethodInput, Map } from 'maplibre-gl';
import type { Local, Mesh, Ribbon, Rgba } from './air';
import { frameMatrix, type Frame, multiply, onScreen } from './air';

/** What the layer draws, built by `air.ts`. */
export interface AirScene {
	frame: Frame;
	track: Ribbon;
	curtain: Mesh;
	columns: Ribbon;
	/** The colour of the track's casing. */
	casing: Rgba;
}

/** What a test can read of the layer: how many frames it drew, and what the last cost. */
export interface AirStats {
	frames: number;
	lastFrameMs: number;
	vertices: number;
}

/** How wide the track's ribbon is on the screen, in pixels, and its casing. */
const TRACK_PX = 2.5;
const CASING_PX = 5.5;
const COLUMN_PX = 9;

const RIBBON_VERTEX = `#version 300 es
in vec3 a_centre;
in vec3 a_side;
in vec4 a_colour;
uniform mat4 u_matrix;
uniform float u_width;
out vec4 v_colour;
void main() {
	v_colour = a_colour;
	gl_Position = u_matrix * vec4(a_centre + a_side * u_width, 1.0);
}`;

const MESH_VERTEX = `#version 300 es
in vec3 a_position;
in vec4 a_colour;
uniform mat4 u_matrix;
out vec4 v_colour;
void main() {
	v_colour = a_colour;
	gl_Position = u_matrix * vec4(a_position, 1.0);
}`;

const FRAGMENT = `#version 300 es
precision mediump float;
in vec4 v_colour;
uniform vec4 u_tint;
uniform float u_tinted;
out vec4 colour;
void main() {
	vec4 c = mix(v_colour, u_tint, u_tinted);
	colour = vec4(c.rgb * c.a, c.a);
}`;

function program(gl: WebGL2RenderingContext, vertex: string): WebGLProgram {
	const made = gl.createProgram()!;
	for (const [type, source] of [
		[gl.VERTEX_SHADER, vertex],
		[gl.FRAGMENT_SHADER, FRAGMENT]
	] as const) {
		const shader = gl.createShader(type)!;
		gl.shaderSource(shader, source);
		gl.compileShader(shader);
		gl.attachShader(made, shader);
	}
	gl.linkProgram(made);
	return made;
}

function buffer(gl: WebGL2RenderingContext, data: Float32Array): WebGLBuffer {
	const made = gl.createBuffer()!;
	gl.bindBuffer(gl.ARRAY_BUFFER, made);
	gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
	return made;
}

interface Bound {
	buffers: WebGLBuffer[];
	count: number;
	draw: (gl: WebGL2RenderingContext) => void;
}

export class AirLayer implements CustomLayerInterface {
	readonly id = 'flight-in-the-air';
	readonly type = 'custom';
	readonly renderingMode = '3d';

	stats: AirStats = { frames: 0, lastFrameMs: 0, vertices: 0 };

	#scene: AirScene;
	#map: Map | undefined;
	#ribbonProgram: WebGLProgram | undefined;
	#meshProgram: WebGLProgram | undefined;
	#track: Bound | undefined;
	#curtain: Bound | undefined;
	#columns: Bound | undefined;
	#marker: Bound | undefined;
	/** The marked moment's line down to the ground, while one is marked. */
	#markerLine: [top: Local, bottom: Local] | undefined;
	/** Whether the line's buffer is to be made again before the next frame. */
	#markerChanged = false;
	/** Called after each frame with the matrix to the screen and the screen's size. */
	#placed: (place: (at: Local) => { x: number; y: number } | undefined) => void;

	constructor(
		scene: AirScene,
		placed: (place: (at: Local) => { x: number; y: number } | undefined) => void
	) {
		this.#scene = scene;
		this.#placed = placed;
	}

	onAdd(map: Map, gl: WebGL2RenderingContext): void {
		this.#map = map;
		this.#ribbonProgram = program(gl, RIBBON_VERTEX);
		this.#meshProgram = program(gl, MESH_VERTEX);
		this.#track = this.#ribbonOf(gl, this.#scene.track);
		this.#columns = this.#ribbonOf(gl, this.#scene.columns);
		this.#curtain = this.#meshOf(gl, this.#scene.curtain);
		this.stats.vertices =
			this.#scene.track.count + this.#scene.columns.count + this.#scene.curtain.count;
	}

	onRemove(_map: Map, gl: WebGL2RenderingContext): void {
		for (const bound of [this.#track, this.#columns, this.#curtain, this.#marker]) {
			for (const made of bound?.buffers ?? []) gl.deleteBuffer(made);
		}
		if (this.#ribbonProgram) gl.deleteProgram(this.#ribbonProgram);
		if (this.#meshProgram) gl.deleteProgram(this.#meshProgram);
	}

	/** Marks a moment: a line from the glider down to the ground. Nothing unmarks it. */
	mark(line: [top: Local, bottom: Local] | undefined): void {
		this.#markerLine = line;
		this.#markerChanged = true;
		this.#map?.triggerRepaint();
	}

	#ribbonOf(gl: WebGL2RenderingContext, ribbon: Ribbon): Bound {
		const made = this.#ribbonProgram!;
		const buffers = [
			buffer(gl, ribbon.centres),
			buffer(gl, ribbon.sides),
			buffer(gl, ribbon.colours)
		];
		const attributes: [string, number][] = [
			['a_centre', 3],
			['a_side', 3],
			['a_colour', 4]
		];
		return {
			buffers,
			count: ribbon.count,
			draw: (gl) => {
				attributes.forEach(([name, size], index) => {
					const location = gl.getAttribLocation(made, name);
					gl.bindBuffer(gl.ARRAY_BUFFER, buffers[index]);
					gl.enableVertexAttribArray(location);
					gl.vertexAttribPointer(location, size, gl.FLOAT, false, 0, 0);
				});
				gl.drawArrays(gl.TRIANGLES, 0, ribbon.count);
			}
		};
	}

	#meshOf(gl: WebGL2RenderingContext, mesh: Mesh): Bound {
		const made = this.#meshProgram!;
		const buffers = [buffer(gl, mesh.positions), buffer(gl, mesh.colours)];
		return {
			buffers,
			count: mesh.count,
			draw: (gl) => {
				(
					[
						['a_position', 3],
						['a_colour', 4]
					] as const
				).forEach(([name, size], index) => {
					const location = gl.getAttribLocation(made, name);
					gl.bindBuffer(gl.ARRAY_BUFFER, buffers[index]);
					gl.enableVertexAttribArray(location);
					gl.vertexAttribPointer(location, size, gl.FLOAT, false, 0, 0);
				});
				gl.drawArrays(gl.TRIANGLES, 0, mesh.count);
			}
		};
	}

	render(gl: WebGL2RenderingContext, options: CustomRenderMethodInput): void {
		const started = performance.now();
		const map = this.#map;
		if (!map || !this.#track || !this.#ribbonProgram || !this.#meshProgram) return;
		const matrix = multiply(
			options.defaultProjectionData.mainMatrix,
			frameMatrix(this.#scene.frame)
		);
		// How long a pixel is on the ground at the flight, in metres.
		const pixelM =
			(2 * Math.PI * 6_378_137 * Math.cos((map.getCenter().lat * Math.PI) / 180)) /
			(512 * 2 ** map.getZoom());

		gl.enable(gl.BLEND);
		gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
		gl.enable(gl.DEPTH_TEST);
		gl.depthFunc(gl.LEQUAL);

		// The curtain first: see-through, and hiding nothing behind it.
		gl.useProgram(this.#meshProgram);
		gl.uniformMatrix4fv(gl.getUniformLocation(this.#meshProgram, 'u_matrix'), false, matrix);
		gl.uniform1f(gl.getUniformLocation(this.#meshProgram, 'u_tinted'), 0);
		gl.depthMask(false);
		this.#curtain?.draw(gl);

		gl.useProgram(this.#ribbonProgram);
		const width = gl.getUniformLocation(this.#ribbonProgram, 'u_width');
		const tinted = gl.getUniformLocation(this.#ribbonProgram, 'u_tinted');
		gl.uniformMatrix4fv(gl.getUniformLocation(this.#ribbonProgram, 'u_matrix'), false, matrix);
		// The thermals' columns, see-through.
		gl.uniform1f(tinted, 0);
		gl.uniform1f(width, (COLUMN_PX / 2) * pixelM);
		this.#columns?.draw(gl);
		// The track's dark casing, writing no depth, then the track over it.
		gl.uniform4fv(gl.getUniformLocation(this.#ribbonProgram, 'u_tint'), this.#scene.casing);
		gl.uniform1f(tinted, 1);
		gl.uniform1f(width, (CASING_PX / 2) * pixelM);
		this.#track.draw(gl);
		gl.depthMask(true);
		gl.uniform1f(tinted, 0);
		gl.uniform1f(width, (TRACK_PX / 2) * pixelM);
		this.#track.draw(gl);

		// The marked moment's line down to the ground, one pixel.
		if (this.#markerChanged) {
			for (const made of this.#marker?.buffers ?? []) gl.deleteBuffer(made);
			this.#marker = undefined;
			if (this.#markerLine) {
				const [top, bottom] = this.#markerLine;
				const ink = this.#scene.casing;
				this.#marker = this.#meshOf(gl, {
					positions: new Float32Array([...top, ...bottom]),
					colours: new Float32Array([...ink, ...ink]),
					count: 2
				});
			}
			this.#markerChanged = false;
		}
		if (this.#marker) {
			gl.useProgram(this.#meshProgram);
			const made = this.#meshProgram;
			this.#marker.buffers.forEach((bound, index) => {
				const location = gl.getAttribLocation(made, index === 0 ? 'a_position' : 'a_colour');
				gl.bindBuffer(gl.ARRAY_BUFFER, bound);
				gl.enableVertexAttribArray(location);
				gl.vertexAttribPointer(location, index === 0 ? 3 : 4, gl.FLOAT, false, 0, 0);
			});
			gl.drawArrays(gl.LINES, 0, 2);
		}

		const canvas = map.getCanvas();
		const [w, h] = [canvas.clientWidth, canvas.clientHeight];
		this.#placed((at) => onScreen(matrix, at, w, h));
		this.stats = {
			...this.stats,
			frames: this.stats.frames + 1,
			lastFrameMs: performance.now() - started
		};
	}
}
