import { resolve } from "node:path";
import { defineConfig } from "@playwright/test";

const CASE_TIMEOUT_MS = 45000;
const LAB_ORIGIN = "http://localhost:8787";

export default defineConfig({
	testDir: "./test/browser",
	fullyParallel: false,
	workers: 1,
	timeout: CASE_TIMEOUT_MS,
	use: {
		baseURL: LAB_ORIGIN,
		permissions: ["microphone"],
		trace: "retain-on-failure",
		screenshot: "only-on-failure",
		launchOptions: {
			args: [
				"--use-fake-ui-for-media-stream",
				"--use-fake-device-for-media-stream",
				`--use-file-for-fake-audio-capture=${resolve("../.local/fixtures/browser.wav")}`,
			],
		},
	},
	reporter: [["list"], ["json", { outputFile: "test-results/results.json" }]],
});
