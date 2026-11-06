import { describe, expect, it } from "vitest";
import { type Call, callSchema } from "./contracts";
import { applyEvent, type Timeline } from "./timeline";

const exampleCall: Call = {
	id: "593ec5c7-89ae-4b81-9e42-6d24da709ae7",
	direction: "inbound",
	caller: "123",
	phase: "ringing",
	started_at: 10,
	ended_at: null,
	reason: null,
	transcript: [],
	summary: null,
	recording: { status: "none" },
	processing: "waiting",
	last_error: null,
};
describe("durable call events", (): void => {
	it("preserves unrelated calls, replaces a matching call, and advances empty events", (): void => {
		const other: Call = {
			...exampleCall,
			id: "593ec5c7-89ae-4b81-9e42-6d24da709ae8",
			caller: "456",
			started_at: 9,
		};
		const initial: Timeline = { cursor: 1, calls: [exampleCall, other] };
		const changed: Call = { ...exampleCall, phase: "ended", ended_at: 12 };
		const updated = applyEvent(initial, {
			id: 2,
			kind: "ended",
			call: changed,
			created_at: 12,
		});
		expect(updated.calls).toEqual([changed, other]);
		expect(
			applyEvent(updated, {
				id: 3,
				kind: "heartbeat",
				call: null,
				created_at: 13,
			}),
		).toEqual({ cursor: 3, calls: [changed, other] });
		expect(initial.calls).toEqual([exampleCall, other]);
	});
	it("orders simultaneous calls by identity and retains the latest two hundred calls", (): void => {
		const initial: Call[] = Array.from({ length: 201 }, (_, index) => ({
			...exampleCall,
			id: `00000000-0000-4000-8000-${index.toString(16).padStart(12, "0")}`,
			started_at: index,
		}));
		const latest: Call = { ...exampleCall, started_at: 201 };
		const event = { id: 4, kind: "incoming", call: latest, created_at: 201 };
		const updated = applyEvent({ cursor: 3, calls: initial }, event);
		expect(updated.calls).toHaveLength(200);
		expect(updated.calls[0]).toEqual(latest);
		expect(updated.calls.at(-1)?.started_at).toBe(2);
		const tied: Call = {
			...latest,
			id: "593ec5c7-89ae-4b81-9e42-6d24da709ae8",
		};
		expect(
			applyEvent({ cursor: 3, calls: [latest] }, { ...event, call: tied })
				.calls,
		).toEqual([tied, latest]);
	});
	it("rejects malformed external values", (): void => {
		expect(
			callSchema.safeParse({ ...exampleCall, phase: "unrecognized" }).success,
		).toBe(false);
		expect(
			callSchema.safeParse({
				...exampleCall,
				started_at: Number.MAX_SAFE_INTEGER + 1,
			}).success,
		).toBe(false);
		expect(
			callSchema.safeParse({ ...exampleCall, id: "../../private" }).success,
		).toBe(false);
		expect(callSchema.parse(exampleCall)).toEqual(exampleCall);
	});
	it("applies each event once and removes deleted calls", (): void => {
		const first: Timeline = { cursor: 1, calls: [] };
		const event = { id: 2, kind: "ringing", call: exampleCall, created_at: 10 };
		const updated = applyEvent(first, event);
		expect(updated.calls).toEqual([exampleCall]);
		expect(updated.cursor).toBe(2);
		expect(applyEvent(updated, event)).toBe(updated);
		expect(applyEvent(updated, { ...event, id: 1 })).toBe(updated);
		expect(
			applyEvent(updated, { ...event, id: 3, kind: "deleted" }).calls,
		).toEqual([]);
		expect(first.calls).toEqual([]);
	});
});
