import type { Logger } from "@stryker-mutator/api/logging";
import { Instrumenter } from "@stryker-mutator/instrumenter";
import { compile } from "svelte/compiler";
import { describe, expect, it, vi } from "vitest";

describe("installed Svelte mutation instrumentation", () => {
	for (const content of [
		"{#snippet line(value)}<span>{value}</span>{/snippet}{@render line(true)}",
		"{#snippet line(value)}<span>{value}</span>{/snippet}{@render line?.(true)}",
		"{#snippet line(value = true)}<span>{value}</span>{/snippet}{@render line()}",
		"{#snippet line({value = true} = {})}<span>{value}</span>{/snippet}{@render line()}",
		"{#snippet line(value =\n{flag: true}\n)}<span>{value.flag}</span>{/snippet}{@render line()}",
		'<script lang="ts">const label: string = "🙂";</script>🙂{#snippet line(value: boolean = true)}<span>{label}{value}</span>{/snippet}{@render line()}',
	]) {
		it(`covers the template expression in ${content}`, async () => {
			const log = vi.fn<(message: string, ...args: unknown[]) => void>();
			const disabled = (): boolean => false;
			const logger: Logger = {
				isTraceEnabled: disabled,
				isDebugEnabled: disabled,
				isInfoEnabled: disabled,
				isWarnEnabled: disabled,
				isErrorEnabled: disabled,
				isFatalEnabled: disabled,
				trace: log,
				debug: log,
				info: log,
				warn: log,
				error: log,
				fatal: log,
			};
			compile(content, { generate: "client" });
			const result = await new Instrumenter(logger).instrument(
				[{ name: "regression.svelte", content, mutate: true }],
				{ plugins: null, excludedMutations: [], ignorers: [] },
			);
			const offset = content.indexOf("true");
			const line = content.slice(0, offset).split("\n").length - 1;
			const column = offset - content.lastIndexOf("\n", offset) - 1;
			expect(result.mutants).toEqual(
				expect.arrayContaining([
					expect.objectContaining({
						mutatorName: "BooleanLiteral",
						replacement: "false",
						location: { start: { line, column }, end: { line, column: column + 4 } },
					}),
				]),
			);
			expect(result.files).toHaveLength(1);
			for (const file of result.files) {
				compile(file.content, { generate: "client" });
				compile(file.content, { generate: "server" });
			}
		});
	}
});
