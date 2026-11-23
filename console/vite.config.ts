import adapter from "@sveltejs/adapter-static";
import { sveltekit } from "@sveltejs/kit/vite";
import { defineConfig } from "vite";

const ANNOUNCER_STYLE_HASH = "sha256-S8qMpvofolR8Mpjy4kQvEm7m1q8clzU4dfDH0AmvZjo=";

export default defineConfig({
	plugins: [
		sveltekit({
			compilerOptions: {
				runes: ({ filename }: { filename: string }): true | undefined =>
					filename.split(/[/\\]/).includes("node_modules") ? undefined : true,
			},
			adapter: adapter({ pages: "dist", assets: "dist", precompress: true }),
			csp: {
				mode: "hash",
				directives: {
					"default-src": ["self"],
					"script-src": ["self"],
					"style-src": ["self"],
					"style-src-attr": ["unsafe-hashes", ANNOUNCER_STYLE_HASH],
					"connect-src": ["self"],
					"font-src": ["self"],
					"img-src": ["self", "data:"],
					"base-uri": ["none"],
					"form-action": ["self"],
					"object-src": ["none"],
				},
			},
		}),
	],
	server: {
		proxy: {
			"/api/console": { target: "http://127.0.0.1:8790", changeOrigin: false },
		},
	},
});
