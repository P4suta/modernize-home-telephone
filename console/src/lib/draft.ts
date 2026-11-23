import { SettingsDraft, type SettingsSnapshot } from "@homephone/settings-workflow";
import type { Changes, Field, Snapshot } from "./api";
export type Draft = {
	revision: string;
	baseline: Readonly<Record<string, Field>>;
	values: Record<string, string[]>;
	core: SettingsDraft;
};
export function settingsSnapshot(snapshot: Snapshot): SettingsSnapshot {
	return {
		revision: snapshot.revision,
		fields: snapshot.settings.map(({ field, help }) => ({
			id: field.id,
			identity: JSON.stringify([
				field.parameter,
				field.label,
				field.description,
				field.kind,
				field.secret,
				field.disabled,
				field.read_only,
				field.multiple,
				field.required,
				field.max_length,
				field.min,
				field.max,
				field.choices.map(({ value, label, disabled }) => [value, label, disabled]),
				help,
			]),
			access: !help
				? "undocumented"
				: help.read_only || field.disabled || field.read_only
					? "read-only"
					: "editable",
			value: field.secret ? { kind: "secret" } : { kind: "public", values: field.values },
		})),
	};
}
function withCore(snapshot: Snapshot, core: SettingsDraft): Draft {
	const fields = snapshot.settings.map((setting) => setting.field);
	return {
		revision: core.revision,
		baseline: Object.fromEntries(fields.map((field) => [field.id, structuredClone(field)])),
		values: core.values(),
		core,
	};
}
export function draftFrom(snapshot: Snapshot): Draft {
	return withCore(snapshot, SettingsDraft.capture(settingsSnapshot(snapshot)));
}
export function editDraft(draft: Draft, id: string, values: string[]): Draft {
	if (!Object.hasOwn(draft.baseline, id)) throw new Error("Unknown setting");
	const core = draft.core.edit(id, values);
	return { ...draft, core, values: core.values() };
}
export function changedValues(draft: Draft): Changes["values"] {
	return draft.core.changes();
}
export function validateChanges(
	snapshot: Snapshot,
	values: Changes["values"],
): Record<string, string> {
	const errors: Record<string, string> = {};
	for (const [id, selected] of Object.entries(values)) {
		const setting = snapshot.settings.find((setting) => setting.field.id === id);
		if (
			!setting?.help ||
			setting.help.read_only ||
			setting.field.disabled ||
			setting.field.read_only
		) {
			errors[id] = "変更できない項目です。";
			continue;
		}
		const field = setting.field;
		const help = setting.help;
		const scalar = ["text", "text_area", "password", "number"].includes(field.kind);
		if (
			selected.length > 256 ||
			(!field.multiple && selected.length > 1) ||
			(scalar && selected.length !== 1) ||
			((field.required || field.secret) && selected.every((value) => value === ""))
		) {
			errors[id] = "値を入力してください。";
			continue;
		}
		if (
			selected.some(
				(value) =>
					new TextEncoder().encode(value).length > 16384 ||
					(field.max_length != null && value.length > field.max_length) ||
					(field.kind !== "text_area" && /[\r\n]/.test(value)),
			)
		) {
			errors[id] = "文字数と改行を確認してください。";
			continue;
		}
		if (
			!scalar &&
			selected.some(
				(value) => !field.choices.some((choice) => choice.value === value && !choice.disabled),
			)
		) {
			errors[id] = "使用できる選択肢を選んでください。";
			continue;
		}
		if (
			(field.kind === "number" || help.ranges.length > 0) &&
			selected.some((value) => {
				const number = Number(value);
				return (
					!/^-?[0-9]+$/.test(value) ||
					!Number.isSafeInteger(number) ||
					number < -2147483648 ||
					number > 2147483647 ||
					(field.min != null && number < field.min) ||
					(field.max != null && number > field.max) ||
					(help.ranges.length > 0 &&
						!help.ranges.some((range) => range.min <= number && number <= range.max))
				);
			})
		)
			errors[id] = "指定された範囲の整数を入力してください。";
	}
	for (const { field } of snapshot.settings) {
		if (!field.required || field.disabled || field.read_only || field.secret) continue;
		if ((values[field.id] ?? field.values).every((value) => value === ""))
			errors[field.id] = "必須項目です。";
	}
	return errors;
}
export function rebaseDraft(draft: Draft, snapshot: Snapshot): Draft {
	try {
		return withCore(snapshot, draft.core.rebase(settingsSnapshot(snapshot)));
	} catch {
		throw new Error(
			"設定項目が変わったため自動で引き継げません。\n編集値を確認して入力し直してください。",
		);
	}
}
export function displayValue(field: Field, values: readonly string[]): string {
	if (field.secret) return values.length ? "変更あり" : "変更なし";
	return (
		values
			.map((value) => field.choices.find((choice) => choice.value === value)?.label ?? value)
			.join("、") || "未設定"
	);
}
