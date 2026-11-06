import AxeBuilder from "@axe-core/playwright";
import { type APIRequestContext, expect, type Page } from "@playwright/test";
import { peakToneRatio, tonePower, wavSamples } from "../signals";
import { test } from "./raw-audio";
import { calls, command, incoming } from "./support";

test("the synthetic microphone disables voice processing", async ({
	page,
}): Promise<void> => {
	await page.goto("/");
	const settings = await page.evaluate(
		async (): Promise<MediaTrackSettings> => {
			const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
			try {
				const track = stream.getAudioTracks()[0];
				if (!track)
					throw new Error("The synthetic microphone has no audio track");
				return track.getSettings();
			} finally {
				for (const track of stream.getTracks()) track.stop();
			}
		},
	);
	expect(settings.echoCancellation).toBe(false);
	expect(settings.noiseSuppression).toBe(false);
	expect(settings.autoGainControl).toBe(false);
});

async function hear(page: Page, frequency: number): Promise<void> {
	await expect
		.poll(
			async (): Promise<number> => {
				const frame = await page
					.locator('audio[aria-label="通話音声"]')
					.evaluate(
						async (audio): Promise<{ samples: number[]; rate: number }> => {
							if (
								!(audio instanceof HTMLAudioElement) ||
								!(audio.srcObject instanceof MediaStream)
							)
								return { samples: [], rate: 8000 };
							const context = new AudioContext();
							try {
								const source = context.createMediaStreamSource(audio.srcObject);
								const analyser = context.createAnalyser();
								analyser.fftSize = 2048;
								const sink = context.createGain();
								sink.gain.value = 0;
								source.connect(analyser);
								analyser.connect(sink);
								sink.connect(context.destination);
								await context.resume();
								for (let frame = 0; frame < 4; frame++)
									await new Promise<void>((resolve): number =>
										requestAnimationFrame((): void => resolve()),
									);
								const samples = new Float32Array(analyser.fftSize);
								analyser.getFloatTimeDomainData(samples);
								return {
									samples: Array.from(samples),
									rate: context.sampleRate,
								};
							} finally {
								await context.close();
							}
						},
					);
				const signal = tonePower(frame.samples, frame.rate, frequency);
				return signal > 0.000001
					? signal /
							Math.max(tonePower(frame.samples, frame.rate, 730), 0.000000001)
					: 0;
			},
			{ timeout: 8000 },
		)
		.toBeGreaterThan(10);
}
async function recordedTones(
	request: APIRequestContext,
	id: string,
	frequencies: readonly number[],
): Promise<void> {
	await expect
		.poll(
			async (): Promise<boolean> =>
				(await calls(request)).some(
					(call): boolean =>
						call.id === id &&
						call.phase === "ended" &&
						call.recording.status === "available" &&
						call.processing === "complete",
				),
			{ timeout: 10000 },
		)
		.toBe(true);
	const response = await request.get(`/api/calls/${id}/recording`);
	expect(response.ok()).toBe(true);
	const recording = wavSamples(await response.body());
	for (const frequency of frequencies) {
		expect(
			peakToneRatio(recording.samples, recording.sampleRate, frequency),
			`recorded ${frequency} Hz versus the 730 Hz null`,
		).toBeGreaterThan(10);
	}
}

async function quiet(page: Page, frequency: number): Promise<void> {
	await expect
		.poll(
			async (): Promise<number> => {
				const observation = await page
					.locator('audio[aria-label="通話音声"]')
					.evaluate(
						async (audio): Promise<{ frames: number[][]; rate: number }> => {
							if (
								!(audio instanceof HTMLAudioElement) ||
								!(audio.srcObject instanceof MediaStream)
							)
								throw new Error("The received media stream disappeared");
							const context = new AudioContext();
							try {
								const source = context.createMediaStreamSource(audio.srcObject);
								const analyser = context.createAnalyser();
								analyser.fftSize = 2048;
								const sink = context.createGain();
								sink.gain.value = 0;
								source.connect(analyser);
								analyser.connect(sink);
								sink.connect(context.destination);
								await context.resume();
								const frames: number[][] = [];
								for (let index = 0; index < 12; index++) {
									await new Promise<void>((resolve) =>
										setTimeout(resolve, 100),
									);
									const samples = new Float32Array(analyser.fftSize);
									analyser.getFloatTimeDomainData(samples);
									frames.push(Array.from(samples));
								}
								return { frames, rate: context.sampleRate };
							} finally {
								await context.close();
							}
						},
					);
				return Math.max(
					...observation.frames.map((frame) =>
						tonePower(frame, observation.rate, frequency),
					),
				);
			},
			{ timeout: 8000 },
		)
		.toBeLessThan(0.000001);
}
test.beforeEach(async ({ request }): Promise<void> => {
	for (const call of await calls(request))
		if (call.phase !== "ended") await command(request, call.id, "hangup");
});

