import { type FormEvent, type ReactElement, useState } from "react";
import { z } from "zod";
import type { Perform } from "./App";
import type { Settings, Status } from "./contracts";
import { phoneStore, request } from "./service";

export function Preferences({
	status,
	busy,
	perform,
}: {
	status: Status | null;
	busy: boolean;
	perform: Perform;
}): ReactElement {
	const [settings, setSettings] = useState<Settings | null>(null);
	const [number, setNumber] = useState("");
	const [rule, setRule] = useState<"allow" | "block" | "screen">("screen");
	const [saved, setSaved] = useState(false);
	const enablePush = async (): Promise<void> => {
		const key = status?.vapid_public_key;
		if (!key) throw new Error("通知の設定が完了していません。");
		if (!("serviceWorker" in navigator) || !("PushManager" in window))
			throw new Error("ブラウザーが通知に対応していません。");
		if ((await Notification.requestPermission()) !== "granted")
			throw new Error("ブラウザーの通知を許可してください。");
		await navigator.serviceWorker.register("/sw.js", { type: "module" });
		const registration = await navigator.serviceWorker.ready;
		const bytes = Uint8Array.from(
			atob(key.replaceAll("-", "+").replaceAll("_", "/")),
			(character) => character.charCodeAt(0),
		);
		const subscription = await registration.pushManager.subscribe({
			userVisibleOnly: true,
			applicationServerKey: bytes,
		});
		await request(
			"/push-subscriptions",
			z.void(),
			"POST",
			subscription.toJSON(),
		);
	};
	return (
		<section id="settings" aria-labelledby="settings-title">
			<div className="section-heading">
				<h2 id="settings-title">設定</h2>
				<span>電話の受け付けと保存</span>
			</div>
			<p>迷惑電話と迷惑電話の疑いは、AIの判定後に切断します。</p>
			{!settings ? (
				<button
					type="button"
					disabled={busy}
					onClick={(): void => {
						void perform(async () => setSettings(await phoneStore.settings()));
					}}
				>
					設定を読み込む
				</button>
			) : (
				<div className="panel">
					<form
						onSubmit={(event: FormEvent): void => {
							event.preventDefault();
							void perform(async () => {
								setSettings(await phoneStore.saveSettings(settings));
								setSaved(true);
							});
						}}
					>
						<label htmlFor="retention">履歴と録音の保存日数</label>
						<input
							id="retention"
							type="number"
							min={1}
							max={365}
							required
							value={settings.retention_days ?? 30}
							onChange={(event): void => {
								setSaved(false);
								setSettings({
									...settings,
									retention_days: Number(event.target.value),
								});
							}}
						/>
						<label htmlFor="quota">録音の保存容量（GiB）</label>
						<input
							id="quota"
							type="number"
							min={0.001}
							step={0.001}
							required
							value={
								(settings.recording_limit_bytes ?? 4294967296) / 1073741824
							}
							onChange={(event): void => {
								setSaved(false);
								setSettings({
									...settings,
									recording_limit_bytes: Math.round(
										Number(event.target.value) * 1073741824,
									),
								});
							}}
						/>
						<p className="note">
							容量を超えると終了済みの古い録音から削除します。
						</p>
						<label htmlFor="webhook">通知先Webhook URL（任意）</label>
						<input
							id="webhook"
							type="url"
							value={settings.webhook_url ?? ""}
							onChange={(event): void => {
								setSaved(false);
								setSettings({
									...settings,
									webhook_url: event.target.value || null,
								});
							}}
						/>
						<button type="submit" disabled={busy}>
							設定を保存する
						</button>
						{saved && <p>設定を保存しました。</p>}
					</form>
					<h3>電話番号ごとの応答</h3>
					<form
						onSubmit={(event: FormEvent): void => {
							event.preventDefault();
							setSaved(false);
							setSettings({
								...settings,
								rules: { ...settings.rules, [number]: rule },
							});
							setNumber("");
						}}
					>
						<label htmlFor="rule-number">電話番号（非通知はanonymous）</label>
						<input
							id="rule-number"
							value={number}
							required
							pattern="(\+?[0-9]{1,32}|anonymous)"
							onChange={(event): void => setNumber(event.target.value)}
						/>
						<label htmlFor="rule-action">応答方法</label>
						<select
							id="rule-action"
							value={rule}
							onChange={(event): void => {
								const value = event.target.value;
								if (
									value === "allow" ||
									value === "block" ||
									value === "screen"
								)
									setRule(value);
							}}
						>
							<option value="screen">AIが受け付ける</option>
							<option value="allow">電話機を呼び出す</option>
							<option value="block">着信を拒否する</option>
						</select>
						<button type="submit">番号を追加する</button>
					</form>
					{Object.entries(settings.rules).map(([number, rule]) => (
						<div className="rule" key={number}>
							<span>
								{number} ·{" "}
								{rule === "allow"
									? "直接呼び出す"
									: rule === "block"
										? "着信を拒否"
										: "AI受付"}
							</span>
							<button
								type="button"
								aria-label={`${number}の応答設定を削除する`}
								onClick={(): void => {
									const rules = { ...settings.rules };
									delete rules[number];
									setSettings({ ...settings, rules });
									setSaved(false);
								}}
							>
								番号を削除する
							</button>
						</div>
					))}
					<p className="note">
						番号の変更後は「設定を保存する」を押してください。
					</p>
				</div>
			)}
			<div className="panel">
				<h3>ブラウザー通知</h3>
				<p>
					通知を許可すると、画面を閉じた後も着信と通話履歴の更新を受け取れます。
				</p>
				<button
					type="button"
					disabled={busy || !status?.vapid_public_key}
					onClick={(): void => {
						void perform(enablePush);
					}}
				>
					ブラウザー通知を有効にする
				</button>
			</div>
		</section>
	);
}
