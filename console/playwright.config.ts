import { defineConfig } from "@playwright/test";
export default defineConfig({
	testDir: "./test/browser",
	fullyParallel: false,
	workers: 1,
	retries: 0,
	timeout: 45000,
	expect: { timeout: 10000 },
	use: {
		baseURL: "http://127.0.0.1:8791",
		viewport: { width: 1440, height: 1000 },
		trace: "retain-on-failure",
		screenshot: "only-on-failure",
	},
	reporter: [["list"], ["json", { outputFile: "test-results/results.json" }]],
	webServer: {
		command:
			"mise x -- cargo run --locked -p phone-server --features console-fixture --bin ht813-console -- fixture --listen 127.0.0.1:8791 --assets console/dist",
		cwd: "..",
		url: "http://127.0.0.1:8791/",
		timeout: 120000,
		reuseExistingServer: false,
		gracefulShutdown: { signal: "SIGINT", timeout: 5000 },
	},
});
