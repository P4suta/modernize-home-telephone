import { expect, type WebSocketRoute } from "@playwright/test";
import { test } from "./raw-audio";
import {
	calls,
	command,
	completed,
	incoming,
	origin,
	saveSettings,
	settings,
} from "./support";

const FOREIGN_ORIGIN = "https://foreign.invalid";

test.beforeEach(async ({ request }): Promise<void> => {
	for (const call of await calls(request))
		if (call.phase !== "ended") await command(request, call.id, "hangup");
});

test("settings validate input, persist across reload, and remove number rules", async ({
	page,
	request,
}): Promise<void> => {
	const original = await settings(request);
	try {
		await page.goto("/");
		await page.getByRole("button", { name: "設定を読み込む" }).click();
		await page.getByLabel("履歴と録音の保存日数").fill("0");
		await page.getByRole("button", { name: "設定を保存する" }).click();
		expect(
			await page
				.getByLabel("履歴と録音の保存日数")
				.evaluate(
					(input): boolean =>
						input instanceof HTMLInputElement && input.validity.rangeUnderflow,
				),
		).toBe(true);
		expect(await settings(request)).toEqual(original);
		await page.getByLabel("履歴と録音の保存日数").fill("31");
		await page.getByLabel("録音の保存容量（GiB）").fill("4");
		await page.getByLabel("電話番号（非通知はanonymous）").fill("999001");
		await page.getByLabel("応答方法").selectOption("allow");
		await page.getByRole("button", { name: "番号を追加する" }).click();
		await page.getByRole("button", { name: "設定を保存する" }).click();
		await expect(
			page.getByText("設定を保存しました。", { exact: true }),
		).toBeVisible();
		await page.reload();
		await page.getByRole("button", { name: "設定を読み込む" }).click();
		await expect(page.getByLabel("履歴と録音の保存日数")).toHaveValue("31");
		await expect(
			page.getByText("999001 · 直接呼び出す", { exact: true }),
		).toBeVisible();
		await page
			.getByRole("button", { name: "999001の応答設定を削除する" })
			.click();
		await page.getByRole("button", { name: "設定を保存する" }).click();
		await expect
			.poll(
				async (): Promise<boolean> =>
					(await settings(request)).rules["999001"] === undefined,
			)
			.toBe(true);
	} finally {
		await saveSettings(request, original);
	}
});

test("an allow rule bypasses screening and a browser answers and releases the line", async ({
	page,
	request,
}): Promise<void> => {
	const original = await settings(request);
	try {
		await saveSettings(request, {
			...original,
			rules: { ...original.rules, "999001": "allow" },
		});
		await page.goto("/");
		await page
			.getByRole("button", { name: "ブラウザー通話を有効にする", exact: true })
			.click();
		await expect(
			page.getByText("ブラウザーで応答できます。", { exact: true }),
		).toBeVisible();
		const id = await incoming(request, "spam");
		await expect(page.locator(".active-call")).toContainText("呼び出し中");
		await page.getByRole("button", { name: "応答する", exact: true }).click();
		await expect(page.locator(".active-call")).toContainText("通話中");
		await page.getByRole("button", { name: "電話を切る", exact: true }).click();
		await completed(request, id);
		expect(
			(await calls(request)).find((call) => call.id === id)?.reason,
		).not.toBe("ai_spam");
		await page
			.getByRole("button", { name: "ブラウザー通話を無効にする", exact: true })
			.click();
	} finally {
		await saveSettings(request, original);
	}
});

test("a block rule rejects a human caller without recording and screen restores AI routing", async ({
	request,
}): Promise<void> => {
	const original = await settings(request);
	try {
		await saveSettings(request, {
			...original,
			rules: { ...original.rules, "999003": "block", "999001": "screen" },
		});
		const blocked = await incoming(request, "human");
		await expect
			.poll(
				async (): Promise<string | null | undefined> =>
					(await calls(request)).find((call) => call.id === blocked)?.reason,
			)
			.toBe("number_blocked");
		expect(
			(await calls(request)).find((call) => call.id === blocked)?.recording
				.status,
		).toBe("none");
		expect(
			(await request.get(`/api/calls/${blocked}/recording`)).status(),
		).toBe(404);
		const screened = await incoming(request, "spam");
		await completed(request, screened);
		expect(
			(await calls(request)).find((call) => call.id === screened)?.reason,
		).toBe("ai_spam");
	} finally {
		await saveSettings(request, original);
	}
});

