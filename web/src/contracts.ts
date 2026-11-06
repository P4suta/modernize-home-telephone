import { z } from "zod";
import type { components } from "./generated/api";

const seconds = z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER);
export const callSchema = z.object({
	id: z.uuid(),
	direction: z.enum(["inbound", "outbound"]),
	caller: z.string().max(256),
	phase: z.enum([
		"offered",
		"screening",
		"ringing",
		"connected",
		"held",
		"voicemail",
		"ended",
	]),
	started_at: seconds,
	ended_at: seconds.nullable(),
	reason: z.string().nullable(),
	transcript: z
		.array(
			z.object({ item_id: z.string(), speaker: z.string(), text: z.string() }),
		)
		.max(256),
	summary: z.string().nullable(),
	recording: z.discriminatedUnion("status", [
		z.object({ status: z.literal("none") }),
		z.object({ status: z.literal("pending") }),
		z.object({ status: z.literal("available"), bytes: seconds }),
		z.object({ status: z.literal("deleted"), reason: z.string() }),
		z.object({ status: z.literal("failed"), reason: z.string() }),
	]),
	processing: z.string(),
	last_error: z.string().nullable(),
}) satisfies z.ZodType<components["schemas"]["Call"]>;
export type Call = z.infer<typeof callSchema>;
export const eventSchema = z.object({
	id: seconds,
	kind: z.string(),
	call: callSchema.nullable(),
	created_at: seconds,
}) satisfies z.ZodType<components["schemas"]["Event"]>;
export type CallEvent = z.infer<typeof eventSchema>;
export const snapshotSchema = z.object({
	calls: z.array(callSchema),
	cursor: seconds,
}) satisfies z.ZodType<components["schemas"]["Snapshot"]>;
export const statusSchema = z.object({
	instance_id: z.uuid(),
	mode: z.enum(["lab", "production"]),
	gateway_ready: z.boolean(),
	ai_configured: z.boolean(),
	vapid_public_key: z.string().nullable(),
}) satisfies z.ZodType<components["schemas"]["Status"]>;
export type Status = z.infer<typeof statusSchema>;
export const settingsSchema = z.object({
	rules: z.record(z.string(), z.enum(["allow", "block", "screen"])),
	retention_days: z.number().int().min(1).max(365).nullable(),
	recording_limit_bytes: seconds.min(1048576).nullable(),
	webhook_url: z.url().nullable(),
}) satisfies z.ZodType<components["schemas"]["Settings"]>;
export type Settings = z.infer<typeof settingsSchema>;
export const sessionSchema = z.object({
	domain: z.string(),
	username: z.string(),
	password: z.string(),
}) satisfies z.ZodType<components["schemas"]["SipSession"]>;
export type SipCredentials = z.infer<typeof sessionSchema>;
export type Command = components["schemas"]["Command"];
