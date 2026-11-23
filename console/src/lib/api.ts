import { z } from "zod";
import type { components } from "./generated/api";

z.config({ jitless: true });

export type Field = components["schemas"]["Field"];
export type Help = components["schemas"]["Help"];
export type Snapshot = components["schemas"]["PageSnapshot"];
export type Changes = components["schemas"]["Changes"];
export type Receipt = components["schemas"]["WriteReceipt"];
export type Session = components["schemas"]["SessionInfo"];
export type PageId = components["schemas"]["PageId"];
const boundedText = z.string().max(16384);
const integerRangeSchema = z
	.strictObject({ min: z.number().int(), max: z.number().int() })
	.refine((range) => range.min <= range.max);
export const helpSchema: z.ZodType<Help> = z.strictObject({
	key: z.string().min(1),
	title: z.string().min(1),
	aliases: z.array(z.string().min(1)).min(1),
	parameters: z.array(z.string().regex(/^P?[0-9]+$/)).length(1),
	category: z.string().min(1),
	summary: z.string().min(1),
	input: z.string().min(1),
	default_value: z.string().min(1),
	recommendation: z.string().min(1),
	impact: z.string().min(1),
	source: z.url().refine((value) => value.startsWith("https://")),
	template_value: boundedText.nullable(),
	reference: z.discriminatedUnion("kind", [
		z.strictObject({
			kind: z.literal("template"),
			firmware: z.string().min(1),
			line: z.number().int().positive(),
		}),
		z.strictObject({ kind: z.literal("device"), firmware: z.string().min(1) }),
	]),
	ranges: z.array(integerRangeSchema).max(256),
	note: z.string(),
	read_only: z.boolean(),
});
export const fieldSchema: z.ZodType<Field> = z
	.strictObject({
		id: z.string().min(1).max(128),
		parameter: z.string().min(1).max(128),
		secret: z.boolean(),
		label: boundedText,
		description: boundedText,
		kind: z.enum(["text", "password", "text_area", "select", "radio", "checkbox", "number"]),
		values: z.array(boundedText).max(256),
		choices: z
			.array(
				z.strictObject({
					value: boundedText,
					label: boundedText,
					disabled: z.boolean(),
				}),
			)
			.max(256),
		disabled: z.boolean(),
		read_only: z.boolean(),
		multiple: z.boolean(),
		required: z.boolean(),
		max_length: z.number().int().nonnegative().nullable(),
		min: z.number().int().nullable(),
		max: z.number().int().nullable(),
	})
	.superRefine((field, context) => {
		if ((field.kind === "password" && !field.secret) || (field.secret && field.choices.length > 0))
			context.addIssue({ code: "custom", message: "Secret metadata must not expose choices" });
		if ((field.secret || field.kind === "password") && field.values.length > 0)
			context.addIssue({
				code: "custom",
				message: "Password values must remain on the server",
			});
		if (new Set(field.choices.map((choice) => choice.value)).size !== field.choices.length)
			context.addIssue({ code: "custom", message: "Duplicate choices" });
	});
export const snapshotSchema: z.ZodType<Snapshot> = z
	.strictObject({
		page: z.enum(["status", "basic", "advanced", "fxs", "fxo"]),
		revision: z.string().refine((value) => {
			const number = value.split(":")[2];
			return (
				/^[0-9a-f]{32}:(status|basic|advanced|fxs|fxo):[1-9][0-9]{0,19}$/.test(value) &&
				number !== undefined &&
				BigInt(number) <= 18446744073709551615n
			);
		}),
		fetched_at_ms: z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER),
		settings: z
			.array(z.strictObject({ field: fieldSchema, help: helpSchema.nullable() }))
			.max(1024),
		information: z.array(z.strictObject({ label: boundedText, value: boundedText })).max(512),
		can_save: z.boolean(),
		can_apply: z.boolean(),
		reconciliation_required: z.boolean(),
		undocumented: z.number().int().nonnegative(),
		native_session_recovered: z.boolean(),
	})
	.refine(
		(snapshot) =>
			new Set(snapshot.settings.map((setting) => setting.field.id)).size ===
				snapshot.settings.length && snapshot.revision.split(":")[1] === snapshot.page,
	);
