import { z } from "zod";
import {
	type Call,
	type Command,
	callSchema,
	eventSchema,
	type Settings,
	type SipCredentials,
	type Status,
	sessionSchema,
	settingsSchema,
	snapshotSchema,
	statusSchema,
} from "./contracts";
import { applyEvent, type Timeline } from "./timeline";

const HTTP_CONFLICT = 409;
const HTTP_UNAUTHORIZED = 401;

export async function request<T>(
	path: string,
	schema: z.ZodType<T>,
	method = "GET",
	body?: unknown,
): Promise<T> {
	const response = await fetch(`/api${path}`, {
		method,
		credentials: "same-origin",
		headers: { "content-type": "application/json" },
		...(body === undefined ? {} : { body: JSON.stringify(body) }),
	});
	if (!response.ok)
		throw new Error(
			response.status === HTTP_CONFLICT
				? "電話回線が使用中か、通話状態が変わりました。"
				: response.status === HTTP_UNAUTHORIZED
					? "接続の認証を確認してください。"
					: "操作を完了できませんでした。接続状態を確認してください。",
		);
	const text = await response.text();
	const value: unknown = text === "" ? undefined : JSON.parse(text);
	return schema.parse(value);
}

export interface Workspace extends Timeline {
	readonly status: Status | null;
	readonly connected: boolean;
	readonly error: string | null;
}
export class PhoneStore {
	private snapshot: Workspace = {
		cursor: 0,
		calls: [],
		status: null,
		connected: false,
		error: null,
	};
	private listeners = new Set<() => void>();
	private source: EventSource | undefined;
	private running = false;
	subscribe = (listener: () => void): (() => void) => {
		this.listeners.add(listener);
		return (): void => {
			this.listeners.delete(listener);
		};
	};
	getSnapshot = (): Workspace => this.snapshot;
	private publish(next: Workspace): void {
		this.snapshot = next;
		for (const listener of this.listeners) listener();
	}
	start = (): void => {
		if (this.running || typeof EventSource === "undefined") return;
		this.running = true;
		window.addEventListener("offline", this.disconnect);
		window.addEventListener("online", this.connect);
		this.connect();
	};
	private connect = (): void => {
		if (!this.running || this.source || !navigator.onLine) return;
		const source = new EventSource("/api/events");
		this.source = source;
		source.addEventListener("snapshot", (event: MessageEvent<string>): void => {
			if (this.source !== source) return;
			try {
				const data: unknown = JSON.parse(event.data);
				const snapshot = snapshotSchema.parse(data);
				this.publish({
					...this.snapshot,
					...snapshot,
					connected: true,
					error: null,
				});
			} catch {
				this.fail();
			}
		});
		source.addEventListener("status", (event: MessageEvent<string>): void => {
			if (this.source !== source) return;
			try {
				const data: unknown = JSON.parse(event.data);
				this.publish({
					...this.snapshot,
					status: statusSchema.parse(data),
					connected: true,
					error: null,
				});
			} catch {
				this.fail();
			}
		});
		source.addEventListener("call", (event: MessageEvent<string>): void => {
			if (this.source !== source) return;
			try {
				const data: unknown = JSON.parse(event.data);
				const updated = applyEvent(this.snapshot, eventSchema.parse(data));
				this.publish({
					...this.snapshot,
					...updated,
					connected: true,
					error: null,
				});
			} catch {
				this.fail();
			}
		});
		source.addEventListener("unavailable", (): void => {
			if (this.source === source) this.fail();
		});
		source.onerror = (): void => {
			if (this.source === source)
				this.publish({ ...this.snapshot, connected: false, status: null });
		};
	};
	private disconnect = (): void => {
		this.source?.close();
		this.source = undefined;
		this.publish({ ...this.snapshot, connected: false, status: null });
	};
	stop = (): void => {
		this.running = false;
		window.removeEventListener("offline", this.disconnect);
		window.removeEventListener("online", this.connect);
		this.disconnect();
	};
	private fail(): void {
		this.publish({
			...this.snapshot,
			connected: false,
			error: "通話履歴を読み込めませんでした。再接続してください。",
		});
	}
	command = (id: string, command: Command): Promise<Call> =>
		request(`/calls/${id}/commands`, callSchema, "POST", command);
	call = (id: string): Promise<Call> => request(`/calls/${id}`, callSchema);
	originate = (number: string): Promise<Call> =>
		request("/calls", callSchema, "POST", { number });
	settings = (): Promise<Settings> => request("/settings", settingsSchema);
	saveSettings = (settings: Settings): Promise<Settings> =>
		request("/settings", settingsSchema, "PUT", settings);
	credentials = (): Promise<SipCredentials> =>
		request("/session", sessionSchema);
	deleteCall = (id: string): Promise<void> =>
		request(`/calls/${id}`, z.void(), "DELETE");
	labIncoming = (scenario: string): Promise<{ channel_id: string }> =>
		request("/lab/incoming", z.object({ channel_id: z.string() }), "POST", {
			scenario,
		});
}
export const phoneStore = new PhoneStore();
