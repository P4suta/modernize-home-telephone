import { writeFile } from "node:fs/promises";
import { chromium, expect, test } from "@playwright/test";
import { launch } from "chrome-launcher";
import lighthouse from "lighthouse";
import desktopConfig from "lighthouse/core/config/desktop-config.js";

const origin = "http://127.0.0.1:8791";
for (const formFactor of ["mobile", "desktop"] as const) {
	for (const route of ["", "status", "basic", "advanced", "fxs", "fxo", "docs"]) {
		test(`Lighthouse ${formFactor} ${route || "login"}`, async ({ page }, info) => {
			test.setTimeout(180000);
			await page.request.post("/__fixture", { data: { operation: "reset" } });
			await page.goto("/");
			if (route !== "" && route !== "docs") {
				await page.getByLabel("パスワード", { exact: true }).fill("fixture-console");
				await page.getByRole("button", { name: "ログイン", exact: true }).click();
				await expect(page.getByRole("heading", { name: "機器情報", exact: true })).toBeVisible();
			}
			const cookies = await page.context().cookies(`${origin}/api/console/session`);
			const cookie = cookies
				.filter((cookie) => cookie.name === "homephone_console")
				.map((cookie) => `${cookie.name}=${cookie.value}`)
				.join("; ");
			const chrome = await launch({
				chromePath: chromium.executablePath(),
				chromeFlags: ["--headless=new"],
				port: 0,
				logLevel: "silent",
			});
			try {
				const result = await lighthouse(
					`${origin}/${route}${route ? "/" : ""}`,
					{
						port: chrome.port,
						logLevel: "error",
						output: ["html"],
						formFactor,
						screenEmulation: {
							mobile: formFactor === "mobile",
							width: formFactor === "mobile" ? 390 : 1440,
							height: formFactor === "mobile" ? 844 : 1000,
							deviceScaleFactor: 1,
							disabled: false,
						},
						disableStorageReset: true,
						onlyCategories: ["performance", "accessibility", "best-practices", "seo"],
						extraHeaders: cookie ? { Cookie: cookie } : {},
					},
					formFactor === "desktop" ? desktopConfig : undefined,
				);
				if (!result) throw new Error("Lighthouse did not produce a report");
				const html = Array.isArray(result.report) ? result.report[0] : result.report;
				if (!html) throw new Error("Lighthouse HTML report is empty");
				const path = info.outputPath("lighthouse.html");
				await writeFile(path, html);
				await info.attach("Lighthouse", { path, contentType: "text/html" });
				await writeFile(info.outputPath("lighthouse.json"), JSON.stringify(result.lhr));
				expect(result.lhr.runtimeError).toBeUndefined();
				expect(result.lhr.configSettings.formFactor).toBe(formFactor);
				expect(result.lhr.configSettings.throttling.cpuSlowdownMultiplier).toBe(
					formFactor === "mobile" ? 4 : 1,
				);
				expect(result.lhr.categories.performance?.score).toBeGreaterThanOrEqual(0.95);
				expect(result.lhr.categories.accessibility?.score).toBe(1);
				expect(result.lhr.categories["best-practices"]?.score).toBe(1);
				expect(result.lhr.audits["is-crawlable"]?.score).toBe(0);
				for (const reference of result.lhr.categories.seo?.auditRefs ?? []) {
					if (reference.id === "is-crawlable") continue;
					const score = result.lhr.audits[reference.id]?.score;
					if (score != null) expect(score, reference.id).toBe(1);
				}
			} finally {
				chrome.kill();
				await page.request.delete("/api/console/session", { headers: { Origin: origin } });
			}
		});
	}
}
