<script lang="ts">
	import { resolve } from "$app/paths";
	import { browser } from "$app/env";
	import { createQuery, useQueryClient } from "@tanstack/svelte-query";
	import {
		createSettingsWorkflow,
		type SettingsWorkflow,
	} from "@homephone/settings-workflow/svelte";
	import { ApiError, getPage, getSession, submit, type PageId, type Changes } from "./api";
	import {
		changedValues,
		displayValue,
		draftFrom,
		editDraft,
		rebaseDraft,
		settingsSnapshot,
		validateChanges,
	} from "./draft";
	import { sections } from "./navigation";
	import { useConsole } from "./state.svelte";
	import Header from "./Header.svelte";
	import Icon from "./Icon.svelte";
	import LoginCard from "./LoginCard.svelte";
	import Modal from "./Modal.svelte";
	import SettingField from "./SettingField.svelte";

	let { pageId }: { pageId: PageId } = $props();
	const consoleState = useConsole();
	const workflows = {
		status: createSettingsWorkflow(),
		basic: createSettingsWorkflow(),
		advanced: createSettingsWorkflow(),
		fxs: createSettingsWorkflow(),
		fxo: createSettingsWorkflow(),
	} satisfies Record<PageId, SettingsWorkflow>;
	const workflow = $derived(workflows[pageId]);
	const client = useQueryClient();
	const session = createQuery(() => ({
		queryKey: ["session"],
		queryFn: getSession,
		enabled: browser,
	}));
	const query = createQuery(() => ({
		queryKey: ["device", pageId],
		queryFn: () => getPage(pageId),
		enabled: browser && session.data?.authenticated === true,
	}));
	const section = $derived(sections.find((section) => section.id === pageId));
	const snapshot = $derived(query.data);
	const draft = $derived(consoleState.drafts[pageId]);
	const changes = $derived(draft ? changedValues(draft) : {});
	const count = $derived(Object.keys(changes).length);
	const stale = $derived(Boolean(snapshot && draft && snapshot.revision !== draft.revision));
	const categories = $derived(
		Array.from(
			new Set(
				snapshot?.settings.map((setting) => setting.help?.category ?? "説明の確認が必要な項目") ??
					[],
			),
		),
	);
	let review = $state(false);
	let discard = $state(false);
	let saving = $state(false);
	const needsReadback = $derived(workflow.phase === "reconciliation-required");
	let mode = $state<Changes["mode"]>("save");
	let message = $state("");
	let error = $state("");
	let errors = $state<Record<string, string>>({});
	let errorSummary: HTMLDivElement | undefined = $state();

	$effect(() => {
		const current = consoleState.drafts[pageId];
		if (
			snapshot &&
			(!current ||
				(current.revision !== snapshot.revision &&
					Object.keys(changedValues(current)).length === 0))
		) {
			consoleState.drafts[pageId] = draftFrom(snapshot);
		}
	});
	$effect(() => {
		if (
			query.error instanceof ApiError &&
			query.error.status === 401 &&
			session.data?.authenticated
		) {
			client.setQueryData(["session"], { authenticated: false, username: null });
		}
	});
	function change(id: string, values: string[]): void {
		if (!draft || saving) return;
		consoleState.drafts[pageId] = editDraft(draft, id, values);
		if (Object.hasOwn(errors, id)) {
			const updated = { ...errors };
			delete updated[id];
			errors = updated;
		}
		message = "";
	}
	async function refresh(): Promise<void> {
		if (query.isFetching || saving) return;
		const targetWorkflow = workflow;
		const result = await query.refetch();
		if (result.isSuccess) targetWorkflow.reset();
	}
	function reviewChanges(event: SubmitEvent): void {
		event.preventDefault();
		if (!snapshot || !draft || saving || stale || needsReadback || count === 0) return;
		errors = validateChanges(snapshot, changes);
		if (Object.keys(errors).length > 0) {
			queueMicrotask(() => errorSummary?.focus());
			return;
		}
		error = "";
		workflow.review(draft.core, snapshot.revision);
		review = true;
	}
	function rebase(): void {
		if (!snapshot || !draft) return;
		try {
			consoleState.drafts[pageId] = rebaseDraft(draft, snapshot);
			workflow.reset();
			error = "";
		} catch (cause) {
			error = cause instanceof Error ? cause.message : "編集値を確認してください。";
		}
	}
	function discardChanges(): void {
		if (snapshot) consoleState.drafts[pageId] = draftFrom(snapshot);
		workflow.reset();
		discard = false;
		errors = {};
		error = "";
		message = "";
	}
	async function save(): Promise<void> {
		if (
			saving ||
			!draft ||
			!snapshot ||
			stale ||
			needsReadback ||
			count === 0 ||
			workflow.phase !== "reviewing"
		)
			return;
		errors = validateChanges(snapshot, changes);
		if (Object.keys(errors).length > 0) {
			review = false;
			return;
		}
		const targetWorkflow = workflow;
		const submitted = targetWorkflow.begin(draft.core, snapshot.revision);
		saving = true;
		error = "";
		message = "";
		const targetPage = pageId;
		const request: Changes = { revision: submitted.revision, mode, values: submitted.values() };
		try {
			const result = await submit(targetPage, request);
			if (consoleState.drafts[targetPage]?.revision !== request.revision) return;
			if (result.snapshot) client.setQueryData(["device", targetPage], result.snapshot);
			const outcome = targetWorkflow.complete(
				result.outcome !== "uncertain" && result.snapshot
					? settingsSnapshot(result.snapshot)
					: undefined,
			);
			if (result.outcome === "confirmed" && result.snapshot && outcome === "confirmed") {
				consoleState.drafts[targetPage] = draftFrom(result.snapshot);
				message =
					mode === "save"
						? "保存済みの値を再取得して確認しました。\n反映や再起動が必要な項目は説明を確認してください。"
						: "反映を要求しました。\n読み戻した設定値との一致を確認しました。\n通話への反映は動作で確認してください。";
			} else if (result.outcome === "verification_required") {
				message =
					"送信後の応答を受信しました。\n秘密情報は読み戻せないため、接続や認証の動作を確認してください。";
			} else {
				message =
					"保存結果を確定できません。\n設定を自動で再送していません。\n取得した値と編集内容を比較してください。";
			}
			review = false;
		} catch (cause) {
			if (targetWorkflow.phase === "submitting") targetWorkflow.interrupt();
			if (consoleState.drafts[targetPage]?.revision !== request.revision) return;
			error = cause instanceof Error ? cause.message : "保存に失敗しました。";
			if (cause instanceof ApiError && cause.status === 401)
				client.setQueryData(["session"], { authenticated: false, username: null });
			if (cause instanceof ApiError && cause.code === "stale_revision") {
				const result = await query.refetch();
				if (result.isSuccess) targetWorkflow.reset();
			}
		} finally {
			if (targetWorkflow.phase === "submitting") targetWorkflow.interrupt();
			saving = false;
		}
	}
