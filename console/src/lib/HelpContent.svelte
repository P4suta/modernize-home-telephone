<script lang="ts">
	import type { Help, Field } from "./api";
	let { help, field }: { help: Help | null | undefined; field?: Field } = $props();
</script>

{#if help}
	<p class="prose">{help.summary}</p>
	{#if help.note}<p class="notice">{help.note}</p>{/if}
	<dl class="help-facts">
		<dt>入力する値</dt>
		<dd>{help.input}</dd>
		{#if field}
			<dt>接続中の機器の入力条件</dt>
			<dd>
				{#if field.min != null || field.max != null}最小 {field.min ?? "指定なし"} / 最大 {field.max ??
						"指定なし"}<br />{/if}
				{#if field.max_length != null}最大 {field.max_length}文字<br />{/if}
				{field.required ? "必須項目" : "空欄の扱いは項目の説明を確認"}
				{#if field.choices.length > 0}<br />選択肢: {field.choices
						.filter((choice) => !choice.disabled)
						.map((choice) => choice.label)
						.join("、")}{/if}
			</dd>
		{/if}
		<dt>工場出荷時の値（資料）</dt>
		<dd>{help.default_value}</dd>
		<dt>テンプレートの記載値</dt>
		<dd>
			{#if help.reference.kind === "device"}
				保存テンプレートに記載がありません。
			{:else}
				{help.template_value === null
					? "秘密情報は表示しません。"
					: help.template_value === ""
						? "空欄"
						: help.template_value}<br />工場出荷時の値とは限りません。
			{/if}
		</dd>
		<dt>原典の参照箇所</dt>
		<dd>
			{#if help.reference.kind === "template"}テンプレートのヘッダー: {help.reference.firmware} / {help
					.reference.line}行目{:else}HT813 {help.reference.firmware}の入力欄と説明資料{/if}
		</dd>
		<dt>通常の操作</dt>
		<dd>{help.recommendation}</dd>
		<dt>変更の影響</dt>
		<dd>{help.impact}</dd>
	</dl>
	<p class="source-link">
		<a href={help.source} target="_blank" rel="external noreferrer">説明の根拠を別のタブで読む</a>
	</p>
{:else}
	<p>項目の説明が未登録です。</p>
	<p>入力条件と初期値を確認するまで編集を停止しています。</p>
	{#if field}<dl class="help-facts">
			<dt>機器の項目名</dt>
			<dd>{field.label}</dd>
			<dt>最大文字数</dt>
			<dd>{field.max_length ?? "機器に指定なし"}</dd>
		</dl>{/if}
{/if}
