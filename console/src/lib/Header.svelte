<script lang="ts">
	import { resolve } from "$app/paths";
	import { browser } from "$app/env";
	import { goto } from "$app/navigation";
	import { createQuery, useQueryClient } from "@tanstack/svelte-query";
	import { getSession, signOut } from "./api";
	import { sections } from "./navigation";
	import { useConsole } from "./state.svelte";
	import Icon from "./Icon.svelte";
	import Modal from "./Modal.svelte";
	let { active }: { active: string } = $props();
	const consoleState = useConsole();
	const client = useQueryClient();
	const session = createQuery(() => ({
		queryKey: ["session"],
		queryFn: getSession,
		enabled: browser,
	}));
	let confirm = $state(false);
	let pending = $state(false);
	let error = $state("");
	async function logout(): Promise<void> {
		if (pending) return;
		pending = true;
		error = "";
		try {
			await signOut();
			consoleState.clear();
			client.clear();
			client.setQueryData(["session"], { authenticated: false, username: null });
			confirm = false;
			await goto(resolve("/"));
		} catch (cause) {
			error = cause instanceof Error ? cause.message : "ログアウトに失敗しました。";
		} finally {
			pending = false;
		}
	}
</script>

<a class="skip-link" href="#main">本文へ移動</a>
<header class="app-header">
	<a class="brand" href={resolve("/status")}
		><Icon name="phone-brand" /><strong>HT813</strong><span class="brand-divider">/</span><span
			>管理画面</span
		></a
	>
	<div class="header-actions">
		{#if session.data?.authenticated}
			<span class="session-name">{session.data.username}</span>
			<button
				type="button"
				class="button"
				onclick={(): void => {
					if (consoleState.dirty) confirm = true;
					else void logout();
				}}
				disabled={pending}><Icon name="logout-action" />ログアウト</button
			>
		{:else if active !== "login"}<a class="button" href={resolve("/")}
				><Icon name="user-label" />ログイン</a
			>{/if}
	</div>
</header>
{#if active !== "login"}
	<nav class="main-navigation" aria-label="管理画面">
		{#each sections as section (section.id)}
			<a href={resolve(section.path)} aria-current={active === section.id ? "page" : undefined}
				><Icon name={section.icon} />{section.title}</a
			>
		{/each}
	</nav>
	<div class="mobile-navigation">
		<label for="section-picker"><Icon name="settings-label" />表示する画面</label>
		<select
			id="section-picker"
			value={active}
			onchange={(event): void => {
				const found = sections.find((section) => section.id === event.currentTarget.value);
				if (found) void goto(resolve(found.path));
			}}
		>
			{#each sections as section (section.id)}<option value={section.id}>{section.title}</option
				>{/each}
		</select>
	</div>
{/if}
{#if error}<p role="alert" class="notice danger">{error}</p>{/if}
<Modal bind:open={confirm} title="編集内容を破棄してログアウト">
	<p>未保存の編集内容を破棄します。</p>
	<div class="actions">
		<button
			type="button"
			class="button"
			onclick={(): void => {
				confirm = false;
			}}><Icon name="discard-action" />編集を続ける</button
		>
		<button
			type="button"
			class="button danger-button"
			disabled={pending}
			onclick={(): void => {
				void logout();
			}}><Icon name="logout-action" />ログアウト</button
		>
	</div>
</Modal>
