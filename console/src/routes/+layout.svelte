<script lang="ts">
	import { resolve } from "$app/paths";
	import { QueryClient, QueryClientProvider } from "@tanstack/svelte-query";
	import type { LayoutProps } from "./$types";
	import { provideConsole } from "#lib/state.svelte.ts";
	import "#lib/app.css";
	let { children }: LayoutProps = $props();
	const client = new QueryClient({
		defaultOptions: {
			queries: {
				retry: false,
				refetchOnWindowFocus: false,
				refetchOnReconnect: false,
				staleTime: Infinity,
				gcTime: 30 * 60 * 1000,
			},
			mutations: { retry: false },
		},
	});
	const state = provideConsole();
</script>

<svelte:head
	><link rel="icon" type="image/svg+xml" href="/icons/phone-brand.svg" /><meta
		name="robots"
		content="noindex, nofollow"
	/><meta name="color-scheme" content="light" /></svelte:head
>
<svelte:window
	onbeforeunload={(event: BeforeUnloadEvent): void => {
		if (state.dirty) event.preventDefault();
	}}
/>
<noscript
	><p class="notice">
		管理画面の操作にはJavaScriptが必要です。<br />ドキュメントはJavaScriptなしでも読めます。
	</p>
	<a href={resolve("/docs")}>ドキュメントを読む</a></noscript
>
<QueryClientProvider {client}>{@render children()}</QueryClientProvider>
