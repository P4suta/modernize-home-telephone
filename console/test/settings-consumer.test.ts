import { SettingsDraft, type SettingsSnapshot } from "@homephone/settings-workflow";
import { describe, expect, it } from "vitest";
import { z } from "zod";

const schema = z.strictObject({
	revision: z.string().min(1),
	host: z.string(),
	tokenConfigured: z.boolean(),
});
type ServiceConfiguration = z.infer<typeof schema>;
function settings(input: ServiceConfiguration): SettingsSnapshot {
	return {
		revision: input.revision,
		fields: [
			{
				id: "service-host",
				identity: "service.host:text",
				access: "editable",
				value: { kind: "public", values: [input.host] },
			},
			{
				id: "service-token",
				identity: "service.token:writeOnly",
				access: "editable",
				value: { kind: "secret" },
			},
		],
	};
}

describe("independent service configuration consumer", () => {
	it("uses built exports with distinct transport bindings and unchanged write-only values", () => {
		const response = schema.parse({ revision: "service:1", host: "", tokenConfigured: true });
		const draft = SettingsDraft.capture(settings(response));
		expect(draft.values()["service-host"]).toEqual([""]);
		expect(draft.changes()).toEqual({});
		const edited = draft.edit("service-host", ["example.test"]);
		const sent = edited.review("service:1").submit(edited, "service:1");
		const values = sent.values();
		const payload = Object.hasOwn(values, "service-host")
			? { host: values["service-host"]?.[0] }
			: {};
		expect(payload).toEqual({ host: "example.test" });
		expect(values).not.toHaveProperty("service-token");
		expect(
			sent.reconcile(settings({ ...response, revision: "service:2", host: "example.test" })),
		).toBe("confirmed");
	});
	it("uses explicit secret replacement while redacting review output", () => {
		const draft = SettingsDraft.capture(
			settings(
				schema.parse({ revision: "service:1", host: "example.test", tokenConfigured: false }),
			),
		).edit("service-token", ["synthetic-token-fixture"]);
		const review = draft.review("service:1");
		expect(review.items()).toEqual([{ id: "service-token", kind: "secret" }]);
		expect(review.submit(draft, "service:1").values()).toEqual({
			"service-token": ["synthetic-token-fixture"],
		});
	});
});