</script>

<svelte:head
	><title>{section?.title ?? "管理画面"} · HT813</title><meta
		name="description"
		content="HT813の状態と設定を日本語で確認します。"
	/></svelte:head
>
<Header active={pageId} />
<main id="main">
	{#if session.isPending}
		<div class="loading-page" role="status">
			<h1>{section?.title}</h1>
			<p>ログイン状態を確認中…</p>
		</div>
	{:else if !session.data?.authenticated}
		<div class="login-page">
			<LoginCard
				recovery={Boolean(draft)}
				oncomplete={(): void => {
					error = "";
				}}
			/>
		</div>
	{:else}
		<div class="page-layout">
			<aside class="section-navigation">
				<p class="caption">設定項目</p>
				<nav aria-label="ページ内">
					{#if pageId === "status"}<a href="#overview"><Icon name="activity-label" />機器情報</a
						>{/if}
					{#each categories as category, index (category)}<a href={`#group-${index}`}
							><Icon name="settings-label" />{category}</a
						>{/each}
					<a href={resolve("/docs")}><Icon name="book" />設定の説明</a>
				</nav>
			</aside>
			<div class="main-content">
				<div class="page-heading">
					<div>
						<h1>{section?.title}</h1>
						<p>{section?.native}</p>
					</div>
					<button
						type="button"
						class="button"
						disabled={query.isFetching || saving}
						onclick={(): void => {
							void refresh();
						}}><Icon name="refresh-action" />{query.isFetching ? "取得中…" : "再取得"}</button
					>
				</div>
				<p class="retrieved-time" role="status">
					{#if snapshot}最終取得 <time datetime={new Date(snapshot.fetched_at_ms).toISOString()}
							>{new Date(snapshot.fetched_at_ms).toLocaleString("ja-JP")}</time
						>{:else}機器の応答を待っています。{/if}
				</p>
				{#if query.isFetching}<p class="notice" role="status">
						機器から読み取り中です。<br />取得済みの値を表示しています。
					</p>{/if}
				{#if query.error}<p role="alert" class="notice danger">{query.error.message}</p>{/if}
				{#if error}<p role="alert" class="notice danger">{error}</p>{/if}
				{#if message}<p role="status" class="notice">{message}</p>{/if}
				{#if saving}<p role="status" class="notice">
						機器に送信し、保存後の値を確認しています。<br />機器の応答を待つ間は再送できません。
					</p>{/if}
				{#if snapshot?.native_session_recovered}<p class="notice">
						機器の認証を回復しました。<br />編集中の値は残しています。
					</p>{/if}
				{#if stale}<section class="notice" aria-label="機器の値の変更">
						<h2>機器の値が変わりました</h2>
						<p>編集内容を残しています。<br />最新の機器の値を基準にして差分を確認してください。</p>
						<button type="button" class="button" onclick={rebase}
							><Icon name="refresh-action" />最新値と編集内容を比較する</button
						>
					</section>{/if}
				{#if needsReadback}<p class="notice danger">
						保存後の値を確認できていません。<br />再取得してから編集を再開してください。
					</p>{/if}
				{#if snapshot && pageId === "status"}
					<section id="overview">
						<h2>機器情報</h2>
						<dl class="information-list">
							{#each snapshot.information as item, index (index)}<div>
									<dt>{item.label}</dt>
									<dd>{item.value}</dd>
								</div>{/each}
						</dl>
					</section>
				{:else if snapshot && draft}
					{#if !snapshot.can_save && !snapshot.can_apply}<p class="notice">
							機器の送信処理を確認できないため、読み取り専用で表示しています。
						</p>{/if}
					{#if snapshot.undocumented > 0}<p class="notice danger">
							説明が未登録の項目が{snapshot.undocumented}件あります。<br
							/>未登録の項目は編集できません。
						</p>{/if}
					<form method="post" action="/api/console/session" novalidate onsubmit={reviewChanges}>
						{#if Object.keys(errors).length > 0}<div
								class="notice danger"
								role="alert"
								tabindex="-1"
								bind:this={errorSummary}
							>
								<h2>入力値を確認してください</h2>
								<ul>
									{#each Object.entries(errors) as [id, text] (id)}<li>
											<a href={`#${id}`}
												>{snapshot.settings.find((setting) => setting.field.id === id)?.field
													.label ?? id}: {text}</a
											>
										</li>{/each}
								</ul>
							</div>{/if}
						{#each categories as category, index (category)}
							<section class="settings-section" id={`group-${index}`}>
								<h2>{category}</h2>
								{#each snapshot.settings.filter((setting) => (setting.help?.category ?? "説明の確認が必要な項目") === category) as setting (setting.field.id)}
									<SettingField
										{setting}
										values={draft.values[setting.field.id] ?? setting.field.values}
										error={errors[setting.field.id] ?? ""}
										pending={saving || (!snapshot.can_save && !snapshot.can_apply)}
										onchange={change}
									/>
								{/each}
							</section>
						{/each}
						<div class="save-bar">
							<p>{count > 0 ? `未保存の変更 ${count}件` : "未保存の変更はありません"}</p>
							<div class="actions">
								<button
									type="button"
									class="button"
									disabled={count === 0 || saving}
									onclick={(): void => {
										discard = true;
									}}><Icon name="discard-action" />変更を破棄</button
								>
								<button
									type="submit"
									class="button primary"
									disabled={count === 0 ||
										saving ||
										query.isFetching ||
										stale ||
										needsReadback ||
										(!snapshot.can_save && !snapshot.can_apply)}
									><Icon name="check-primary" />変更を確認</button
								>
							</div>
						</div>
					</form>
				{/if}
			</div>
		</div>
	{/if}
</main>
<Modal bind:open={discard} title="変更を破棄">
	<p>編集中の値を破棄して、取得済みの機器の値へ戻します。</p>
	<div class="actions">
		<button
			type="button"
			class="button"
			onclick={(): void => {
				discard = false;
			}}><Icon name="save-action" />編集を続ける</button
		><button type="button" class="button danger-button" onclick={discardChanges}
			><Icon name="discard-action" />変更を破棄</button
		>
	</div>
</Modal>
<Modal bind:open={review} title="変更内容を確認">
	<p>変更する項目と、保存する方法を確認してください。</p>
	{#if draft}
		<div class="review-table">
			<table>
				<caption>送信する変更 {count}件</caption><thead
					><tr
						><th scope="col">項目</th><th scope="col">取得時の値</th><th scope="col">変更後</th></tr
					></thead
				>
				<tbody
					>{#each Object.entries(changes) as [id, values] (id)}{@const field =
							draft.baseline[id]}{#if field}<tr
								><th scope="row">{field.label}</th><td>{displayValue(field, field.values)}</td><td
									>{displayValue(field, values)}</td
								></tr
							>{/if}{/each}</tbody
				>
			</table>
		</div>
	{/if}
	<fieldset>
		<legend>保存する方法</legend><label class="choice"
			><input
				type="radio"
				name="save-mode"
				value="save"
				bind:group={mode}
				disabled={!snapshot?.can_save || saving}
			/>保存（Update）</label
		>
		<p class="field-hint">設定を保存します。<br />反映は項目の説明を確認してください。</p>
		<label class="choice"
			><input
				type="radio"
				name="save-mode"
				value="save_and_apply"
				bind:group={mode}
				disabled={!snapshot?.can_apply || saving}
			/>保存して反映（Apply）</label
		>
		<p class="field-hint">設定を保存して反映します。<br />通話や接続に影響する場合があります。</p>
	</fieldset>
	{#if saving}<p role="status">機器への送信後、値を確認しています。</p>{/if}
	<div class="actions">
		<button
			type="button"
			class="button"
			disabled={saving}
			onclick={(): void => {
				review = false;
			}}><Icon name="discard-action" />編集へ戻る</button
		>
		<button
			type="button"
			class="button primary"
			disabled={saving || stale || (mode === "save" ? !snapshot?.can_save : !snapshot?.can_apply)}
			onclick={(): void => {
				void save();
			}}
			><Icon name="check-primary" />{saving
				? "保存結果を確認中…"
				: mode === "save"
					? "保存"
					: "保存して反映"}</button
		>
	</div>
</Modal>
