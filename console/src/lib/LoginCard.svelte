<script lang="ts">
	import { resolve } from "$app/paths";
	import { useQueryClient } from "@tanstack/svelte-query";
	import { signIn } from "./api";
	import Icon from "./Icon.svelte";
	let { oncomplete, recovery = false }: { oncomplete: () => void; recovery?: boolean } = $props();
	const client = useQueryClient();
	let username = $state("admin");
	let password = $state("");
	let visible = $state(false);
	let pending = $state(false);
	let error = $state("");
	async function login(event: SubmitEvent): Promise<void> {
		event.preventDefault();
		if (pending) return;
		pending = true;
		error = "";
		try {
			const result = await signIn(username, password);
			password = "";
			client.removeQueries({ queryKey: ["device"] });
			client.setQueryData(["session"], result);
			oncomplete();
		} catch (cause) {
			error = cause instanceof Error ? cause.message : "ログインに失敗しました。";
		} finally {
			pending = false;
		}
	}
</script>

<section class="login-card" aria-labelledby="login-title">
	<div class="login-mark"><Icon name="phone-login" /></div>
	<h1 id="login-title">{recovery ? "ログインして編集を再開" : "HT813にログイン"}</h1>
	<p class="login-description">
		{recovery ? "編集中の値を残しています。" : "機器の管理者アカウントを使用します。"}
	</p>
	{#if error}<p role="alert" class="notice danger">{error}</p>{/if}
	<form
		method="post"
		action="/api/console/session"
		onsubmit={(event): void => {
			void login(event);
		}}
	>
		<div class="field">
			<label for="username"><Icon name="user-label" />ユーザー名</label>
			<input
				id="username"
				name="username"
				autocomplete="username"
				required
				maxlength="30"
				bind:value={username}
				disabled={pending}
			/>
		</div>
		<div class="field">
			<label for="password"><Icon name="lock-label" />パスワード</label>
			<div class="password-control">
				<input
					id="password"
					name="password"
					type={visible ? "text" : "password"}
					autocomplete="current-password"
					required
					maxlength="30"
					bind:value={password}
					disabled={pending}
				/>
				<button
					type="button"
					class="help-trigger"
					aria-label={visible ? "パスワードを隠す" : "パスワードを表示"}
					aria-pressed={visible}
					onclick={(): void => {
						visible = !visible;
					}}><Icon name="eye-action" /></button
				>
			</div>
		</div>
		<button type="submit" class="button primary login-submit" disabled={pending}
			><Icon name="arrow-primary" />{pending ? "機器に接続中…" : "ログイン"}</button
		>
		<p class="caption" role="status">{pending ? "機器の応答に時間がかかる場合があります。" : ""}</p>
	</form>
	<a class="button text-button" href={resolve("/docs")}
		><Icon name="book" />接続と設定の説明を読む</a
	>
</section>
