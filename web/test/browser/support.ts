import { type APIRequestContext, expect } from "@playwright/test";
import { z } from "zod";
import {
	type Call,
	callSchema,
	type Settings,
	settingsSchema,
} from "../../src/contracts";

export const origin = "http://localhost:8787";
const HTTP_OK = 200;
const HTTP_ACCEPTED = 202;
const RECORDING_COMPLETION_MS = 10000;
export async function calls(request: APIRequestContext): Promise<Call[]> {
	const response = await request.get("/api/calls");
	expect(response.status()).toBe(HTTP_OK);
	return z.array(callSchema).parse(await response.json());
}
export async function incoming(
	request: APIRequestContext,
	scenario: string,
): Promise<string> {
	const previous = new Set((await calls(request)).map((call) => call.id));
	const response = await request.post("/api/lab/incoming", {
		headers: { Origin: origin },
		data: { scenario },
	});
	expect(response.status()).toBe(HTTP_ACCEPTED);
	let id: string | undefined;
	await expect
		.poll(async (): Promise<string | undefined> => {
			id = (await calls(request)).find((call) => !previous.has(call.id))?.id;
			return id;
		})
		.toBeDefined();
	if (!id) throw new Error("The gateway did not admit the new call");
	return id;
}
export async function command(
	request: APIRequestContext,
	id: string,
	action: string,
): Promise<void> {
	const response = await request.post(`/api/calls/${id}/commands`, {
		headers: { Origin: origin },
		data: { action },
	});
	expect(response.status()).toBe(HTTP_OK);
}
export async function settings(request: APIRequestContext): Promise<Settings> {
	const response = await request.get("/api/settings");
	expect(response.status()).toBe(HTTP_OK);
	return settingsSchema.parse(await response.json());
}
export async function saveSettings(
	request: APIRequestContext,
	value: Settings,
): Promise<void> {
	const response = await request.put("/api/settings", {
		headers: { Origin: origin },
		data: value,
	});
	expect(response.status()).toBe(HTTP_OK);
}
export async function completed(
	request: APIRequestContext,
	id: string,
): Promise<void> {
	await expect
		.poll(
			async (): Promise<boolean> =>
				(await calls(request)).some(
					(call) =>
						call.id === id &&
						call.phase === "ended" &&
						call.processing === "complete" &&
						call.recording.status === "available",
				),
			{ timeout: RECORDING_COMPLETION_MS },
		)
		.toBe(true);
}
