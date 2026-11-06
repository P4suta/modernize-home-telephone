import { resolve } from "node:path";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

const DEV_LISTEN_ADDRESS = "127.0.0.1";
const API_PROXY = "http://127.0.0.1:8787";
const SIP_PROXY = "ws://127.0.0.1:8787";
const UNIT_TEST_WORKERS = 2;

export default defineConfig({
	plugins: [react()],
	build: {
		rolldownOptions: {
			input: { app: resolve("index.html"), sw: resolve("worker/sw.ts") },
			output: {
				entryFileNames: (chunk): string =>
					chunk.name === "sw" ? "sw.js" : "assets/[name]-[hash].js",
				codeSplitting: {
					groups: [
						{ name: "telephony", test: /node_modules\/sip.js/ },
						{ name: "react", test: /node_modules\/react/ },
						{ name: "validation", test: /node_modules\/zod/ },
					],
				},
			},
		},
	},
	server: {
		host: DEV_LISTEN_ADDRESS,
		strictPort: true,
		proxy: {
			"/api": API_PROXY,
			"/sip": { target: SIP_PROXY, ws: true },
		},
	},
	test: {
		environment: "jsdom",
		setupFiles: ["./test/setup.ts"],
		include: ["src/**/*.test.{ts,tsx}"],
		maxWorkers: UNIT_TEST_WORKERS,
		coverage: { provider: "v8", include: ["src/*.ts", "src/*.tsx"] },
	},
});
