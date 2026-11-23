import AxeBuilder from "@axe-core/playwright";
import { expect, type Page, test } from "@playwright/test";
import { snapshotSchema } from "../../src/lib/api";
import { catalogue } from "../../src/lib/documentation";

const origin = "http://127.0.0.1:8791";
async function control(page: Page, data: Record<string, unknown>): Promise<void> {
	const response = await page.request.post("/__fixture", { data });
	expect(response.status()).toBe(204);
}
async function login(page: Page): Promise<void> {
	await page.goto("/");
	await page.getByLabel("パスワード", { exact: true }).fill("fixture-console");
	await page.getByRole("button", { name: "ログイン", exact: true }).click();
	await expect(page.getByRole("heading", { name: "機器情報", exact: true })).toBeVisible();
}
async function fxs(page: Page): Promise<void> {
	await page.goto("/fxs/");
	await expect(page.getByRole("textbox", { name: "ローカルRTPポート", exact: false })).toHaveValue(
		"5004",
	);
}
async function statistics(page: Page): Promise<{ writes: number; reads: number; logins: number }> {
	const response = await page.request.get("/__fixture");
	const value: unknown = await response.json();
	if (
		!value ||
		typeof value !== "object" ||
		!("writes" in value) ||
		typeof value.writes !== "number" ||
		!("reads" in value) ||
		typeof value.reads !== "number" ||
		!("logins" in value) ||
		typeof value.logins !== "number"
	)
		throw new Error("Invalid fixture statistics");
	return { writes: value.writes, reads: value.reads, logins: value.logins };
}
test.beforeEach(async ({ page }) => {
	await control(page, { operation: "reset" });
});
test.afterEach(async ({ page }) => {
	await page.request.delete("/api/console/session", { headers: { Origin: origin } });
});

