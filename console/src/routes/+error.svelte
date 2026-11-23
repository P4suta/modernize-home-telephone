<script lang="ts">
	import { resolve } from "$app/paths";
	import { page } from "$app/state";
	import Header from "#lib/Header.svelte";
	import Icon from "#lib/Icon.svelte";
</script>

<svelte:head
	><title>ページを表示できません · HT813</title><meta
		name="description"
		content="管理画面のページを開き直してください。"
	/></svelte:head
>
<Header active="error" />
<main id="main" class="error-page">
	<h1>ページを表示できません</h1>
	<p>エラー {page.status}</p>
	<a class="button" href={resolve("/status")}><Icon name="activity-label" />状態を開く</a><a
		class="button"
		href={resolve("/docs")}><Icon name="book" />ドキュメントを読む</a
	>
</main>
