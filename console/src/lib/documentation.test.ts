import { describe, expect, it } from "vitest";
import { appendFirmwareHelp, catalogue, searchHelp } from "./documentation";

describe("parameter documentation", () => {
	it("searches documented defaults and choice explanations alongside exact parameter numbers", () => {
		expect(searchHelp("5004").some((help) => help.parameters.includes("P39"))).toBe(true);
		expect(searchHelp("MD5").some((help) => help.parameters.includes("P21906"))).toBe(true);
		expect(searchHelp("P16")).toHaveLength(1);
	});
	it("covers every preserved definition without conflating accounts, masks or call limits", () => {
		expect(catalogue.filter((help) => help.reference.kind === "template")).toHaveLength(598);
		expect(catalogue).toHaveLength(608);
		for (const [parameter, word] of [
			["P16", "サブネットマスク"],
			["P731", "登録"],
			["P4420", "時間上限"],
			["P91", "禁止"],
		]) {
			const help = searchHelp(parameter ?? "");
			expect(help).toHaveLength(1);
			expect(help[0]?.title).toContain(word);
		}
	});
	it("finds codec explanations by normalized terms and combined words", () => {
		expect(searchHelp("ＰＣＭＵ").length).toBeGreaterThan(0);
		expect(searchHelp("PCMA FXS").length).toBeGreaterThan(0);
		expect(searchHelp("unknown-only-fixture")).toEqual([]);
	});

	it("documents observed firmware fields without fabricated template provenance", () => {
		const firmware = catalogue.filter((help) => help.reference.kind === "device");
		expect(firmware).toHaveLength(10);
		expect(appendFirmwareHelp([], firmware)).toEqual(firmware);
		expect(() => appendFirmwareHelp([], [])).toThrow();
		expect(() => appendFirmwareHelp(catalogue, firmware)).toThrow();
		expect(() => appendFirmwareHelp([], [...firmware, ...firmware])).toThrow();
		for (const help of firmware) {
			expect(help.template_value).toBeNull();
			expect(help.reference).toEqual({ kind: "device", firmware: "1.0.19.6" });
			expect(searchHelp(help.parameters[0] ?? "")).toHaveLength(1);
		}
		expect(searchHelp("P8467")).toHaveLength(1);
		expect(searchHelp("8467")).toHaveLength(1);
		expect(searchHelp("P28772")[0]?.input).toContain("-12〜12");
		expect(searchHelp("P28178")[0]?.default_value).toBe("3");
	});
});
