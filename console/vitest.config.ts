import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vitest/config";
export default defineConfig({
	plugins: [svelte({ configFile: false })],
	test: { environment: "node", include: ["src/**/*.test.ts", "test/**/*.test.ts"], maxWorkers: 2 },
});