test("the workspace is accessible, monochrome, and fits a narrow screen with recording history", async ({
	page,
	request,
}): Promise<void> => {
	const id = await incoming(request, "silent");
	await expect
		.poll(
			async (): Promise<string | undefined> =>
				(await calls(request)).find((call): boolean => call.id === id)?.phase,
		)
		.toBe("screening");
	await command(request, id, "hangup");
	await expect
		.poll(
			async (): Promise<string | undefined> =>
				(await calls(request)).find((call): boolean => call.id === id)
					?.recording.status,
			{ timeout: 10000 },
		)
		.toBe("available");
	await page.goto("/");
	await expect(page.getByRole("status", { name: "接続状態" })).toContainText(
		"接続済み",
	);
	await expect(page.getByRole("heading", { level: 1 })).toHaveText(
		"Home Telephone",
	);
	expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
	const colors = await page.evaluate((): string[] =>
		Array.from(
			new Set(
				Array.from(document.querySelectorAll("*")).flatMap((element) => {
					const style = getComputedStyle(element);
					return [style.color, style.backgroundColor, style.borderTopColor];
				}),
			),
		),
	);
	expect(
		colors.filter(
			(color) =>
				!["rgb(0, 0, 0)", "rgb(255, 255, 255)", "rgba(0, 0, 0, 0)"].includes(
					color,
				),
		),
	).toEqual([]);
	await page.screenshot({
		path: "test-results/workspace-desktop.png",
		fullPage: true,
	});
	await page.setViewportSize({ width: 390, height: 844 });
	expect(
		await page.evaluate(
			(): boolean => document.documentElement.scrollWidth <= innerWidth,
		),
	).toBe(true);
	await page.screenshot({
		path: "test-results/workspace-mobile.png",
		fullPage: true,
	});
});

test("real WebRTC carries known audio through takeover, hold, read aloud, and the saved recording", async ({
	page,
	request,
}): Promise<void> => {
	await page.goto("/");
	await expect(page.getByRole("status", { name: "接続状態" })).toContainText(
		"接続済み",
	);
	await page
		.getByRole("button", { name: "ブラウザー通話を有効にする", exact: true })
		.click();
	await expect(
		page.getByText("ブラウザーで応答できます。", { exact: true }),
	).toBeVisible();
	const id = await incoming(request, "silent");
	await expect(page.locator(".active-call")).toContainText("AI受付中");
	await page.getByRole("button", { name: "ブラウザーへ引き継ぐ" }).click();
	await expect(page.locator(".active-call")).toContainText("通話中");
	await expect(
		page.getByText("マイクと音声を接続しています。", { exact: true }),
	).toBeVisible();
	await hear(page, 500);
	await page
		.getByRole("button", { name: "マイクをミュートする", exact: true })
		.click();
	await expect(
		page.getByRole("button", { name: "マイクのミュートを解除する" }),
	).toHaveAttribute("aria-pressed", "true");
	await page
		.getByRole("button", { name: "マイクのミュートを解除する" })
		.click();
	await page.getByRole("button", { name: "保留する", exact: true }).click();
	await expect(page.locator(".active-call")).toContainText("保留中");
	await page
		.getByRole("button", { name: "通話を再開する", exact: true })
		.click();
	await expect(page.locator(".active-call")).toContainText("通話中");
	await hear(page, 500);
	await page.getByLabel("プッシュ信号", { exact: true }).fill("123#");
	await page.getByRole("button", { name: "プッシュ信号を送る" }).click();
	await page.getByLabel("相手へ読み上げる文章").fill("試験通話です。");
	await page
		.getByRole("button", { name: "文章を読み上げる", exact: true })
		.click();
	await expect(page.getByRole("alert")).toHaveCount(0);
	await hear(page, 1000);
	await page.getByRole("button", { name: "電話を切る", exact: true }).click();
	await expect(page.locator(".active-call")).toHaveCount(0);
	await recordedTones(request, id, [400, 500, 1000]);
	await page
		.getByRole("button", { name: "ブラウザー通話を無効にする", exact: true })
		.click();
});

test("outgoing WebRTC completes a SIPp dialog and returns the browser audio through RTP echo", async ({
	page,
	request,
}): Promise<void> => {
	const previous = new Set(
		(await calls(request)).map((call): string => call.id),
	);
	await page.goto("/");
	await page
		.getByRole("button", { name: "ブラウザー通話を有効にする", exact: true })
		.click();
	await expect(
		page.getByText("ブラウザーで応答できます。", { exact: true }),
	).toBeVisible();
	await page.getByLabel("電話番号", { exact: true }).fill("0312345678");
	await page.getByRole("button", { name: "発信する", exact: true }).click();
	await expect(page.locator(".active-call")).toContainText("通話中");
	const call = (await calls(request)).find(
		(call): boolean => !previous.has(call.id),
	);
	expect(call?.direction).toBe("outbound");
	if (!call) throw new Error("The outbound call was not admitted");
	await hear(page, 400);
	await page
		.getByRole("button", { name: "マイクをミュートする", exact: true })
		.click();
	await quiet(page, 400);
	await page
		.getByRole("button", { name: "マイクのミュートを解除する", exact: true })
		.click();
	await hear(page, 400);
	await page.getByRole("button", { name: "保留する", exact: true }).click();
	await expect(page.locator(".active-call")).toContainText("保留中");
	await quiet(page, 400);
	await page
		.getByRole("button", { name: "通話を再開する", exact: true })
		.click();
	await hear(page, 400);
	await expect
		.poll(
			async (): Promise<number> =>
				page
					.locator('audio[aria-label="通話音声"]')
					.evaluate((audio): number =>
						audio instanceof HTMLAudioElement ? audio.currentTime : 0,
					),
		)
		.toBeGreaterThan(0.5);
	await page.getByRole("button", { name: "電話を切る", exact: true }).click();
	await expect(page.locator(".active-call")).toHaveCount(0);
	await recordedTones(request, call.id, [400]);
	await page
		.getByRole("button", { name: "ブラウザー通話を無効にする", exact: true })
		.click();
});

for (const scenario of ["spam", "suspected_spam"]) {
	test(`${scenario} is disconnected and recorded without a paid provider`, async ({
		request,
	}): Promise<void> => {
		const id = await incoming(request, scenario);
		await expect
			.poll(
				async (): Promise<boolean> =>
					(await calls(request)).some(
						(call): boolean =>
							call.id === id &&
							call.reason === "ai_spam" &&
							call.recording.status === "available" &&
							call.processing === "complete",
					),
				{ timeout: 15000 },
			)
			.toBe(true);
	});
}
