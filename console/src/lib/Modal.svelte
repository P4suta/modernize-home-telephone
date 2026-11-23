<script lang="ts">
	import type { Snippet } from "svelte";
	import Icon from "./Icon.svelte";
	let {
		open = $bindable(false),
		title,
		children,
	}: { open: boolean; title: string; children: Snippet } = $props();
	let dialog: HTMLDialogElement | undefined = $state();
	let heading: HTMLHeadingElement | undefined = $state();
	const id = $props.id();
	$effect(() => {
		if (!dialog) return;
		if (open && !dialog.open) {
			dialog.showModal();
			heading?.focus();
		}
		if (!open && dialog.open) dialog.close();
	});
</script>

<dialog
	bind:this={dialog}
	aria-labelledby={id}
	onclose={(): void => {
		open = false;
	}}
	oncancel={(): void => {
		open = false;
	}}
>
	<header class="dialog-heading">
		<h2 {id} tabindex="-1" bind:this={heading}>{title}</h2>
		<button
			type="button"
			class="button"
			onclick={(): void => {
				open = false;
			}}><Icon name="save-action" />閉じる</button
		>
	</header>
	<div class="dialog-content">{@render children()}</div>
</dialog>