test("login errors preserve the account field and never expose the password", async ({ page }) => {
	await page.goto("/");
	await page.getByLabel("パスワード", { exact: true }).fill("incorrect");
	await page.getByRole("button", { name: "ログイン", exact: true }).click();
	await expect(page.getByRole("alert")).toBeVisible();
	await expect(page.getByLabel("ユーザー名")).toHaveValue("admin");
	await expect(page.getByLabel("パスワード", { exact: true })).toHaveAttribute("type", "password");
	expect((await statistics(page)).writes).toBe(0);
});
test("navigation retains drafts and cached pages without another device read", async ({ page }) => {
	await login(page);
	await fxs(page);
	await page.getByRole("textbox", { name: "ローカルRTPポート", exact: false }).fill("5014");
	await page.getByRole("link", { name: "基本設定", exact: true }).click();
	await expect(page.getByRole("combobox", { name: "タイムゾーン", exact: false })).toHaveValue(
		"UTC",
	);
	const before = await statistics(page);
	await page.getByRole("link", { name: "電話機（FXS）", exact: true }).click();
	await expect(page.getByRole("textbox", { name: "ローカルRTPポート", exact: false })).toHaveValue(
		"5014",
	);
	await expect(page.getByText("未保存の変更 1件", { exact: true })).toBeVisible();
	expect((await statistics(page)).reads).toBe(before.reads);
});
test("contextual help supports keyboard dismissal and links to the shared reference", async ({
	page,
}) => {
	await login(page);
	await fxs(page);
	const trigger = page.getByRole("button", { name: "ローカルRTPポート（FXS）の説明", exact: true });
	await trigger.click();
	const help = page.getByRole("complementary", { name: "ローカルRTPポート（FXS）", exact: true });
	await expect(help).toBeVisible();
	await expect(help.getByText("工場出荷時の値（資料）", { exact: true })).toBeVisible();
	await expect(help.getByRole("link", { name: "詳しい説明を読む" })).toHaveAttribute(
		"href",
		/docs.*q=P39.*#parameter-p39/,
	);
	await page.keyboard.press("Escape");
	await expect(help).not.toBeVisible();
	await expect(trigger).toBeFocused();
});
test("invalid input cannot open review or submit a native write", async ({ page }, info) => {
	const errors: string[] = [];
	let submissions = 0;
	page.on("pageerror", (error): void => {
		errors.push(error.message);
	});
	page.on("request", (request): void => {
		if (request.method() === "POST" && new URL(request.url()).pathname === "/api/console/pages/fxs")
			submissions += 1;
	});
	await page.route("**/api/console/pages/fxs", async (route) => {
		if (route.request().method() !== "GET") {
			await route.continue();
			return;
		}
		const response = await route.fetch();
		const snapshot = snapshotSchema.parse(await response.json());
		const basis = snapshot.settings[0]?.field;
		const help = catalogue.find((entry) => entry.parameters.includes("P34"));
		if (!basis || !help) throw new Error("Missing password fixture metadata");
		snapshot.settings.push({
			field: {
				...basis,
				id: "password-fixture",
				parameter: "P34",
				label: help.title,
				kind: "password",
				secret: true,
				values: [],
				choices: [],
				disabled: false,
				read_only: false,
				multiple: false,
				required: false,
				max_length: null,
				min: null,
				max: null,
			},
			help,
		});
		await route.fulfill({ response, json: snapshot });
	});
	await login(page);
	await fxs(page);
	await page.getByRole("textbox", { name: "ローカルRTPポート", exact: false }).fill("1023");
	await page.getByRole("button", { name: "変更を確認", exact: true }).click();
	await expect(page.getByRole("alert")).toContainText("入力値を確認");
	await expect(page.getByRole("dialog")).not.toBeVisible();
	expect((await statistics(page)).writes).toBe(0);
	await page.getByRole("textbox", { name: "ローカルRTPポート", exact: false }).fill("5004");
	const password = page.locator("#password-fixture");
	await password.fill("synthetic-password-fixture");
	await password.fill("");
	await expect(password).toHaveValue("");
	await page.getByRole("button", { name: "変更を確認", exact: true }).click();
	await expect(page.getByRole("alert")).toContainText("入力値を確認");
	await expect(page.getByRole("dialog")).not.toBeVisible();
	await expect(password).toHaveAttribute("aria-invalid", "true");
	await password
		.locator("..")
		.screenshot({ path: info.outputPath("empty-password-validation.png") });
	await password.fill("synthetic-password-fixture");
	await page.getByRole("button", { name: "変更を確認", exact: true }).click();
	await expect(page.getByRole("dialog")).toContainText("変更あり");
	await expect(page.getByRole("dialog")).not.toContainText("synthetic-password-fixture");
	await page.keyboard.press("Escape");
	expect(errors).toEqual([]);
	expect(submissions).toBe(0);
	expect((await statistics(page)).writes).toBe(0);
});
test("review cancellation and a completed save preserve the single submission contract", async ({
	page,
}) => {
	await login(page);
	await fxs(page);
	await page.getByRole("textbox", { name: "ローカルRTPポート", exact: false }).fill("5014");
	await page.getByRole("button", { name: "変更を確認", exact: true }).click();
	await expect(page.getByRole("dialog")).toBeVisible();
	await page.keyboard.press("Escape");
	await expect(page.getByRole("dialog")).not.toBeVisible();
	expect((await statistics(page)).writes).toBe(0);
	await page.getByRole("button", { name: "変更を確認", exact: true }).click();
	await page.getByRole("button", { name: "保存", exact: true }).click();
	await expect(page.getByText("未保存の変更はありません", { exact: true })).toBeVisible();
	await expect(page.getByRole("textbox", { name: "ローカルRTPポート", exact: false })).toHaveValue(
		"5014",
	);
	expect((await statistics(page)).writes).toBe(1);
});
test("expired native authentication recovers a read while retaining edits", async ({ page }) => {
	await login(page);
	await fxs(page);
	await page.getByRole("textbox", { name: "ローカルRTPポート", exact: false }).fill("5014");
	const before = await statistics(page);
	await control(page, { operation: "expire" });
	await page.getByRole("button", { name: "再取得", exact: true }).click();
	await expect(page.getByText("機器の認証を回復しました。", { exact: false })).toBeVisible();
	await expect(page.getByRole("textbox", { name: "ローカルRTPポート", exact: false })).toHaveValue(
		"5014",
	);
	const after = await statistics(page);
	expect(after.logins).toBe(before.logins + 1);
	expect(after.writes).toBe(0);
});
test("a stale preflight refreshes the comparison without submitting the draft", async ({
	page,
}) => {
	await login(page);
	await fxs(page);
	await page.getByRole("textbox", { name: "ローカルRTPポート", exact: false }).fill("5014");
	await page.getByRole("button", { name: "変更を確認", exact: true }).click();
	await control(page, { operation: "set", parameter: "P39", value: "5006" });
	await page.getByRole("button", { name: "保存", exact: true }).click();
	await page.getByRole("button", { name: "閉じる", exact: true }).click();
	await expect(
		page.getByRole("heading", { name: "機器の値が変わりました", exact: true }),
	).toBeVisible();
	expect((await statistics(page)).writes).toBe(0);
	await page.getByRole("button", { name: "最新値と編集内容を比較する", exact: true }).click();
	await expect(page.getByRole("textbox", { name: "ローカルRTPポート", exact: false })).toHaveValue(
		"5014",
	);
	await page.getByRole("button", { name: "変更を確認", exact: true }).click();
	await expect(page.getByRole("table")).toContainText("5006");
});
test("an uncertain native response is shown without a second write", async ({ page }) => {
	await login(page);
	await fxs(page);
	await page.getByRole("textbox", { name: "ローカルRTPポート", exact: false }).fill("5014");
	await control(page, { operation: "fail_after_write" });
	await page.getByRole("button", { name: "変更を確認", exact: true }).click();
	await page.getByRole("button", { name: "保存", exact: true }).click();
	await expect(page.getByText("保存結果を確定できません。", { exact: false })).toBeVisible();
	expect((await statistics(page)).writes).toBe(1);
});
test("documentation searches codec names and parameter identifiers without a session", async ({
	page,
}) => {
	await page.goto("/docs/");
	await page.getByRole("searchbox").fill("pcmu");
	await expect(
		page.getByRole("heading", { name: "音声コーデックの優先順位1（FXS）", exact: true }),
	).toBeVisible();
	await page.getByRole("searchbox").fill("P739");
	await expect(
		page.getByRole("heading", { name: "ローカルRTPポート（FXO）", exact: true }),
	).toBeVisible();
	await expect(page.locator("article")).toHaveCount(1);
	await page.getByRole("searchbox").fill("存在しない検索語");
	await expect(page.getByText("一致する説明がありません。", { exact: false })).toBeVisible();
});
test("every page meets automatic accessibility and narrow layout checks", async ({
	page,
}, info) => {
	const errors: string[] = [];
	page.on("pageerror", (error) => errors.push(error.message));
	await login(page);
	for (const route of ["status", "basic", "advanced", "fxs", "fxo", "docs"]) {
		await page.goto(`/${route}/`);
		if (route !== "docs")
			await expect(page.getByRole("button", { name: "再取得", exact: true })).toBeEnabled();
		const results = await new AxeBuilder({ page })
			.withTags(["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"])
			.analyze();
		expect(results.violations).toEqual([]);
		for (const width of [390, 320]) {
			await page.setViewportSize({ width, height: 844 });
			expect(
				await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth),
			).toBe(true);
		}
		await page.screenshot({ path: info.outputPath(`${route}-narrow.png`), fullPage: true });
		await page.screenshot({ path: info.outputPath(`${route}-narrow-viewport.png`) });
		await page.setViewportSize({ width: 1440, height: 1000 });
		await page.screenshot({ path: info.outputPath(`${route}-desktop.png`) });
	}
	expect(errors).toEqual([]);
});

