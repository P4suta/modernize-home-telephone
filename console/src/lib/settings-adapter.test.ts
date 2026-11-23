import { describe, expect, it } from "vitest";
import { SettingsDraft } from "../../packages/settings-workflow/src/index";
import { createSettingsWorkflow } from "../../packages/settings-workflow/src/workflow.svelte";

const input = {
	revision: "captured",
	fields: [
		{
			id: "name",
			identity: "text",
			access: "editable",
			value: { kind: "public", values: ["before"] },
		},
	],
};
function edited(): SettingsDraft {
	return SettingsDraft.capture(input).edit("name", ["after"]);
}
describe("Svelte settings workflow", () => {
	it("checks every four-action trace through the submission state machine", () => {
		type Phase = ReturnType<typeof createSettingsWorkflow>["phase"];
		type Action = "review" | "begin" | "complete" | "interrupt" | "reset" | "cancel";
		const actions: Action[] = ["review", "begin", "complete", "interrupt", "reset", "cancel"];
		const transitions: Record<Phase, Partial<Record<Action, Phase>>> = {
			editing: { review: "reviewing", reset: "editing", cancel: "editing" },
			reviewing: { review: "reviewing", begin: "submitting", reset: "editing", cancel: "editing" },
			submitting: { complete: "editing", interrupt: "reconciliation-required" },
			"reconciliation-required": { reset: "editing" },
		};
		for (let trace = 0; trace < actions.length ** 4; trace += 1) {
			const flow = createSettingsWorkflow();
			const draft = edited();
			const matching = {
				revision: "fresh",
				fields: [{ ...input.fields[0], value: { kind: "public", values: ["after"] } }],
			};
			const operations: Record<Action, () => unknown> = {
				review: () => flow.review(draft, "captured"),
				begin: () => flow.begin(draft, "captured"),
				complete: () => flow.complete(matching),
				interrupt: () => flow.interrupt(),
				reset: () => flow.reset(),
				cancel: () => flow.cancel(),
			};
			let remaining = trace;
			let phase: Phase = "editing";
			for (let step = 0; step < 4; step += 1) {
				const action = actions[remaining % actions.length];
				remaining = Math.floor(remaining / actions.length);
				if (!action) throw new Error("Invalid trace action");
				const expected: Phase | undefined = transitions[phase][action];
				if (expected === undefined) expect(operations[action]).toThrowError("invalid-transition");
				else {
					operations[action]();
					phase = expected;
				}
				expect(flow.phase).toBe(phase);
			}
		}
	});
	it("retains review data while sending and treats missing readback as unverified", () => {
		const flow = createSettingsWorkflow();
		const draft = edited();
		flow.cancel();
		flow.review(draft, "captured");
		flow.review(draft, "captured");
		const items = flow.items;
		expect(() => flow.begin(draft, "fresh")).toThrowError("stale-revision");
		flow.begin(draft, "captured");
		expect(flow.items).toEqual(items);
		expect(() => flow.review(draft, "captured")).toThrowError("invalid-transition");
		expect(flow.complete()).toBe("mismatch");
		expect(flow.items).toEqual([]);
		expect(() => flow.cancel()).toThrowError("invalid-transition");
		flow.reset();
		flow.review(draft, "captured");
		flow.reset();
		expect(flow.phase).toBe("editing");
	});
	it("requires operational verification for a secret after a successful request", () => {
		const input = {
			revision: "captured",
			fields: [{ id: "token", identity: "secret", access: "editable", value: { kind: "secret" } }],
		};
		const draft = SettingsDraft.capture(input).edit("token", ["synthetic-secret"]);
		const flow = createSettingsWorkflow();
		flow.review(draft, "captured");
		flow.begin(draft, "captured");
		expect(flow.complete(input)).toBe("verification-required");
		expect(flow.phase).toBe("reconciliation-required");
	});
	it("requires review and blocks duplicate submissions until reconciliation", () => {
		const flow = createSettingsWorkflow();
		const draft = edited();
		expect(flow.phase).toBe("editing");
		expect(() => flow.begin(draft, "captured")).toThrow();
		flow.review(draft, "captured");
		expect(flow.phase).toBe("reviewing");
		expect(flow.items).toEqual([
			{ id: "name", kind: "public", before: ["before"], after: ["after"] },
		]);
		expect(flow.begin(draft, "captured").values()).toEqual({ name: ["after"] });
		expect(flow.phase).toBe("submitting");
		for (const operation of [
			() => flow.begin(draft, "captured"),
			() => flow.review(draft, "captured"),
			() => flow.reset(),
			() => flow.cancel(),
		])
			expect(operation).toThrow();
		expect(flow.complete(input)).toBe("mismatch");
		expect(flow.phase).toBe("reconciliation-required");
		expect(() => flow.review(draft, "captured")).toThrow();
		flow.reset();
		expect(flow.phase).toBe("editing");
	});
	it("confirms matching readback and preserves the block after interruption", () => {
		const flow = createSettingsWorkflow();
		const draft = edited();
		flow.review(draft, "captured");
		flow.cancel();
		expect(flow.items).toEqual([]);
		flow.review(draft, "captured");
		flow.begin(draft, "captured");
		flow.interrupt();
		expect(flow.phase).toBe("reconciliation-required");
		expect(() => flow.begin(draft, "captured")).toThrow();
		flow.reset();
		flow.review(draft, "captured");
		flow.begin(draft, "captured");
		expect(
			flow.complete({
				revision: "fresh",
				fields: [{ ...input.fields[0], value: { kind: "public", values: ["after"] } }],
			}),
		).toBe("confirmed");
		expect(flow.phase).toBe("editing");
		expect(() => flow.complete(input)).toThrow();
		expect(() => flow.interrupt()).toThrow();
	});
});
