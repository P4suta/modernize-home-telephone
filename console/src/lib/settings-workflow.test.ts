import { describe, expect, it } from "vitest";
import {
	SettingsDraft,
	SettingsError,
	type SettingsSnapshot,
} from "../../packages/settings-workflow/src/index";

function fixture(revision = "captured"): SettingsSnapshot {
	return {
		revision,
		fields: [
			{
				id: "name",
				identity: "name:text:editable",
				access: "editable",
				value: { kind: "public", values: ["desk"] },
			},
			{
				id: "token",
				identity: "token:secret:editable",
				access: "editable",
				value: { kind: "secret" },
			},
			{
				id: "serial",
				identity: "serial:readonly",
				access: "read-only",
				value: { kind: "public", values: ["unit"] },
			},
			{
				id: "unknown",
				identity: "unknown:undocumented",
				access: "undocumented",
				value: { kind: "public", values: [] },
			},
		],
	};
}

describe("settings workflow", () => {
	it("exhaustively checks ordered changes and readback over short selections", () => {
		const selections = [[], ["a"], ["b"], ["a", "a"], ["a", "b"], ["b", "a"], ["b", "b"]];
		const captured = (values: string[], revision: string): SettingsSnapshot => ({
			revision,
			fields: [
				{
					id: "choice",
					identity: "selection",
					access: "editable",
					value: { kind: "public", values },
				},
			],
		});
		for (const baseline of selections)
			for (const values of selections) {
				const draft = SettingsDraft.capture(captured(baseline, "captured")).edit("choice", values);
				const changed = JSON.stringify(baseline) !== JSON.stringify(values);
				expect(draft.changes()).toEqual(changed ? { choice: values } : {});
				if (!changed) continue;
				const submitted = draft.review("captured").submit(draft, "captured");
				for (const actual of selections)
					expect(submitted.reconcile(captured(actual, "readback"))).toBe(
						JSON.stringify(actual) === JSON.stringify(values) ? "confirmed" : "mismatch",
					);
			}
	});
	it("rejects malformed external snapshots and preserves structured faults", () => {
		const valid = fixture();
		expect(() =>
			SettingsDraft.capture(Object.assign((): undefined => undefined, valid)),
		).toThrowError("invalid-snapshot");
		const field = valid.fields[0];
		if (!field) throw new Error("Missing fixture");
		for (const value of [
			null,
			false,
			"invalid",
			{},
			{ ...valid, revision: 1 },
			{ ...valid, fields: null },
			{ ...valid, fields: {} },
			...[
				null,
				{},
				{ ...field, id: "" },
				{ ...field, id: 1 },
				{ ...field, identity: 1 },
				{ ...field, access: "unknown" },
				{ ...field, value: null },
				{ ...field, value: { kind: "secret", values: [] } },
				{ ...field, value: { kind: "public", values: [1] } },
				{ ...field, value: { kind: "public", values: null } },
				{ ...field, value: { kind: "unknown" } },
			].map((invalid) => ({ ...valid, fields: [invalid] })),
		]) {
			expect(() => SettingsDraft.capture(value)).toThrowError("invalid-snapshot");
		}
		try {
			SettingsDraft.capture(null);
		} catch (cause) {
			expect(cause).toBeInstanceOf(SettingsError);
			expect(cause).toMatchObject({
				name: "SettingsError",
				message: "invalid-snapshot",
				code: "invalid-snapshot",
			});
		}
		const draft = SettingsDraft.capture(valid);
		expect(() => draft.edit("missing", ["value"])).toThrowError("unknown-field");
		expect(() => draft.edit("serial", ["value"])).toThrowError("not-editable");
		expect(() => draft.edit("token", ["value", "other"])).toThrowError("invalid-value");
		for (const values of [null, "value", [1]]) {
			const call = (): unknown => Reflect.apply(draft.edit.bind(draft), draft, ["name", values]);
			expect(call).toThrowError("invalid-value");
		}
	});
	it("owns captured and submitted values without allowing revision or review mutation", () => {
		const input = {
			revision: "captured",
			fields: [
				{
					id: "name",
					identity: "name",
					access: "editable",
					value: { kind: "public", values: ["before"] },
				},
			],
		};
		const original = SettingsDraft.capture(input);
		input.fields[0]?.value.values.push("external");
		expect(original.values().name).toEqual(["before"]);
		expect(Reflect.set(original, "revision", "changed")).toBe(false);
		const values = ["after"];
		const edited = original.edit("name", values);
		values.push("external");
		const review = edited.review("captured");
		const items = review.items();
		const item = items[0];
		if (item?.kind === "public") {
			item.before.push("external");
			item.after.push("external");
		}
		expect(review.items()).toEqual([
			{ id: "name", kind: "public", before: ["before"], after: ["after"] },
		]);
		const submitted = review.submit(edited, "captured");
		expect(submitted.revision).toBe("captured");
		submitted.values().name?.push("external");
		expect(submitted.values()).toEqual({ name: ["after"] });
	});
	it("captures independent baselines and refuses inaccessible edits and empty secret review", () => {
		const input = fixture();
		const draft = SettingsDraft.capture(input);
		expect(draft.values()).toEqual({ name: ["desk"], token: [], serial: ["unit"], unknown: [] });
		expect(draft.changes()).toEqual({});
		for (const id of ["missing", "toString", "serial", "unknown"])
			expect(() => draft.edit(id, ["changed"])).toThrow();
		expect(draft.edit("token", []).changes()).toEqual({});
		expect(() => draft.edit("token", [""]).review("captured")).toThrowError("invalid-value");
		expect(() =>
			draft.edit("token", [""]).edit("name", ["changed"]).review("captured"),
		).toThrowError("invalid-value");
		expect(() => draft.edit("token", ["", ""])).toThrowError("invalid-value");
		const emptyPublic = draft.edit("name", [""]);
		expect(emptyPublic.review("captured").submit(emptyPublic, "captured").values()).toEqual({
			name: [""],
		});
		const edited = draft.edit("name", ["phone"]);
		expect(edited.changes()).toEqual({ name: ["phone"] });
		expect(draft.changes()).toEqual({});
		const copy = edited.values();
		copy.name?.push("external");
		expect(edited.values().name).toEqual(["phone"]);
		expect(edited.edit("name", ["desk"]).changes()).toEqual({});
	});
	it("preserves ordered selections and requires explicit identity-aware rebase", () => {
		const draft = SettingsDraft.capture(fixture()).edit("name", ["a", "b"]);
		const fresh = fixture("fresh");
		expect(draft.rebase(fresh).revision).toBe("fresh");
		expect(draft.rebase(fresh).changes()).toEqual({ name: ["a", "b"] });
		expect(draft.edit("name", ["b", "a"]).changes()).toEqual({ name: ["b", "a"] });
		for (const fields of [
			[],
			fresh.fields.filter((field) => field.id !== "name"),
			fresh.fields.map((field) =>
				field.id === "name" ? { ...field, identity: "different" } : field,
			),
		])
			expect(() => draft.rebase({ ...fresh, fields })).toThrow();
		expect(() =>
			draft.rebase({
				...fresh,
				fields: fresh.fields.map((field) =>
					field.id === "name" ? { ...field, access: "read-only" } : field,
				),
			}),
		).toThrow();
	});
	it("binds review to the exact immutable draft and current revision", () => {
		const draft = SettingsDraft.capture(fixture()).edit("name", ["phone"]);
		expect(() => SettingsDraft.capture(fixture()).review("captured")).toThrow();
		expect(() => draft.review("stale")).toThrow();
		const review = draft.review("captured");
		expect(review.items()).toEqual([
			{ id: "name", kind: "public", before: ["desk"], after: ["phone"] },
		]);
		expect(() => review.submit(draft.edit("name", ["other"]), "captured")).toThrow();
		expect(() => review.submit(draft, "stale")).toThrow();
		expect(review.submit(draft, "captured").values()).toEqual({ name: ["phone"] });
	});
	it("does not let secret verification hide public mismatches or changed field kinds", () => {
		const input = fixture();
		const secretFirst = { ...input, fields: [...input.fields].reverse() };
		const draft = SettingsDraft.capture(secretFirst)
			.edit("token", ["secret-fixture"])
			.edit("name", ["after"]);
		const submission = draft.review("captured").submit(draft, "captured");
		expect(submission.reconcile(fixture("fresh"))).toBe("mismatch");
		const changedKind = {
			...input,
			fields: input.fields.map((field) =>
				field.id === "name" ? { ...field, value: { kind: "secret" as const } } : field,
			),
		};
		expect(() => draft.rebase(changedKind)).toThrowError("field-changed");
		expect(submission.reconcile(changedKind)).toBe("mismatch");
		expect(() => SettingsDraft.capture(input).review("captured")).toThrowError("no-changes");
		expect(() => draft.review("fresh")).toThrowError("stale-revision");
		expect(() =>
			draft.review("captured").submit(draft.edit("name", ["other"]), "captured"),
		).toThrowError("review-changed");
		expect(() => draft.review("captured").submit(draft, "fresh")).toThrowError("stale-revision");
	});
	it("redacts secret review data and never infers secret verification from readback", () => {
		const draft = SettingsDraft.capture(fixture()).edit("token", ["private-fixture"]);
		const review = draft.review("captured");
		expect(review.items()).toEqual([{ id: "token", kind: "secret" }]);
		expect(JSON.stringify(review.items())).not.toContain("private-fixture");
		const submitted = review.submit(draft, "captured");
		expect(submitted.values()).toEqual({ token: ["private-fixture"] });
		expect(submitted.reconcile(fixture("fresh"))).toBe("verification-required");
	});
	it("requires matching public values and field identity before confirming a write", () => {
		const draft = SettingsDraft.capture(fixture()).edit("name", ["phone"]);
		const submitted = draft.review("captured").submit(draft, "captured");
		expect(submitted.reconcile(fixture("fresh"))).toBe("mismatch");
		const fresh = fixture("fresh");
		const matching = {
			...fresh,
			fields: fresh.fields.map((field) =>
				field.id === "name"
					? { ...field, value: { kind: "public" as const, values: ["phone"] } }
					: field,
			),
		};
		expect(submitted.reconcile(matching)).toBe("confirmed");
		expect(
			submitted.reconcile({
				...matching,
				fields: matching.fields.map((field) =>
					field.id === "name" ? { ...field, access: "read-only" } : field,
				),
			}),
		).toBe("mismatch");
		expect(submitted.reconcile({ ...matching, fields: [] })).toBe("mismatch");
		expect(
			submitted.reconcile({
				...matching,
				fields: matching.fields.map((field) => ({ ...field, identity: "changed" })),
			}),
		).toBe("mismatch");
	});
	it("rejects empty identities and duplicate bindings without exposing secrets", () => {
		const snapshot = fixture();
		for (const input of [
			{ ...snapshot, revision: "" },
			{ ...snapshot, fields: [...snapshot.fields, ...snapshot.fields] },
			{ ...snapshot, fields: snapshot.fields.map((field) => ({ ...field, identity: "" })) },
		])
			expect(() => SettingsDraft.capture(input)).toThrow();
	});
});
