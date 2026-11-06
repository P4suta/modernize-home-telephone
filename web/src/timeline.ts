import type { Call, CallEvent } from "./contracts";

const HISTORY_LIMIT = 200;

export interface Timeline {
	readonly cursor: number;
	readonly calls: readonly Call[];
}
export function applyEvent(timeline: Timeline, event: CallEvent): Timeline {
	if (event.id <= timeline.cursor) return timeline;
	let calls = [...timeline.calls];
	if (event.call) {
		const call = event.call;
		calls = calls.filter((entry) => entry.id !== call.id);
		if (event.kind !== "deleted") calls.push(call);
		calls.sort(
			(left, right) =>
				right.started_at - left.started_at || right.id.localeCompare(left.id),
		);
	}
	return { cursor: event.id, calls: calls.slice(0, HISTORY_LIMIT) };
}
