import { describe, expect, it } from "vitest";
import type { Field, Snapshot } from "./api";
import { fieldSchema, snapshotSchema } from "./api";
import { catalogue } from "./documentation";
import {
	changedValues,
	displayValue,
	draftFrom,
	editDraft,
	rebaseDraft,
	validateChanges,
} from "./draft";

const field: Field = {
	id: "field-0",
	parameter: "P39",
	secret: false,
	label: "RTPポート",
	description: "",
	kind: "number",
	values: ["5004"],
	choices: [],
	disabled: false,
	read_only: false,
	multiple: false,
	required: true,
	max_length: 5,
	min: 1024,
	max: 65534,
};
function snapshot(fields: Field[] = [field], revision = "1"): Snapshot {
	return {
		page: "fxs",
		revision: `0123456789abcdef0123456789abcdef:fxs:${revision}`,
		fetched_at_ms: 1,
		settings: fields.map((field) => ({
			field,
			help: catalogue.find((help) => help.parameters.includes("P47")) ?? null,
		})),
		information: [],
		can_save: true,
		can_apply: true,
		reconciliation_required: false,
		undocumented: 0,
		native_session_recovered: false,
	};
}
describe("draft contracts", () => {
	it("owns independent values and detects every change in ordered selections", () => {
		const original = snapshot([{ ...field, values: ["first", "second"] }]);
		const draft = draftFrom(original);
		expect(draft.values).toEqual({ "field-0": ["first", "second"] });
		original.settings[0]?.field.values.push("external");
		expect(draft.baseline["field-0"]?.values).toEqual(["first", "second"]);
		expect(draft.values["field-0"]).toEqual(["first", "second"]);
		for (const values of [
			["first"],
			["first", "second", "third"],
			["first", "changed"],
			["second", "first"],
		])
			expect(changedValues(editDraft(draft, "field-0", values))).toEqual({ "field-0": values });
		expect(() => editDraft(draft, "missing", ["value"])).toThrow("Unknown setting");
		expect(() => editDraft(draft, "toString", ["value"])).toThrow("Unknown setting");
	});
	it("validates each field independently and supplies actionable errors", () => {
		const text: Field = {
			...field,
			kind: "text",
			required: false,
			min: null,
			max: null,
			max_length: null,
		};
		const page = snapshot([
			{ ...text, disabled: true },
			{ ...text, id: "second" },
		]);
		expect(validateChanges(page, { second: ["valid"] })).toEqual({});
		expect(validateChanges(page, { "field-0": ["valid"] })).toEqual({
			"field-0": "変更できない項目です。",
		});
		for (const values of [[], ["a", "b"]])
			expect(validateChanges(snapshot([text]), { "field-0": values })).toEqual({
				"field-0": "値を入力してください。",
			});
		expect(validateChanges(snapshot([text]), { "field-0": [""] })).toEqual({});
		for (const value of ["a\nb", "a\rb"])
			expect(validateChanges(snapshot([text]), { "field-0": [value] })).toEqual({
				"field-0": "文字数と改行を確認してください。",
			});
		expect(
			validateChanges(snapshot([{ ...text, kind: "text_area" }]), { "field-0": ["a\r\nb"] }),
		).toEqual({});
		expect(validateChanges(snapshot([{ ...text, max_length: 3 }]), { "field-0": ["abc"] })).toEqual(
			{},
		);
		expect(validateChanges(snapshot([text]), { "field-0": ["a".repeat(16384)] })).toEqual({});
		expect(validateChanges(snapshot([text]), { "field-0": ["あ".repeat(5462)] })).toEqual({
			"field-0": "文字数と改行を確認してください。",
		});
		expect(validateChanges(snapshot(), { "field-0": ["x"] })).toEqual({
			"field-0": "指定された範囲の整数を入力してください。",
		});
	});
	it("enforces selection cardinality and validates every selected value", () => {
		const choice: Field = {
			...field,
			kind: "select",
			multiple: true,
			required: false,
			min: null,
			max: null,
			max_length: null,
			choices: Array.from({ length: 256 }, (_, index) => ({
				value: String(index),
				label: String(index),
				disabled: false,
			})),
		};
		const all = choice.choices.map((item) => item.value);
		expect(validateChanges(snapshot([choice]), { "field-0": all })).toEqual({});
		expect(validateChanges(snapshot([choice]), { "field-0": [...all, "0"] })).toEqual({
			"field-0": "値を入力してください。",
		});
		expect(
			validateChanges(snapshot([{ ...choice, multiple: false }]), { "field-0": ["0", "1"] }),
		).toEqual({ "field-0": "値を入力してください。" });
		expect(validateChanges(snapshot([choice]), { "field-0": [] })).toEqual({});
		expect(validateChanges(snapshot([choice]), { "field-0": ["0", "unknown"] })).toEqual({
			"field-0": "使用できる選択肢を選んでください。",
		});
		expect(
			validateChanges(snapshot([{ ...choice, max_length: 1 }]), { "field-0": ["0", "10"] }),
		).toEqual({ "field-0": "文字数と改行を確認してください。" });
		const required = snapshot([
			{
				...choice,
				required: true,
				choices: [...choice.choices, { value: "", label: "Empty", disabled: false }],
			},
		]);
		expect(validateChanges(required, { "field-0": ["", "0"] })).toEqual({});
	});
	it("checks signed integer limits without another bound hiding the rejection", () => {
		const numeric: Field = { ...field, min: null, max: null, max_length: null };
		for (const value of ["-2147483648", "2147483647", "-1", "0"])
			expect(validateChanges(snapshot([numeric]), { "field-0": [value] })).toEqual({});
		for (const value of ["-2147483649", "2147483648"])
			expect(validateChanges(snapshot([numeric]), { "field-0": [value] })).toEqual({
				"field-0": "指定された範囲の整数を入力してください。",
			});
		const page = snapshot([
			{
				...numeric,
				kind: "select",
				multiple: true,
				choices: ["161", "160"].map((value) => ({ value, label: value, disabled: false })),
			},
		]);
		const help = catalogue.find((help) => help.parameters.includes("P21903"));
		if (!help) throw new Error("Missing SNMP port contract");
		page.settings = page.settings.map((setting) => ({ ...setting, help }));
		expect(validateChanges(page, { "field-0": ["161", "160"] })).toEqual({
			"field-0": "指定された範囲の整数を入力してください。",
		});
	});
	it("checks untouched required fields while preserving optional and inaccessible values", () => {
		const empty: Field = { ...field, kind: "text", min: null, max: null, values: [""] };
		expect(validateChanges(snapshot([empty]), {})).toEqual({ "field-0": "必須項目です。" });
		expect(validateChanges(snapshot([empty]), { "field-0": ["valid"] })).toEqual({});
		for (const changed of [
			{ required: false },
			{ disabled: true },
			{ read_only: true },
			{ secret: true },
		])
			expect(validateChanges(snapshot([{ ...empty, ...changed }]), {})).toEqual({});
		expect(
			validateChanges(snapshot([{ ...empty, multiple: true, values: ["", "valid"] }]), {}),
		).toEqual({});
	});
	it("refuses every changed field identity before rebasing edits", () => {
		const draft = editDraft(draftFrom(snapshot()), "field-0", ["5012"]);
		for (const change of [
			{ kind: "text" as const },
			{ description: "Changed meaning" },
			{ parameter: "P40" },
			{ secret: true },
		])
			expect(() => rebaseDraft(draft, snapshot([{ ...field, ...change }], "2"))).toThrow(
				"設定項目が変わったため自動で引き継げません。\n編集値を確認して入力し直してください。",
			);
	});
	it("displays public choice labels, raw values, separators and empty state", () => {
		const publicField: Field = {
			...field,
			choices: [
				{ value: "0", label: "PCMU", disabled: false },
				{ value: "8", label: "PCMA", disabled: false },
			],
		};
		expect(displayValue(publicField, ["8", "0", "raw"])).toBe("PCMA、PCMU、raw");
		expect(displayValue(publicField, [])).toBe("未設定");
	});
	it("refuses changed constraints, permissions and documentation when rebasing", () => {
		const draft = editDraft(draftFrom(snapshot()), "field-0", ["5012"]);
		for (const change of [
			{ min: 6000 },
			{ max: 5000 },
			{ max_length: 3 },
			{ disabled: true },
			{ read_only: true },
			{ multiple: true },
			{ required: false },
			{ choices: [{ value: "x", label: "Changed", disabled: false }] },
		])
			expect(() => rebaseDraft(draft, snapshot([{ ...field, ...change }], "2"))).toThrow();
		const undocumented = snapshot([field], "2");
		undocumented.settings = undocumented.settings.map((setting) => ({ ...setting, help: null }));
		expect(() => rebaseDraft(draft, undocumented)).toThrow();
	});
	it("refuses edits when the adapted source denies permission or lacks documentation", () => {
		for (const changed of [
			{ ...field, disabled: true },
			{ ...field, read_only: true },
		])
			expect(() => editDraft(draftFrom(snapshot([changed])), "field-0", ["5012"])).toThrowError(
				"not-editable",
			);
		const documented = snapshot();
		for (const help of [
			null,
			...documented.settings.flatMap(({ help }) => (help ? [{ ...help, read_only: true }] : [])),
		]) {
			const page = {
				...documented,
				settings: documented.settings.map((setting) => ({ ...setting, help })),
			};
			expect(() => editDraft(draftFrom(page), "field-0", ["5012"])).toThrow();
		}
	});
	it("binds each choice value, label and access flag when rebasing", () => {
		const choice = { value: "first", label: "First", disabled: false };
		const original = { ...field, choices: [choice] };
		const draft = editDraft(draftFrom(snapshot([original])), "field-0", ["5012"]);
		for (const change of [{ value: "second" }, { label: "Second" }, { disabled: true }])
			expect(() =>
				rebaseDraft(draft, snapshot([{ ...original, choices: [{ ...choice, ...change }] }], "2")),
			).toThrow();
	});
	it("accepts full certificate fields and rejects the first byte beyond the application bound", () => {
		const certificate: Field = {
			...field,
			parameter: "P8220",
			kind: "text_area",
			min: null,
			max: null,
			max_length: 8192,
			values: [],
		};
		expect(fieldSchema.safeParse({ ...certificate, values: ["a".repeat(8192)] }).success).toBe(
			true,
		);
		expect(validateChanges(snapshot([certificate]), { "field-0": ["a".repeat(8192)] })).toEqual({});
		expect(
			validateChanges(snapshot([{ ...certificate, max_length: null }]), {
				"field-0": ["a".repeat(16385)],
			}),
		).toHaveProperty("field-0");
	});
	it("enforces documented disjoint numeric ranges and ambiguous metadata on text controls", () => {
		const page = snapshot([{ ...field, kind: "text", min: null, max: null }]);
		const help = catalogue.find((help) => help.parameters.includes("P21903"));
		if (!help) throw new Error("Missing SNMP port contract");
		page.settings = page.settings.map((setting) => ({ ...setting, help }));
		for (const good of ["161", "1025", "65535"])
			expect(validateChanges(page, { "field-0": [good] })).toEqual({});
		for (const bad of ["160", "162", "1024", "65536", "1e3", "+161", "161.0"])
			expect(validateChanges(page, { "field-0": [bad] })).toHaveProperty("field-0");
		page.settings = page.settings.map((setting) => ({
			...setting,
			help: { ...help, read_only: true },
		}));
		expect(validateChanges(page, { "field-0": ["161"] })).toHaveProperty("field-0");
	});
	it("retains the baseline without creating a change", () => {
		const draft = draftFrom(snapshot());
		expect(draft.revision).toBe("0123456789abcdef0123456789abcdef:fxs:1");
		expect(changedValues(draft)).toEqual({});
		expect(changedValues(editDraft(draft, "field-0", ["5012"]))).toEqual({
			"field-0": ["5012"],
		});
		expect(
			changedValues(editDraft(editDraft(draft, "field-0", ["5012"]), "field-0", ["5004"])),
		).toEqual({});
	});
	it("requires explicit rebasing and retains edits while accepting unchanged fresh values", () => {
		const draft = editDraft(draftFrom(snapshot()), "field-0", ["5012"]);
		const fresh = snapshot([{ ...field, values: ["5006"] }], "2");
		expect(draft.revision).toBe("0123456789abcdef0123456789abcdef:fxs:1");
		const rebased = rebaseDraft(draft, fresh);
		expect(rebased.revision).toBe("0123456789abcdef0123456789abcdef:fxs:2");
		expect(changedValues(rebased)).toEqual({ "field-0": ["5012"] });
		expect(rebased.baseline["field-0"]?.values).toEqual(["5006"]);
	});
	it("refuses to move an edit onto a different field after form reordering", () => {
		const draft = editDraft(draftFrom(snapshot()), "field-0", ["5012"]);
		expect(() => rebaseDraft(draft, snapshot([{ ...field, label: "別の設定" }], "2"))).toThrow();
		expect(() => rebaseDraft(draft, snapshot([], "2"))).toThrow();
	});
	it("validates numeric syntax and exact boundaries independently", () => {
		for (const good of ["1024", "5004", "65534"])
			expect(validateChanges(snapshot(), { "field-0": [good] })).toEqual({});
		for (const bad of ["1023", "65535", "1.5", "5e3", "", "x", "-1", "2147483648"])
			expect(validateChanges(snapshot(), { "field-0": [bad] })).toHaveProperty("field-0");
	});
	it("rejects unknown, disabled, read-only, undocumented, oversized and mixed invalid choices", () => {
		const choice: Field = {
			...field,
			kind: "select",
			min: null,
			max: null,
			required: false,
			multiple: true,
			choices: [
				{ value: "0", label: "PCMU", disabled: false },
				{ value: "8", label: "PCMA", disabled: true },
			],
		};
		expect(validateChanges(snapshot([choice]), { "field-0": ["0"] })).toEqual({});
		for (const values of [["8"], ["0", "8"], ["unknown"]])
			expect(validateChanges(snapshot([choice]), { "field-0": values })).toHaveProperty("field-0");
		expect(validateChanges(snapshot(), { missing: ["1"] })).toHaveProperty("missing");
		for (const changed of [
			{ ...field, disabled: true },
			{ ...field, read_only: true },
			{ ...field, max_length: 2 },
		])
			expect(validateChanges(snapshot([changed]), { "field-0": ["5012"] })).toHaveProperty(
				"field-0",
			);
		const undocumented = snapshot();
		for (const setting of undocumented.settings) setting.help = null;
		expect(validateChanges(undocumented, { "field-0": ["5012"] })).toHaveProperty("field-0");
	});
	it("keeps passwords out of review labels and refuses an empty password edit", () => {
		const password: Field = {
			...field,
			kind: "password",
			secret: true,
			values: [],
			min: null,
			max: null,
			required: false,
			max_length: 30,
		};
		expect(displayValue(password, ["private"])).toBe("変更あり");
		expect(displayValue(password, [])).toBe("変更なし");
		expect(validateChanges(snapshot([password]), { "field-0": [""] })).toHaveProperty("field-0");
		expect(validateChanges(snapshot([password]), { "field-0": ["private"] })).toEqual({});
	});
	it("permits clearing a password input while refusing an empty replacement at review", () => {
		const password: Field = {
			...field,
			kind: "password",
			secret: true,
			values: [],
			min: null,
			max: null,
			required: false,
		};
		const page = snapshot([password]);
		const typed = editDraft(draftFrom(page), "field-0", ["synthetic-password-fixture"]);
		const cleared = editDraft(typed, "field-0", [""]);
		expect(cleared.values["field-0"]).toEqual([""]);
		expect(validateChanges(page, changedValues(cleared))).toHaveProperty("field-0");
		expect(() => cleared.core.review(cleared.revision)).toThrowError("invalid-value");
		expect(changedValues(editDraft(typed, "field-0", []))).toEqual({});
	});
});
describe("external contracts", () => {
	it("refuses inconsistent secret flags and secrets exposed as choices", () => {
		expect(
			fieldSchema.safeParse({ ...field, kind: "password", values: [], secret: false }).success,
		).toBe(false);
		expect(
			fieldSchema.safeParse({
				...field,
				kind: "select",
				values: [],
				secret: true,
				choices: [{ value: "private-option-fixture", label: "Credential", disabled: false }],
			}).success,
		).toBe(false);
	});
	it("rejects secrets, malformed revisions, duplicate bindings and duplicate choices", () => {
		expect(fieldSchema.safeParse({ ...field, kind: "password" }).success).toBe(false);
		expect(
			fieldSchema.safeParse({
				...field,
				choices: [
					{ value: "x", label: "a", disabled: false },
					{ value: "x", label: "b", disabled: false },
				],
			}).success,
		).toBe(false);
		for (const revision of ["0", "01", "-1", "18446744073709551616"])
			expect(snapshotSchema.safeParse({ ...snapshot(), revision }).success).toBe(false);
		expect(snapshotSchema.safeParse(snapshot([field, field])).success).toBe(false);
		expect(snapshotSchema.safeParse(snapshot()).success).toBe(true);
	});
});
