import { type FormEvent, type ReactElement, useState } from "react";
import type { Perform } from "./App";
import { type BrowserState, browserPhone } from "./browser-phone";
import type { Call } from "./contracts";
import { callTime, Transcript } from "./History";
import { phoneStore, type Workspace } from "./service";

const phases: Record<Call["phase"], string> = {
	offered: "着信",
	screening: "AI受付中",
	ringing: "呼び出し中",
	connected: "通話中",
	held: "保留中",
	voicemail: "伝言を録音中",
	ended: "終了",
};
export function Calls({
	workspace,
	phone,
	busy,
	perform,
}: {
	workspace: Workspace;
	phone: BrowserState;
	busy: boolean;
	perform: Perform;
}): ReactElement {
	const [number, setNumber] = useState("");
	const [spoken, setSpoken] = useState("");
	const [digits, setDigits] = useState("");
	const [scenario, setScenario] = useState("human");
	const active = workspace.calls.find((call) => call.phase !== "ended");
	const available = phone.phase === "ready";
	const command = (action: "hangup" | "hold" | "resume"): void => {
		if (active) void perform(() => phoneStore.command(active.id, { action }));
	};
	return (
		<section id="calls" aria-labelledby="calls-title">
			<div className="section-heading">
				<h2 id="calls-title">通話</h2>
				<span>{active ? phases[active.phase] : "着信待ち"}</span>
			</div>
			<div className="columns">
				<div className="panel">
					<h3>ブラウザー通話</h3>
					<p>
						{phone.phase === "talking"
							? "マイクと音声を接続しています。"
							: phone.phase === "incoming"
								? "ブラウザーに着信しています。"
								: phone.phase === "ready"
									? "ブラウザーで応答できます。"
									: phone.phase === "error"
										? "電話への接続に失敗しました。再接続してください。"
										: "有効にするとブラウザーから電話を利用できます。"}
					</p>
					{phone.phase === "offline" || phone.phase === "error" ? (
						<button
							type="button"
							disabled={busy}
							onClick={(): void => {
								void perform(async () => {
									await browserPhone.disable();
									await browserPhone.enable(await phoneStore.credentials());
								});
							}}
						>
							ブラウザー通話を有効にする
						</button>
					) : (
						<button
							type="button"
							disabled={busy}
							onClick={(): void => {
								void perform(() => browserPhone.disable());
							}}
						>
							ブラウザー通話を無効にする
						</button>
					)}
					{phone.phase === "incoming" && (
						<div className="actions">
							<button
								type="button"
								disabled={busy}
								onClick={(): void => {
									void perform(() => browserPhone.answer());
								}}
							>
								応答する
							</button>
							<button
								type="button"
								disabled={busy}
								onClick={(): void => {
									void perform(() => browserPhone.hangup());
								}}
							>
								ブラウザーの着信を辞退する
							</button>
						</div>
					)}
					{phone.phase === "talking" && (
						<button
							type="button"
							aria-pressed={phone.muted}
							onClick={browserPhone.mute}
						>
							{phone.muted
								? "マイクのミュートを解除する"
								: "マイクをミュートする"}
						</button>
					)}
					{phone.audioBlocked && (
						<button
							type="button"
							onClick={(): void => {
								void perform(() => browserPhone.playAudio());
							}}
						>
							通話音声を再生する
						</button>
					)}
					{/* biome-ignore lint/a11y/useMediaCaption: Live telephone audio has no prerecorded caption track. */}
					<audio ref={browserPhone.setAudio} autoPlay aria-label="通話音声" />
				</div>
				<div className="panel">
					<h3>電話をかける</h3>
					<form
						onSubmit={(event: FormEvent): void => {
							event.preventDefault();
							void perform(() => browserPhone.dial(number));
						}}
					>
						<label htmlFor="dial-number">電話番号</label>
						<input
							id="dial-number"
							type="tel"
							autoComplete="tel"
							inputMode="tel"
							pattern="\+?[0-9]{1,32}"
							required
							value={number}
							onChange={(event): void => setNumber(event.target.value)}
						/>
						<button disabled={busy || !available || !!active} type="submit">
							発信する
						</button>
					</form>
					<p className="note">通話を開始するとマイクの許可を求めます。</p>
				</div>
			</div>
			{active && (
				<article className="active-call">
					<div className="section-heading">
						<h3>{active.caller || "非通知"}</h3>
						<span>{phases[active.phase]}</span>
					</div>
					<p>
						{callTime(active.started_at)} ·{" "}
						{active.direction === "inbound" ? "着信" : "発信"}
					</p>
					<div className="actions">
						{(active.phase === "screening" || active.phase === "ringing") && (
							<button
								type="button"
								disabled={busy || !available}
								onClick={(): void => {
									void perform(async () => {
										await browserPhone.prepareTakeover();
										await phoneStore.command(active.id, { action: "takeover" });
									});
								}}
							>
								ブラウザーへ引き継ぐ
							</button>
						)}
						{active.phase === "connected" && (
							<button
								type="button"
								disabled={busy}
								onClick={(): void => command("hold")}
							>
								保留する
							</button>
						)}
						{active.phase === "held" && (
							<button
								type="button"
								disabled={busy}
								onClick={(): void => command("resume")}
							>
								通話を再開する
							</button>
						)}
						<button
							className="filled"
							type="button"
							disabled={busy}
							onClick={(): void => command("hangup")}
						>
							電話を切る
						</button>
					</div>
					{(active.phase === "screening" || active.phase === "connected") && (
						<form
							onSubmit={(event: FormEvent): void => {
								event.preventDefault();
								void perform(async () => {
									await phoneStore.command(active.id, {
										action: "speak",
										text: spoken,
									});
									setSpoken("");
								});
							}}
						>
							<label htmlFor="spoken-text">相手へ読み上げる文章</label>
							<textarea
								id="spoken-text"
								maxLength={500}
								required
								value={spoken}
								onChange={(event): void => setSpoken(event.target.value)}
							/>
							<button disabled={busy || !spoken.trim()} type="submit">
								文章を読み上げる
							</button>
						</form>
					)}
					{active.phase === "connected" && (
						<form
							onSubmit={(event: FormEvent): void => {
								event.preventDefault();
								void perform(() =>
									phoneStore.command(active.id, { action: "dtmf", digits }),
								);
							}}
						>
							<label htmlFor="dtmf">プッシュ信号</label>
							<input
								id="dtmf"
								type="text"
								inputMode="tel"
								maxLength={32}
								pattern="[0-9*#ABCD]+"
								required
								value={digits}
								onChange={(event): void => setDigits(event.target.value)}
							/>
							<button type="submit" disabled={busy}>
								プッシュ信号を送る
							</button>
						</form>
					)}
					<Transcript call={active} />
					{active.last_error && (
						<p>
							AI接続または録音に問題が発生しました。通話履歴で処理状態を確認できます。
						</p>
					)}
				</article>
			)}
			{workspace.status?.mode === "lab" && (
				<details className="lab">
					<summary>仮想回線で試す</summary>
					<p>外部の電話回線と有料APIを使わず、着信とAI判定を確認できます。</p>
					<form
						onSubmit={(event: FormEvent): void => {
							event.preventDefault();
							void perform(() => phoneStore.labIncoming(scenario));
						}}
					>
						<label htmlFor="scenario">着信の種類</label>
						<select
							id="scenario"
							value={scenario}
							onChange={(event): void => setScenario(event.target.value)}
						>
							<option value="human">住人への取り次ぎ</option>
							<option value="spam">迷惑電話</option>
							<option value="suspected_spam">迷惑電話の疑い</option>
							<option value="message">伝言を残す</option>
							<option value="silent">無応答</option>
						</select>
						<button
							type="submit"
							disabled={busy || !!active || !workspace.status.gateway_ready}
						>
							仮想回線から着信させる
						</button>
					</form>
				</details>
			)}
		</section>
	);
}
