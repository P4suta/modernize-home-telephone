import {
	createServer,
	type IncomingMessage,
	type ServerResponse,
} from "node:http";
import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";
import { statusSchema } from "../../src/contracts";
import { saveSettings, settings } from "./support";

test("NAS settings retain audio manually and explain a bounded local spool at narrow and enlarged sizes", async ({
	page,
	request,
}): Promise<void> => {
	const original = await settings(request);
	const status = statusSchema.parse(
		await (await request.get("/api/status")).json(),
	);
	const nasStatus = statusSchema.parse({
		...status,
		recording: {
			enabled: true,
			destination: "nas",
			max_seconds: 7200,
			minimum_bytes: 460816384,
		},
	});
	const fixture = createServer(
		(_request: IncomingMessage, response: ServerResponse): void => {
			response.writeHead(200, {
				"Content-Type": "text/event-stream",
				"Cache-Control": "no-store",
				"Access-Control-Allow-Origin": "http://localhost:8787",
			});
			response.write(`event: status\ndata: ${JSON.stringify(nasStatus)}\n\n`);
		},
	);
	await new Promise<void>((resolve, reject): void => {
		fixture.once("error", reject);
		fixture.listen(0, "127.0.0.1", resolve);
	});
	const address = fixture.address();
	if (typeof address !== "object" || address === null)
		throw new Error("The status fixture has no local address");
	try {
		await saveSettings(request, {
			...original,
			retention_days: 0,
			recording_limit_bytes: 536870912,
		});
		await page.route("**/api/status", async (route): Promise<void> => {
			await route.fulfill({ json: nasStatus });
		});
		await page.route("**/api/events", async (route): Promise<void> => {
			await route.continue({
				url: `http://127.0.0.1:${address.port}/api/events`,
			});
		});
		await page.goto("/");
		await page.getByRole("button", { name: "設定を読み込む" }).click();
		await expect(
			page.getByText("手動で削除するまで保存", { exact: true }),
		).toBeVisible();
		await expect(page.locator("#recording-storage-help")).not.toBeVisible();
		await expect(page.getByText("120分", { exact: true })).toBeVisible();
		await expect(page.getByLabel("履歴と録音の保存日数")).toHaveCount(0);
		const quota = page.getByLabel("本体の一時保存上限（GiB）");
		await expect(quota).toHaveValue("0.5");
		await quota.fill("0.429");
		await page.getByRole("button", { name: "設定を保存する" }).click();
		expect(
			await quota.evaluate(
				(input): boolean =>
					input instanceof HTMLInputElement && input.validity.rangeUnderflow,
			),
		).toBe(true);
		expect((await settings(request)).recording_limit_bytes).toBe(536870912);
		await quota.fill("0.5");
		await page.getByRole("button", { name: "設定を保存する" }).click();
		await expect(
			page.getByText("設定を保存しました。", { exact: true }),
		).toBeVisible();
		expect((await settings(request)).retention_days).toBe(0);
		await page.screenshot({
			path: "test-results/nas-settings-desktop.png",
			fullPage: true,
		});
		for (const width of [1280, 390]) {
			await page.setViewportSize({ width, height: 844 });
			const help = page.getByRole("button", {
				name: "一時保存上限の説明",
				exact: true,
			});
			await help.focus();
			await page.keyboard.press("Enter");
			await expect(
				page.getByRole("heading", { name: "一時保存上限", exact: true }),
			).toBeVisible();
			expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
			await page.keyboard.press("Escape");
			await expect(page.locator("#recording-storage-help")).not.toBeVisible();
			await expect(help).toBeFocused();
			expect(
				await page.evaluate(
					(): boolean => document.documentElement.scrollWidth <= innerWidth,
				),
			).toBe(true);
		}
		await page.screenshot({
			path: "test-results/nas-settings-narrow.png",
			fullPage: true,
		});
		await page.addStyleTag({
			content:
				"html {font-size:200%} * {line-height:1.5 !important;letter-spacing:.12em !important;word-spacing:.16em !important} p {margin-bottom:2em !important}",
		});
		expect(
			await page.evaluate(
				(): boolean => document.documentElement.scrollWidth <= innerWidth,
			),
		).toBe(true);
		expect(
			await page.evaluate((): { tag: string; name: string; right: number }[] =>
				Array.from(document.querySelectorAll("*"))
					.filter(
						(element): boolean =>
							element.getClientRects().length > 0 &&
							element.getBoundingClientRect().right > innerWidth + 1,
					)
					.map((element) => ({
						tag: element.tagName,
						name: element.getAttribute("class") ?? element.id,
						right: element.getBoundingClientRect().right,
					})),
			),
		).toEqual([]);
		await page
			.getByRole("button", { name: "一時保存上限の説明", exact: true })
			.click();
		await expect(
			page.getByRole("button", { name: "× 閉じる", exact: true }),
		).toBeVisible();
		await page.screenshot({ path: "test-results/nas-settings-enlarged.png" });
		await page.getByRole("button", { name: "× 閉じる", exact: true }).click();
		await expect(page.locator("#recording-storage-help")).not.toBeVisible();
	} finally {
		await page.close();
		await new Promise<void>((resolve, reject): void => {
			fixture.close((error): void => {
				if (error) reject(error);
				else resolve();
			});
			fixture.closeAllConnections();
		});
		await saveSettings(request, original);
	}
});