test("browser reauthentication fetches the current view before rebasing an existing draft", async ({
	page,
}) => {
	await login(page);
	await fxs(page);
	await page.getByRole("textbox", { name: "ローカルRTPポート", exact: false }).fill("5014");
	await page.request.delete("/api/console/session", { headers: { Origin: origin } });
	await control(page, { operation: "set", parameter: "P39", value: "5006" });
	await page.getByRole("button", { name: "再取得", exact: true }).click();
	await expect(
		page.getByRole("heading", { name: "ログインして編集を再開", exact: true }),
	).toBeVisible();
	await page.getByLabel("パスワード", { exact: true }).fill("fixture-console");
	await page.getByRole("button", { name: "ログイン", exact: true }).click();
	await expect(
		page.getByRole("heading", { name: "機器の値が変わりました", exact: true }),
	).toBeVisible();
	await page.getByRole("button", { name: "最新値と編集内容を比較する", exact: true }).click();
	await expect(page.getByRole("textbox", { name: "ローカルRTPポート", exact: false })).toHaveValue(
		"5014",
	);
	await page.getByRole("button", { name: "変更を確認", exact: true }).click();
	await expect(page.getByRole("table")).toContainText("5006");
	expect((await statistics(page)).writes).toBe(0);
});
test("private pages enforce cache separation and reject unapproved inline scripts and styles", async ({
	page,
}) => {
	const response = await page.goto("/");
	expect(response?.headers()["cache-control"]).toBe("no-cache");
	const api = await page.request.get("/api/console/session");
	expect(api.headers()["cache-control"]).toBe("no-store");
	const asset = await page.locator('link[rel="modulepreload"]').first().getAttribute("href");
	if (!asset) throw new Error("Missing immutable entrypoint");
	const immutable = await page.request.get(asset);
	expect(immutable.headers()["cache-control"]).toBe("public, max-age=31536000, immutable");
	const missing = await page.request.get("/_app/immutable/missing-fixture.js");
	expect(missing.status()).toBe(404);
	expect(missing.headers()["cache-control"]).toBe("no-cache");
	const applied = await page.evaluate(() => {
		const style = document.createElement("style");
		style.textContent = "body { --unapproved-fixture: applied; }";
		document.head.append(style);
		const script = document.createElement("script");
		script.textContent = "document.documentElement.dataset.unapprovedFixture = 'applied'";
		document.head.append(script);
		return {
			style: getComputedStyle(document.body).getPropertyValue("--unapproved-fixture"),
			script: document.documentElement.dataset.unapprovedFixture,
		};
	});
	expect(applied.style).toBe("");
	expect(applied.script).toBeUndefined();
});
test("documentation pagination and direct help links preserve the requested parameter", async ({
	page,
}) => {
	await page.goto("/docs/");
	await expect(page.locator("article")).toHaveCount(20);
	await page.getByRole("button", { name: "次の20件", exact: true }).click();
	await expect(page.getByRole("heading", { name: "設定項目の説明", exact: true })).toBeFocused();
	await page.goto("/docs/?q=P739#parameter-p739");
	await expect(page.locator("article")).toHaveCount(1);
	await expect(
		page.getByRole("heading", { name: "ローカルRTPポート（FXO）", exact: true }),
	).toBeVisible();
});

test("firmware explanations distinguish observed controls from preserved templates", async ({
	page,
}, info) => {
	await page.goto("/docs/?q=P28772");
	await expect(
		page.getByRole("heading", { name: "音声内DTMFの送信音量", exact: true }),
	).toBeVisible();
	await expect(page.getByText("HT813 1.0.19.6の入力欄と説明資料", { exact: true })).toBeVisible();
	await expect(
		page.getByText("保存テンプレートに記載がありません。", { exact: false }),
	).toBeVisible();
	await expect(page.getByText("秘密情報は表示しません。", { exact: false })).not.toBeVisible();
	await page.screenshot({ path: info.outputPath("firmware-help.png"), fullPage: true });
	await page.getByRole("searchbox").fill("P39");
	await expect(page.getByText(/テンプレートのヘッダー: 1\.0\.17\.3/)).toBeVisible();
});
