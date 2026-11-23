export type SettingsField = {
	id: string;
	identity: string;
	access: "editable" | "read-only" | "undocumented";
	value: { kind: "public"; values: readonly string[] } | { kind: "secret" };
};
export type SettingsSnapshot = { revision: string; fields: readonly SettingsField[] };
export type SettingValues = Record<string, string[]>;
export type ReviewItem =
	| { id: string; kind: "public"; before: string[]; after: string[] }
	| { id: string; kind: "secret" };
export type Reconciliation = "confirmed" | "mismatch" | "verification-required";
export type SettingsFault =
	| "invalid-snapshot"
	| "unknown-field"
	| "not-editable"
	| "invalid-value"
	| "field-changed"
	| "stale-revision"
	| "no-changes"
	| "review-changed"
	| "invalid-transition";

export class SettingsError extends Error {
	constructor(public readonly code: SettingsFault) {
		super(code);
		this.name = "SettingsError";
	}
}

function record(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null;
}
function strings(value: unknown): value is string[] {
	return Array.isArray(value) && value.every((item: unknown) => typeof item === "string");
}
function setting(value: unknown): value is SettingsField {
	return (
		record(value) &&
		typeof value.id === "string" &&
		value.id.length > 0 &&
		typeof value.identity === "string" &&
		value.identity.length > 0 &&
		(value.access === "editable" ||
			value.access === "read-only" ||
			value.access === "undocumented") &&
		record(value.value) &&
		((value.value.kind === "secret" && !Object.hasOwn(value.value, "values")) ||
			(value.value.kind === "public" && strings(value.value.values)))
	);
}
function snapshot(value: unknown): value is SettingsSnapshot {
	return (
		record(value) &&
		typeof value.revision === "string" &&
		value.revision.length > 0 &&
		Array.isArray(value.fields) &&
		value.fields.every(setting) &&
		new Set(value.fields.map((field: SettingsField) => field.id)).size === value.fields.length
	);
}
function equal(left: readonly string[], right: readonly string[]): boolean {
	return left.length === right.length && left.every((value, index) => value === right[index]);
}
function sameField(left: SettingsField, right: SettingsField): boolean {
	return (
		left.identity === right.identity &&
		left.access === right.access &&
		left.value.kind === right.value.kind
	);
}
type Entry = { field: SettingsField; values: readonly string[] };

export class SettingsDraft {
	readonly revision: string;
	readonly #entries: ReadonlyMap<string, Entry>;
	private constructor(revision: string, entries: ReadonlyMap<string, Entry>) {
		this.revision = revision;
		this.#entries = entries;
		Object.freeze(this);
	}
	static capture(input: unknown): SettingsDraft {
		if (!snapshot(input)) throw new SettingsError("invalid-snapshot");
		const entries = new Map(
			input.fields.map((field) => [
				field.id,
				{
					field: structuredClone(field),
					values: field.value.kind === "public" ? [...field.value.values] : [],
				},
			]),
		);
		return new SettingsDraft(input.revision, entries);
	}
	values(): SettingValues {
		return Object.fromEntries([...this.#entries].map(([id, { values }]) => [id, [...values]]));
	}
	changes(): SettingValues {
		return Object.fromEntries(
			this.#changedEntries().map(({ field, values }) => [field.id, [...values]]),
		);
	}
	#changedEntries(): Entry[] {
		return [...this.#entries.values()].filter(
			({ field, values }) =>
				!equal(field.value.kind === "public" ? field.value.values : [], values),
		);
	}
	edit(id: string, values: readonly string[]): SettingsDraft {
		const entry = this.#entries.get(id);
		if (!entry) throw new SettingsError("unknown-field");
		const { field } = entry;
		if (field.access !== "editable") throw new SettingsError("not-editable");
		if (!strings(values) || (field.value.kind === "secret" && values.length > 1))
			throw new SettingsError("invalid-value");
		return new SettingsDraft(
			this.revision,
			new Map(this.#entries).set(id, { field, values: [...values] }),
		);
	}
	rebase(input: unknown): SettingsDraft {
		let fresh = SettingsDraft.capture(input);
		for (const { field, values } of this.#changedEntries()) {
			const after = fresh.#entries.get(field.id);
			if (!after || !sameField(field, after.field)) throw new SettingsError("field-changed");
			fresh = fresh.edit(field.id, values);
		}
		return fresh;
	}
	review(currentRevision: string): SettingsReview {
		if (this.revision !== currentRevision) throw new SettingsError("stale-revision");
		if (Object.keys(this.changes()).length === 0) throw new SettingsError("no-changes");
		if (
			this.#changedEntries().some(
				({ field, values }) => field.value.kind === "secret" && values[0] === "",
			)
		)
			throw new SettingsError("invalid-value");
		const items: ReviewItem[] = this.#changedEntries().map(({ field, values }) => {
			return field.value.kind === "secret"
				? { id: field.id, kind: "secret" }
				: { id: field.id, kind: "public", before: [...field.value.values], after: [...values] };
		});
		return new SettingsReview(this, items);
	}
	compare(input: unknown): Reconciliation {
		const fresh = SettingsDraft.capture(input);
		let secret = false;
		for (const { field, values } of this.#changedEntries()) {
			const after = fresh.#entries.get(field.id);
			if (!after || !sameField(field, after.field)) return "mismatch";
			if (after.field.value.kind === "secret") secret = true;
			else if (!equal(values, after.field.value.values)) return "mismatch";
		}
		return secret ? "verification-required" : "confirmed";
	}
}

class SettingsReview {
	readonly #draft: SettingsDraft;
	readonly #items: readonly ReviewItem[];
	constructor(draft: SettingsDraft, items: readonly ReviewItem[]) {
		this.#draft = draft;
		this.#items = items;
	}
	items(): ReviewItem[] {
		return structuredClone([...this.#items]);
	}
	submit(draft: SettingsDraft, currentRevision: string): SettingsSubmission {
		if (draft !== this.#draft) throw new SettingsError("review-changed");
		if (draft.revision !== currentRevision) throw new SettingsError("stale-revision");
		return new SettingsSubmission(draft);
	}
}
class SettingsSubmission {
	readonly #draft: SettingsDraft;
	constructor(draft: SettingsDraft) {
		this.#draft = draft;
	}
	get revision(): string {
		return this.#draft.revision;
	}
	values(): SettingValues {
		return this.#draft.changes();
	}
	reconcile(input: unknown): Reconciliation {
		return this.#draft.compare(input);
	}
}