test("recordings play and seek, transcript failures retry, and confirmed deletion persists", async ({
	page,
	request,
}): Promise<void> => {
	const id = await incoming(request, "spam");
	await completed(request, id);
	let detailRequests = 0;
	await page.route(`**/api/calls/${id}`, async (route) => {
		detailRequests += 1;
		if (detailRequests === 1)
			await route.fulfill({ status: 503, body: "unavailable" });
		else await route.continue();
	});
	await page.goto("/");
	const history = page
		.locator(".history-call")
		.filter({ has: page.locator(`audio[src="/api/calls/${id}/recording"]`) });
	await expect(history).toHaveCount(1);
	expect(detailRequests).toBe(0);
	await history.getByText("文字起こしを表示", { exact: true }).click();
	await expect(history.getByRole("alert")).toHaveText(
		"文字起こしを読み込めませんでした。",
	);
	await history.getByRole("button", { name: "再読み込み" }).click();
	await expect(history.getByRole("alert")).toHaveCount(0);
	await expect(history.locator("details strong").first()).toBeVisible();
	expect(detailRequests).toBe(2);
	await history
		.getByRole("button", { name: "録音を再生", exact: true })
		.click();
	await expect(
		history.getByRole("button", { name: "録音を一時停止" }),
	).toBeVisible();
	await expect
		.poll(
			async (): Promise<number> =>
				history
					.locator("audio")
					.evaluate((audio): number =>
						audio instanceof HTMLAudioElement ? audio.currentTime : 0,
					),
		)
		.toBeGreaterThan(0);
	await history.getByRole("button", { name: "録音を一時停止" }).click();
	await history.getByLabel("再生位置").fill("0.1");
	await expect
		.poll(
			async (): Promise<number> =>
				history
					.locator("audio")
					.evaluate((audio): number =>
						audio instanceof HTMLAudioElement ? audio.currentTime : 0,
					),
		)
		.toBeCloseTo(0.1, 1);
	const range = await request.get(`/api/calls/${id}/recording`, {
		headers: { Range: "bytes=0-43" },
	});
	expect(range.status()).toBe(206);
	expect((await range.body()).length).toBe(44);
	expect(range.headers()["cache-control"]).toBe("no-store");
	await history
		.getByRole("button", { name: "履歴を削除する", exact: true })
		.click();
	await history.getByRole("button", { name: "削除を取り消す" }).click();
	expect((await request.get(`/api/calls/${id}`)).status()).toBe(200);
	await history
		.getByRole("button", { name: "履歴を削除する", exact: true })
		.click();
	await history.getByRole("button", { name: "履歴の削除を実行する" }).click();
	await expect(history).toHaveCount(0);
	expect((await request.get(`/api/calls/${id}`)).status()).toBe(404);
	expect((await request.get(`/api/calls/${id}/recording`)).status()).toBe(404);
	await page.reload();
	await expect(
		page.locator(`audio[src="/api/calls/${id}/recording"]`),
	).toHaveCount(0);
});

test("SSE reconnect converges after missed call and deletion events without duplicate history", async ({
	page,
	context,
	request,
}): Promise<void> => {
	await page.goto("/");
	await expect(page.getByRole("status", { name: "接続状態" })).toContainText(
		"仮想回線・模擬AI",
	);
	await context.setOffline(true);
	await expect(page.getByRole("status", { name: "接続状態" })).toContainText(
		"接続待ち",
		{ timeout: 20000 },
	);
	const id = await incoming(request, "spam");
	await completed(request, id);
	await context.setOffline(false);
	const audio = page.locator(`audio[src="/api/calls/${id}/recording"]`);
	await expect(audio).toHaveCount(1, { timeout: 15000 });
	await context.setOffline(true);
	const deletion = await request.delete(`/api/calls/${id}`, {
		headers: { Origin: origin },
	});
	expect(deletion.status()).toBe(204);
	await context.setOffline(false);
	await expect(audio).toHaveCount(0, { timeout: 15000 });
});

