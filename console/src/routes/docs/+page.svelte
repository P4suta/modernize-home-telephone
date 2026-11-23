<script lang="ts">
	import Header from "#lib/Header.svelte";
	import HelpContent from "#lib/HelpContent.svelte";
	import Icon from "#lib/Icon.svelte";
	import { searchHelp } from "#lib/documentation.ts";
	import { page } from "$app/state";
	let search = $state("");
	let pageNumber = $state(0);
	const results = $derived(searchHelp(search));
	const visibleResults = $derived(results.slice(pageNumber * 20, (pageNumber + 1) * 20));
	$effect(() => {
		search = page.url.searchParams.get("q") ?? "";
		pageNumber = 0;
	});
	function movePage(delta: number): void {
		pageNumber += delta;
		document.getElementById("reference-title")?.focus();
	}
</script>

<svelte:head
	><title>ドキュメント · HT813</title><meta
		name="description"
		content="HT813の配線、設定項目、音声用語、保存と反映、接続エラーの対処を日本語で確認できます。"
	/></svelte:head
>
<Header active="docs" />
<main id="main" class="page-layout">
	<aside class="section-navigation">
		<p class="caption">説明</p>
		<nav aria-label="ドキュメント内">
			<a href="#start"><Icon name="book" />はじめに</a><a href="#reference"
				><Icon name="settings-label" />設定項目の説明</a
			><a href="#terms"><Icon name="audio-label" />用語集</a><a href="#save"
				><Icon name="save-action" />保存と反映</a
			><a href="#recovery"><Icon name="refresh-label" />困ったとき</a><a href="#sources"
				><Icon name="book" />参照資料</a
			>
		</nav>
	</aside>
	<div class="main-content documentation">
		<div class="page-heading">
			<div>
				<h1>ドキュメント</h1>
				<p>設定の意味を調べてから変更できます。</p>
			</div>
		</div>
		<div class="field documentation-search">
			<label for="documentation-search"><Icon name="search" />説明を検索</label><input
				id="documentation-search"
				type="search"
				bind:value={search}
				oninput={(): void => {
					pageNumber = 0;
				}}
				placeholder="RTP、PCMU、メーカーの項目名など"
				aria-controls="reference"
				autocomplete="off"
			/>
			<p class="field-hint">日本語、メーカーの項目名、P番号から探せます。</p>
		</div>
		<p class="notice">
			現在値、工場出荷時の値、テンプレートの記載値を区別しています。<br
			/>資料で初期値を確定できない場合も明示します。<br />各設定の「i」と共通の説明を表示します。
		</p>
		<noscript
			><p class="notice">
				全項目の検索とページ切り替えにはJavaScriptを有効にしてください。<br
				/>参照資料のメーカーガイドからも設定を確認できます。
			</p></noscript
		>
		{#if search.trim() === ""}
			<section id="start">
				<h2>はじめに</h2>
				<div class="reading">
					<p>FXSは電話機をつなぐ端子です。<br />FXOは電話回線を受ける端子です。</p>
					<p>
						BL3000HMの電話端子からHT813のFXOへ接続します。<br
						/>電話機はHT813のFXSへ、HT813のWANはルーターのLANへ接続します。
					</p>
					<p>
						配線が終わったら「状態」でファームウェアとポートの状態を確認します。<br
						/>SIPの登録状態と、電話機や回線の使用状態は別に確認してください。
					</p>
					<p>
						管理画面へ接続できても、通話の設定が完了したとは限りません。<br
						/>変更後は着信、発信、双方向の音声、切断を確認してください。
					</p>
				</div>
			</section>
		{/if}
		<section id="reference" aria-labelledby="reference-title">
			<h2 id="reference-title" tabindex="-1">設定項目の説明</h2>
			<p role="status">
				{results.length}件の説明 / {results.length ? pageNumber * 20 + 1 : 0}〜{Math.min(
					(pageNumber + 1) * 20,
					results.length,
				)}件目
			</p>
			{#if results.length === 0}<p>
					一致する説明がありません。<br />メーカーの項目名や短い用語で検索してください。
				</p>{/if}
			{#each visibleResults as help (help.key)}
				<article
					class="documentation-article"
					id={help.key}
					aria-labelledby={`heading-${help.key}`}
				>
					<h3 id={`heading-${help.key}`}>{help.title}</h3>
					<p class="caption">{help.category} · {help.parameters.join(" / ")}</p>
					<p class="native-label">
						{help.aliases.filter((alias) => /[A-Za-z]/.test(alias)).join(" / ")}
					</p>
					<HelpContent {help} />
				</article>
			{/each}
			<nav class="actions" aria-label="設定項目のページ">
				<button
					type="button"
					class="button"
					disabled={pageNumber === 0}
					onclick={(): void => {
						movePage(-1);
					}}><Icon name="book" />前の20件</button
				>
				<span>{pageNumber + 1} / {Math.max(1, Math.ceil(results.length / 20))}ページ</span>
				<button
					type="button"
					class="button"
					disabled={(pageNumber + 1) * 20 >= results.length}
					onclick={(): void => {
						movePage(1);
					}}><Icon name="book" />次の20件</button
				>
			</nav>
		</section>
		<section id="terms">
			<h2>用語集</h2>
			<dl class="help-facts">
				<dt>SIP</dt>
				<dd>
					通話の開始、応答、終了などを伝える仕組みです。<br
					/>登録に成功しても音声の経路が通るとは限りません。
				</dd>
				<dt>RTP / RTCP</dt>
				<dd>
					RTPは音声データを運び、RTCPは通信状態などの制御情報を伝えます。<br
					/>HT813ではRTPとRTCPに隣接するポートを使用します。
				</dd>
				<dt>PCMU / PCMA</dt>
				<dd>
					G.711の音声符号化方式です。<br />PCMUはμ-law、PCMAはA-lawです。<br
					/>接続先と共通の方式を選ぶ必要があります。
				</dd>
				<dt>NAT / STUN</dt>
				<dd>
					NATはネットワークのアドレスを変換する仕組みです。<br
					/>STUNは外側から見えるアドレスなどを確認するために使われます。<br
					/>Tailscale経由の管理画面への接続とは別の設定です。
				</dd>
				<dt>DHCP / DNS / NTP</dt>
				<dd>
					DHCPはネットワーク設定の自動割り当て、DNSは名前とアドレスの解決、NTPは時刻の同期を担当します。
				</dd>
				<dt>FXS / FXO / PSTN</dt>
				<dd>
					FXSは電話機側、FXOは電話回線側の端子です。<br
					/>PSTNは公衆電話網を表す用語で、設定項目では電話回線側の動作を指す場合があります。
				</dd>
			</dl>
		</section>
		<section id="save">
			<h2>保存と反映</h2>
			<div class="reading">
				<p>
					「保存」は本家のUpdateに対応します。<br />「保存して反映」はApplyに対応します。<br
					/>再起動が必要な項目は別に確認してください。
				</p>
				<p>
					変更内容を確認してから送信します。<br />保存の直前に機器の値と認証情報を再取得します。<br
					/>機器の値が変わっていた場合は、編集内容を残して比較を求めます。
				</p>
				<p>
					通信が途切れた保存を自動で繰り返しません。<br />保存後の値を読み戻して確認します。<br
					/>パスワードは読み戻せないため、認証や通話での確認が必要です。
				</p>
				<p>
					編集内容は画面を移動しても同じタブ内で保持します。<br
					/>ログアウトやタブを閉じる操作では編集内容が失われます。<br
					/>パスワードをブラウザーの保存領域へ書き込みません。
				</p>
			</div>
		</section>
		<section id="recovery">
			<h2>困ったとき</h2>
			<details>
				<summary>画面の取得に時間がかかる</summary>
				<p class="reading">
					本家の機器が応答するまで取得済みの値を表示します。<br
					/>画面の切り替えでは取得済みのデータを再利用します。<br
					/>再取得を連打せず、通信の完了を待ってください。
				</p>
			</details>
			<details>
				<summary>本家のログインが期限切れになった</summary>
				<p class="reading">
					管理アプリのサーバーが読み取り用の認証を回復します。<br />編集値は画面に残します。<br
					/>認証情報が変わった場合やサーバーを再起動した場合は再ログインしてください。<br
					/>管理アプリは24時間操作がないセッションを終了します。
				</p>
			</details>
			<details>
				<summary>通話の片側だけ音が聞こえない</summary>
				<p class="reading">
					SIPの登録だけでは音声経路を確認できません。<br
					/>RTPの送受信先、ポート、NAT、接続先の音声設定を確認してください。<br
					/>原因を確認してから設定を変更してください。
				</p>
			</details>
			<details>
				<summary>相手が切っても回線が使用中になる</summary>
				<p class="reading">
					回線の電流断、切断音、極性反転の通知方式を確認します。<br
					/>回線の仕様や観測結果に合わせて切断検出を設定します。<br
					/>日本の回線へ米国向けの値を無条件に適用しないでください。
				</p>
			</details>
			<details>
				<summary>管理画面へ接続できなくなった</summary>
				<p class="reading">
					機器の電源、WAN側の配線、IPアドレスを確認します。<br
					/>通話を復旧する必要がある場合は、電話機をBL3000HMの電話端子へ戻して物理経路を復旧します。<br
					/>バックアップと認証情報を確認してから設定を復旧してください。
				</p>
			</details>
		</section>
		<section id="sources">
			<h2>参照資料と適用範囲</h2>
			<div class="reading">
				<p>
					設定項目ごとにメーカー資料へのリンクを表示します。<br
					/>接続中の機器が提示する値、メーカー初期値、運用上の推奨を区別します。
				</p>
				<p>
					設定テンプレートのヘッダーは1.0.17.3、配布アーカイブのメンバー名は1.0.19.6です。<br
					/>資料の相違を機器の値より優先して自動適用しません。
				</p>
				<p>
					<a
						href="https://documentation.grandstream.com/knowledge-base/ht813-administration-guide/"
						target="_blank"
						rel="noreferrer">HT813 Administration Guide</a
					>
				</p>
				<p>
					<a
						href="https://documentation.grandstream.com/knowledge-base/ht813-quick-installation-guide/"
						target="_blank"
						rel="noreferrer">HT813 Quick Installation Guide</a
					>
				</p>
				<p>
					<a
						href="https://firmware.grandstream.com/Release_Note_HT813_1.0.19.6.pdf"
						target="_blank"
						rel="noreferrer">Firmware 1.0.19.6 Release Notes</a
					>
				</p>
			</div>
		</section>
	</div>
</main>
