<script lang="ts">
	import { resolve } from "$app/paths";
	import type { Setting } from "./types";
	import Icon from "./Icon.svelte";
	import HelpContent from "./HelpContent.svelte";
	let { setting }: { setting: Setting } = $props();
	const id = $props.id();
</script>

<button
	type="button"
	class="help-trigger"
	popovertarget={id}
	aria-label={`${setting.field.label}の説明`}><Icon name="info" /></button
>
<aside {id} popover="auto" class="help-popover" aria-labelledby={`${id}-heading`}>
	<header class="dialog-heading">
		<h2 id={`${id}-heading`}>{setting.field.label}</h2>
		<button type="button" class="button" popovertarget={id} popovertargetaction="hide"
			><Icon name="discard-action" />閉じる</button
		>
	</header>
	<HelpContent help={setting.help} field={setting.field} />
	{#if setting.help}<a
			class="button"
			href={resolve(
				`/docs?q=${encodeURIComponent(setting.help.parameters[0] ?? "")}#${setting.help.key}`,
			)}><Icon name="book" />詳しい説明を読む</a
		>{/if}
</aside>
