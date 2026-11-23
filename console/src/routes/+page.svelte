<script lang="ts">
	import { resolve } from "$app/paths";
	import { goto } from "$app/navigation";
	import Header from "#lib/Header.svelte";
	import LoginCard from "#lib/LoginCard.svelte";
</script>

<svelte:head
	><title>ログイン · HT813</title><meta
		name="description"
		content="HT813の状態と設定を日本語で管理する画面です。"
	/></svelte:head
>
<Header active="login" />
<main id="main" class="login-page">
	<LoginCard
		oncomplete={(): void => {
			void goto(resolve("/status"));
		}}
	/>
</main>
