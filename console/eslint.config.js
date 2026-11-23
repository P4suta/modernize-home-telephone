import js from "@eslint/js";
import { defineConfig } from "eslint/config";
import prettier from "eslint-config-prettier";
import svelte from "eslint-plugin-svelte";
import globals from "globals";
import ts from "typescript-eslint";

export default defineConfig(
	{
		ignores: [
			".svelte-kit/**",
			"dist/**",
			"packages/*/dist/**",
			"packages/*/.svelte-kit/**",
			"node_modules/**",
			"test-results/**",
			"playwright-report/**",
			"src/lib/generated/**",
		],
	},
	js.configs.recommended,
	ts.configs.recommendedTypeChecked,
	svelte.configs.recommended,
	prettier,
	svelte.configs.prettier,
	{
		languageOptions: {
			globals: { ...globals.browser, ...globals.node },
			parserOptions: {
				projectService: true,
				tsconfigRootDir: import.meta.dirname,
				extraFileExtensions: [".svelte"],
			},
		},
		rules: {
			"@typescript-eslint/explicit-function-return-type": [
				"error",
				{ allowExpressions: true, allowTypedFunctionExpressions: true },
			],
			"svelte/button-has-type": "error",
		},
	},
	{
		files: ["**/*.svelte", "**/*.svelte.ts"],
		languageOptions: { parserOptions: { parser: ts.parser } },
	},
);
