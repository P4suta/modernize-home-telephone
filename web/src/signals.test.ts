import { expect, test } from "vitest";
import { peakToneRatio, tonePower } from "../test/signals";

test("recording analysis includes short calls and refuses silence, unrelated tones, and insufficient samples", (): void => {
	for (const length of [800, 1600, 4096, 15000]) {
		const samples = Array.from(
			{ length },
			(_, index): number => 0.25 * Math.sin((2 * Math.PI * 400 * index) / 8000),
		);
		expect(peakToneRatio(samples, 8000, 400)).toBeGreaterThan(10);
		expect(peakToneRatio(samples, 8000, 500)).toBe(0);
		expect(
			peakToneRatio(
				samples.map((): number => 0),
				8000,
				400,
			),
		).toBe(0);
	}
	expect(peakToneRatio([1, 0, -1], 8000, 400)).toBe(0);
});

test("the audio measurement detects the known signal and rejects silence and a wrong frequency", (): void => {
	for (const rate of [8000, 48000]) {
		const samples = Array.from(
			{ length: 4096 },
			(_, index): number => 0.25 * Math.sin((2 * Math.PI * 500 * index) / rate),
		);
		expect(tonePower(samples, rate, 500)).toBeGreaterThan(0.001);
		expect(tonePower(samples, rate, 500)).toBeGreaterThan(
			100 * tonePower(samples, rate, 730),
		);
		expect(tonePower(samples, rate, 1000)).toBeLessThan(0.000001);
		expect(
			tonePower(
				samples.map((): number => 0),
				rate,
				500,
			),
		).toBe(0);
	}
});