test("SIP transport loss permits explicit reconnection and an incoming call", async ({
	page,
	request,
}): Promise<void> => {
	let transport: WebSocketRoute | undefined;
	await page.routeWebSocket("**/sip", (route) => {
		transport = route;
		route.connectToServer();
	});
	await page.goto("/");
	await page
		.getByRole("button", { name: "ブラウザー通話を有効にする", exact: true })
		.click();
	await expect(
		page.getByText("ブラウザーで応答できます。", { exact: true }),
	).toBeVisible();
	expect(transport).toBeDefined();
	await transport?.close({ code: 1011, reason: "Injected signaling loss" });
	await expect(
		page.getByText("電話への接続に失敗しました。再接続してください。", {
			exact: true,
		}),
	).toBeVisible();
	await page
		.getByRole("button", { name: "ブラウザー通話を有効にする", exact: true })
		.click();
	await expect(
		page.getByText("ブラウザーで応答できます。", { exact: true }),
	).toBeVisible();
	const id = await incoming(request, "human");
	await page.getByRole("button", { name: "応答する", exact: true }).click();
	await expect(page.locator(".active-call")).toContainText("通話中");
	await page.getByRole("button", { name: "電話を切る", exact: true }).click();
	await completed(request, id);
});

test("active calls reject deletion, competing origination, invalid commands, and foreign origins", async ({
	request,
}): Promise<void> => {
	const id = await incoming(request, "silent");
	await expect
		.poll(
			async (): Promise<string | undefined> =>
				(await calls(request)).find((call) => call.id === id)?.phase,
		)
		.toBe("screening");
	expect(
		(
			await request.delete(`/api/calls/${id}`, { headers: { Origin: origin } })
		).status(),
	).toBe(409);
	expect(
		(
			await request.post("/api/calls", {
				headers: { Origin: origin },
				data: { number: "0312345678" },
			})
		).status(),
	).toBe(409);
	expect(
		(
			await request.post(`/api/calls/${id}/commands`, {
				headers: { Origin: origin },
				data: { action: "resume" },
			})
		).status(),
	).toBe(409);
	expect(
		(
			await request.post(`/api/calls/${id}/commands`, {
				headers: { Origin: FOREIGN_ORIGIN },
				data: { action: "hangup" },
			})
		).status(),
	).toBe(403);
	expect((await calls(request)).find((call) => call.id === id)?.phase).toBe(
		"screening",
	);
	await command(request, id, "hangup");
	expect(
		(
			await request.post(`/api/calls/${id}/commands`, {
				headers: { Origin: origin },
				data: { action: "hold" },
			})
		).status(),
	).toBe(404);
});

test("message classification enters voicemail and its deadline saves audio and transcript", async ({
	page,
	request,
}): Promise<void> => {
	test.setTimeout(90000);
	await page.goto("/");
	const id = await incoming(request, "message");
	await expect(page.locator(".active-call")).toContainText("伝言を録音中");
	await expect
		.poll(
			async (): Promise<string | undefined> =>
				(await calls(request)).find((call) => call.id === id)?.phase,
			{ timeout: 70000 },
		)
		.toBe("ended");
	await completed(request, id);
	await expect(
		page.locator(`audio[src="/api/calls/${id}/recording"]`),
	).toHaveCount(1);
	const detail = await request.get(`/api/calls/${id}`);
	const value: unknown = await detail.json();
	expect(value).toMatchObject({
		phase: "ended",
		reason: "voicemail_complete",
		processing: "complete",
	});
	expect(value).toHaveProperty("transcript");
});
