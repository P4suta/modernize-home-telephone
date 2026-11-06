import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { PhoneStore } from "./service";

class EventStream extends EventTarget {
	static instances: EventStream[] = [];
	closed = false;
	onerror: (() => void) | undefined;
	constructor(readonly url: string) {
		super();
		EventStream.instances.push(this);
	}
	close(): void {
		this.closed = true;
	}
	event(name: string, data: unknown): void {
		this.dispatchEvent(new MessageEvent(name, { data: JSON.stringify(data) }));
	}
}
const status = {
	instance_id: "5d85948d-d676-4933-bc83-6e04562efe8b",
	mode: "lab",
	gateway_ready: true,
	ai_configured: true,
	vapid_public_key: null,
};
const call = {
	id: "5d85948d-d676-4933-bc83-6e04562efe8b",
	direction: "inbound",
	caller: "999001",
	phase: "ended",
	started_at: 1,
	ended_at: 2,
	reason: "ai_spam",
	transcript: [],
	summary: null,
	recording: { status: "none" },
	processing: "complete",
	last_error: null,
};
let store: PhoneStore;
beforeEach((): void => {
	EventStream.instances = [];
	vi.stubGlobal("EventSource", EventStream);
	vi.stubGlobal(
		"fetch",
		vi.fn().mockResolvedValue(new Response(JSON.stringify(status))),
	);
	store = new PhoneStore();
});
afterEach((): void => {
	store.stop();
	vi.unstubAllGlobals();
});
function stream(): EventStream {
	const value = EventStream.instances.at(-1);
	if (!value) throw new Error("The stream was not started");
	return value;
}
describe("workspace connection ownership", (): void => {
	it("updates gateway readiness from validated stream events without a status fetch", (): void => {
		store.start();
		stream().event("snapshot", { cursor: 0, calls: [] });
		stream().event("status", status);
		expect(store.getSnapshot().status).toEqual(status);
		stream().event("status", { ...status, gateway_ready: false });
		expect(store.getSnapshot().status?.gateway_ready).toBe(false);
		expect(fetch).not.toHaveBeenCalled();
		stream().onerror?.();
		expect(store.getSnapshot().status).toBeNull();
		stream().event("status", status);
		stream().event("status", { ...status, gateway_ready: "unvalidated" });
		expect(store.getSnapshot().error).not.toBeNull();
		stream().event("status", status);
		expect(store.getSnapshot().error).toBeNull();
		expect(store.getSnapshot().status?.gateway_ready).toBe(true);
	});
	it("closes the stream offline and takes a fresh snapshot after returning online", async (): Promise<void> => {
		store.start();
		const first = stream();
		first.event("snapshot", { cursor: 0, calls: [] });
		first.event("status", status);
		expect(store.getSnapshot().status).not.toBeNull();
		window.dispatchEvent(new Event("offline"));
		expect(first.closed).toBe(true);
		expect(store.getSnapshot().connected).toBe(false);
		expect(store.getSnapshot().status).toBeNull();
		window.dispatchEvent(new Event("online"));
		expect(EventStream.instances).toHaveLength(2);
		stream().event("snapshot", { cursor: 1, calls: [call] });
		expect(store.getSnapshot().calls).toEqual([call]);
		first.event("snapshot", { cursor: 0, calls: [] });
		expect(store.getSnapshot().cursor).toBe(1);
	});
	it("ignores status and snapshot callbacks from a stopped stream", (): void => {
		store.start();
		const first = stream();
		first.event("snapshot", { cursor: 0, calls: [] });
		store.stop();
		const stopped = store.getSnapshot();
		first.event("status", status);
		first.event("snapshot", { cursor: 1, calls: [call] });
		expect(store.getSnapshot()).toBe(stopped);
		window.dispatchEvent(new Event("online"));
		expect(EventStream.instances).toHaveLength(1);
	});
	it("clears a malformed event error when the stream supplies another valid event", (): void => {
		store.start();
		stream().event("call", { invalid: true });
		expect(store.getSnapshot().error).not.toBeNull();
		stream().event("call", { id: 1, kind: "ended", created_at: 2, call });
		expect(store.getSnapshot().error).toBeNull();
		expect(store.getSnapshot().connected).toBe(true);
	});
});
