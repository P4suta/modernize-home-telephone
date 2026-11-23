import { type DeepMergeLeafURI, deepmergeCustom } from "deepmerge-ts";
import { expect, it } from "vitest";

const merge = deepmergeCustom<unknown, { DeepMergeArraysURI: DeepMergeLeafURI }>({
	mergeArrays: false,
});

it("replaces configuration arrays while retaining nested options and function identities", () => {
	const runes = (): boolean => true;
	const defaults = { extensions: [".svelte"], compilerOptions: { dev: false, runes } };
	const result = merge(defaults, { extensions: [".component"], compilerOptions: { dev: true } });
	expect(result).toEqual({ extensions: [".component"], compilerOptions: { dev: true, runes } });
	expect(result.compilerOptions.runes).toBe(runes);
	expect(defaults.extensions).toEqual([".svelte"]);
	expect(defaults.compilerOptions.dev).toBe(false);
});

it("does not inherit attacker properties through a configuration prototype key", () => {
	const payload = { session: Object.fromEntries([["__proto__", { admin: true }]]) };
	const nullPrototype: Record<string, unknown> = {};
	Object.setPrototypeOf(nullPrototype, null);
	for (const session of [null, nullPrototype, Object.fromEntries([["__proto__", null]])]) {
		const result = merge({ session }, payload);
		expect("admin" in result.session).toBe(false);
		expect(Object.getPrototypeOf(result.session)).toBe(Object.prototype);
	}
	expect("admin" in {}).toBe(false);
});
