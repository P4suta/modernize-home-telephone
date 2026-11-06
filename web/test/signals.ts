const MIN_FOURIER_SAMPLES = 2;
const NYQUIST_DIVISOR = 2;
const HANN_WEIGHT = 0.5;
const FULL_CYCLE = 2 * Math.PI;
const MAX_ANALYSIS_SAMPLES = 4096;
const MIN_ANALYSIS_SAMPLES = 800;
const WINDOW_OVERLAP_DIVISOR = 2;
const MIN_SIGNAL_POWER = 0.000001;
const MIN_NULL_POWER = 0.000000001;
const NULL_FREQUENCY = 730;
const FOURCC_BYTES = 4;
const RIFF_HEADER_BYTES = 12;
const CHUNK_HEADER_BYTES = 8;
const WAVE_FORMAT_OFFSET = 8;
const MIN_WAV_BYTES = 44;
const PCM_FORMAT_BYTES = 16;
const PCM_SAMPLE_BITS = 16;
const PCM_SAMPLE_BYTES = 2;
const CHANNELS_OFFSET = 2;
const SAMPLE_RATE_OFFSET = 4;
const SAMPLE_BITS_OFFSET = 14;
const PCM_SAMPLE_RATE = 8000;
const PCM_FULL_SCALE = 32768;

export function tonePower(
	samples: readonly number[],
	rate: number,
	frequency: number,
): number {
	if (
		samples.length < MIN_FOURIER_SAMPLES ||
		rate <= 0 ||
		frequency <= 0 ||
		frequency >= rate / NYQUIST_DIVISOR
	)
		return 0;
	let real = 0;
	let imaginary = 0;
	let weight = 0;
	for (const [index, sample] of samples.entries()) {
		const window =
			HANN_WEIGHT -
			HANN_WEIGHT * Math.cos((FULL_CYCLE * index) / (samples.length - 1));
		const angle = (FULL_CYCLE * frequency * index) / rate;
		real += window * sample * Math.cos(angle);
		imaginary += window * sample * Math.sin(angle);
		weight += window;
	}
	return (real * real + imaginary * imaginary) / (weight * weight);
}

export function peakToneRatio(
	samples: readonly number[],
	rate: number,
	frequency: number,
): number {
	let peak = 0;
	const size = Math.min(MAX_ANALYSIS_SAMPLES, samples.length);
	if (size < MIN_ANALYSIS_SAMPLES) return 0;
	for (
		let offset = 0;
		offset + size <= samples.length;
		offset += Math.floor(size / WINDOW_OVERLAP_DIVISOR)
	) {
		const window = samples.slice(offset, offset + size);
		const power = tonePower(window, rate, frequency);
		if (power > MIN_SIGNAL_POWER)
			peak = Math.max(
				peak,
				power /
					Math.max(tonePower(window, rate, NULL_FREQUENCY), MIN_NULL_POWER),
			);
	}
	return peak;
}

export function wavSamples(bytes: Uint8Array): {
	sampleRate: number;
	samples: number[];
} {
	const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
	const name = (position: number): string =>
		String.fromCharCode(...bytes.slice(position, position + FOURCC_BYTES));
	if (
		bytes.length < MIN_WAV_BYTES ||
		name(0) !== "RIFF" ||
		name(WAVE_FORMAT_OFFSET) !== "WAVE"
	)
		throw new Error("Invalid WAV header");
	let rate = 0;
	for (
		let offset = RIFF_HEADER_BYTES;
		offset + CHUNK_HEADER_BYTES <= bytes.length;
	) {
		const size = view.getUint32(offset + FOURCC_BYTES, true);
		const start = offset + CHUNK_HEADER_BYTES;
		const end = start + size;
		if (end > bytes.length) throw new Error("Truncated WAV chunk");
		if (name(offset) === "fmt ") {
			if (
				size < PCM_FORMAT_BYTES ||
				view.getUint16(start, true) !== 1 ||
				view.getUint16(start + CHANNELS_OFFSET, true) !== 1 ||
				view.getUint16(start + SAMPLE_BITS_OFFSET, true) !== PCM_SAMPLE_BITS
			)
				throw new Error("Unsupported WAV format");
			rate = view.getUint32(start + SAMPLE_RATE_OFFSET, true);
		}
		if (name(offset) === "data") {
			if (rate !== PCM_SAMPLE_RATE || size % PCM_SAMPLE_BYTES !== 0)
				throw new Error("Invalid telephony WAV data");
			return {
				sampleRate: rate,
				samples: Array.from(
					{ length: size / PCM_SAMPLE_BYTES },
					(_, index): number =>
						view.getInt16(start + index * PCM_SAMPLE_BYTES, true) /
						PCM_FULL_SCALE,
				),
			};
		}
		offset = end + (size % PCM_SAMPLE_BYTES);
	}
	throw new Error("Missing WAV samples");
}
