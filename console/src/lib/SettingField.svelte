<script lang="ts">
	import type { Setting } from "./types";
	import Icon from "./Icon.svelte";
	import SettingHelp from "./SettingHelp.svelte";
	let {
		setting,
		values,
		error = "",
		pending = false,
		onchange,
	}: {
		setting: Setting;
		values: readonly string[];
		error?: string;
		pending?: boolean;
		onchange: (id: string, values: string[]) => void;
	} = $props();
	const field = $derived(setting.field);
	const disabled = $derived(pending || field.disabled || !setting.help || setting.help.read_only);
	const id = $derived(field.id);
	const described = $derived(error ? `${id}-error` : `${id}-hint`);
</script>

{#if field.kind === "radio" || field.kind === "checkbox"}
	<fieldset {id} class="field" disabled={disabled || field.read_only}>
		<legend
			><span class="field-label"
				><Icon name="settings-radio-label" />{field.label}<SettingHelp {setting} /></span
			></legend
		>
		<div class="choices">
			{#each field.choices as choice, index (choice.value)}
				<label class="choice" for={`${id}-${index}`}>
					<input
						id={`${id}-${index}`}
						name={id}
						type={field.kind === "radio" ? "radio" : "checkbox"}
						value={choice.value}
						checked={values.includes(choice.value)}
						disabled={choice.disabled}
						aria-describedby={described}
						aria-invalid={Boolean(error)}
						onchange={(event): void => {
							const selected = event.currentTarget.checked;
							onchange(
								id,
								field.kind === "radio"
									? [choice.value]
									: selected
										? [...values, choice.value]
										: values.filter((value) => value !== choice.value),
							);
						}}
					/>
					{choice.label}
				</label>
			{/each}
		</div>
		<p id={`${id}-hint`} class="field-hint">
			{#if field.read_only}読み取り専用の項目です。{/if}
			{field.required ? "必須" : ""}{!setting.help ? "説明を確認するまで編集できません。" : ""}
		</p>
		{#if error}<p id={`${id}-error`} class="field-error">{error}</p>{/if}
	</fieldset>
{:else}
	<div class="field">
		<div class="field-label-row">
			<label for={id}
				><Icon
					name={field.secret ? "key-label" : "settings-label"}
				/>{field.label}{#if field.required}<span class="required-label">必須</span>{/if}</label
			><SettingHelp {setting} />
		</div>
		{#if field.kind === "select"}
			<select
				{id}
				{disabled}
				multiple={field.multiple}
				required={field.required && !field.secret}
				aria-describedby={described}
				aria-invalid={Boolean(error)}
				onchange={(event): void => {
					onchange(
						id,
						Array.from(event.currentTarget.selectedOptions, (option) => option.value),
					);
				}}
			>
				{#if !field.multiple && values.length === 0}<option value="">未設定</option>{/if}
				{#each field.choices as choice (choice.value)}<option
						value={choice.value}
						selected={values.includes(choice.value)}
						disabled={choice.disabled || field.read_only}>{choice.label}</option
					>{/each}
			</select>
		{:else if field.kind === "text_area"}
			<textarea
				{id}
				name={id}
				rows="4"
				value={values[0] ?? ""}
				{disabled}
				readonly={field.read_only}
				maxlength={field.max_length ?? undefined}
				required={field.required && !field.secret}
				aria-describedby={described}
				aria-invalid={Boolean(error)}
				spellcheck="false"
				oninput={(event): void => {
					onchange(id, [event.currentTarget.value]);
				}}></textarea>
		{:else}
			<input
				{id}
				name={id}
				type={field.secret ? "password" : "text"}
				inputmode={field.kind === "number" || (setting.help?.ranges.length ?? 0) > 0
					? "numeric"
					: undefined}
				value={values[0] ?? ""}
				{disabled}
				readonly={field.read_only}
				maxlength={field.max_length ?? undefined}
				required={field.required && !field.secret}
				autocomplete={field.secret ? "new-password" : "off"}
				spellcheck="false"
				aria-describedby={described}
				aria-invalid={Boolean(error)}
				oninput={(event): void => {
					onchange(id, [event.currentTarget.value]);
				}}
			/>
		{/if}
		<p id={`${id}-hint`} class="field-hint">
			{#if field.read_only}読み取り専用の項目です。{/if}
			{#if field.secret}変更する場合だけ入力します。{/if}
			{#if field.min != null || field.max != null}入力範囲: {field.min ?? "指定なし"}〜{field.max ??
					"指定なし"}。{/if}
			{#if field.max_length != null}最大{field.max_length}文字。{/if}
			{#if !setting.help}説明を確認するまで編集できません。{/if}
		</p>
		{#if error}<p id={`${id}-error`} class="field-error">{error}</p>{/if}
	</div>
{/if}
