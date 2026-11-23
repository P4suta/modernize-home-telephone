import {
	type Reconciliation,
	type ReviewItem,
	type SettingsDraft,
	SettingsError,
} from "./index.js";

type Review = ReturnType<SettingsDraft["review"]>;
type Submission = ReturnType<Review["submit"]>;
type State =
	| { phase: "editing" }
	| { phase: "reviewing"; review: Review }
	| { phase: "submitting"; submission: Submission; review: Review }
	| { phase: "reconciliation-required" };
export interface SettingsWorkflow {
	readonly phase: State["phase"];
	readonly items: ReviewItem[];
	review(draft: SettingsDraft, currentRevision: string): void;
	begin(draft: SettingsDraft, currentRevision: string): Submission;
	complete(readback?: unknown): Reconciliation;
	interrupt(): void;
	reset(): void;
	cancel(): void;
}

export function createSettingsWorkflow(): SettingsWorkflow {
	let state = $state.raw<State>({ phase: "editing" });
	return {
		get phase(): State["phase"] {
			return state.phase;
		},
		get items(): ReviewItem[] {
			return state.phase === "reviewing" || state.phase === "submitting"
				? state.review.items()
				: [];
		},
		review(draft: SettingsDraft, currentRevision: string): void {
			if (state.phase !== "editing" && state.phase !== "reviewing")
				throw new SettingsError("invalid-transition");
			state = { phase: "reviewing", review: draft.review(currentRevision) };
		},
		begin(draft: SettingsDraft, currentRevision: string): Submission {
			if (state.phase !== "reviewing") throw new SettingsError("invalid-transition");
			const submission = state.review.submit(draft, currentRevision);
			state = { phase: "submitting", submission, review: state.review };
			return submission;
		},
		complete(readback?: unknown): Reconciliation {
			if (state.phase !== "submitting") throw new SettingsError("invalid-transition");
			const outcome = readback === undefined ? "mismatch" : state.submission.reconcile(readback);
			state = { phase: outcome === "confirmed" ? "editing" : "reconciliation-required" };
			return outcome;
		},
		interrupt(): void {
			if (state.phase !== "submitting") throw new SettingsError("invalid-transition");
			state = { phase: "reconciliation-required" };
		},
		reset(): void {
			if (state.phase === "submitting") throw new SettingsError("invalid-transition");
			state = { phase: "editing" };
		},
		cancel(): void {
			if (state.phase !== "reviewing" && state.phase !== "editing")
				throw new SettingsError("invalid-transition");
			state = { phase: "editing" };
		},
	};
}