const sessionSchema: z.ZodType<Session> = z.strictObject({
	authenticated: z.boolean(),
	username: z.string().nullable(),
});
const receiptSchema: z.ZodType<Receipt> = z.strictObject({
	outcome: z.enum(["confirmed", "uncertain", "verification_required"]),
	snapshot: snapshotSchema.nullable(),
});
const problemSchema = z.strictObject({ code: z.string(), message: z.string() });
export class ApiError extends Error {
	constructor(
		public readonly status: number,
		public readonly code: string,
		message: string,
	) {
		super(message);
		this.name = "ApiError";
	}
}
async function request<T>(
	path: string,
	method: "GET" | "POST" | "DELETE",
	schema: z.ZodType<T>,
	payload?: unknown,
): Promise<T> {
	let response: Response;
	try {
		response = await fetch(`/api/console/${path}`, {
			method,
			credentials: "same-origin",
			cache: "no-store",
			...(payload === undefined
				? {}
				: {
						headers: { "Content-Type": "application/json" },
						body: JSON.stringify(payload),
					}),
		});
	} catch {
		throw new ApiError(
			0,
			"network_error",
			"サーバーへ接続できません。\n編集中の値は残しています。",
		);
	}
	if (!response.ok) {
		const parsed = problemSchema.safeParse(await response.json().catch(() => null));
		throw new ApiError(
			response.status,
			parsed.success ? parsed.data.code : "request_failed",
			parsed.success
				? parsed.data.message
				: "処理に失敗しました。\n接続状態と入力値を確認してください。",
		);
	}
	const parsed = schema.safeParse(await response.json());
	if (!parsed.success)
		throw new ApiError(
			502,
			"invalid_response",
			"機器の応答形式を確認できません。\n設定の送信は停止しています。",
		);
	return parsed.data;
}
export function getSession(): Promise<Session> {
	return request("session", "GET", sessionSchema);
}
export function signIn(username: string, password: string): Promise<Session> {
	return request("session", "POST", sessionSchema, { username, password });
}
export async function signOut(): Promise<void> {
	const response = await fetch("/api/console/session", {
		method: "DELETE",
		credentials: "same-origin",
	});
	if (!response.ok)
		throw new ApiError(response.status, "logout_failed", "ログアウトに失敗しました。");
}
export function getPage(page: PageId): Promise<Snapshot> {
	return request(`pages/${page}`, "GET", snapshotSchema);
}
export function submit(page: PageId, changes: Changes): Promise<Receipt> {
	return request(`pages/${page}`, "POST", receiptSchema, changes);
}

export type ParameterReference = components["schemas"]["ParameterReference"];
export type HelpTopic = components["schemas"]["HelpTopic"];
export const parameterReferenceSchema: z.ZodType<ParameterReference> = z.strictObject({
	id: z.string().regex(/^P?[0-9]+$/),
	title: z.string().min(1),
	topic: z.string().min(1),
	section: z.enum(["basic", "advanced", "fxs", "fxo"]),
	source_line: z.number().int().positive(),
	active: z.boolean(),
	template_value: boundedText.nullable(),
	default_value: z.string().nullable(),
	ranges: z.array(integerRangeSchema).max(256),
	note: z.string(),
	read_only: z.boolean(),
});
export const helpTopicSchema: z.ZodType<HelpTopic> = z.strictObject({
	key: z.string().min(1),
	title: z.string().min(1),
	aliases: z.array(z.string().min(1)).min(1),
	summary: z.string().min(1),
	input: z.string().min(1),
	recommendation: z.string().min(1),
	impact: z.string().min(1),
	source: z.url().refine((value) => value.startsWith("https://")),
});
