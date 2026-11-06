import {
	type ReactElement,
	useCallback,
	useEffect,
	useRef,
	useState,
} from "react";
import type { Perform } from "./App";
import type { Call } from "./contracts";
import { phoneStore } from "./service";

export function callTime(seconds: number): string {
	return new Intl.DateTimeFormat("ja-JP", {
		dateStyle: "short",
		timeStyle: "short",
	}).format(new Date(seconds * 1000));
}
export function Transcript({ call }: { call: Call }): ReactElement {
	const [open, setOpen] = useState(false);
	const [lines, setLines] = useState(call.transcript);
	const [error, setError] = useState(false);
	const [loading, setLoading] = useState(false);
	const generation = useRef(0);
	const load = useCallback(async (): Promise<void> => {
		const current = ++generation.current;
		setLoading(true);
		setError(false);
		try {
			const detail = await phoneStore.call(call.id);
			if (current === generation.current) setLines(detail.transcript);
		} catch {
			if (current === generation.current) setError(true);
		} finally {
			if (current === generation.current) setLoading(false);
		}
	}, [call]);
	useEffect((): (() => void) => {
		if (open) void load();
		return (): void => {
			generation.current += 1;
		};
	}, [load, open]);
	return (
		<details onToggle={(event): void => setOpen(event.currentTarget.open)}>
			<summary>文字起こしを表示</summary>
			{loading && <p role="status">文字起こしを読み込んでいます。</p>}
			{error && (
				<div>
					<p role="alert">文字起こしを読み込めませんでした。</p>
					<button
						type="button"
						onClick={(): void => {
							void load();
						}}
					>
						再読み込み
					</button>
				</div>
			)}
			{!loading && !error && lines.length === 0 && (
				<p>文字起こしはまだありません。</p>
			)}
			{!error &&
				lines.map((line) => (
					<p key={line.item_id}>
						<strong>
							{line.speaker === "caller"
								? "発信者"
								: line.speaker === "assistant"
									? "AI受付"
									: "録音"}
						</strong>{" "}
						{line.text}
					</p>
				))}
		</details>
	);
}

export function RecordingPlayer({ call }: { call: Call }): ReactElement {
	const audio = useRef<HTMLAudioElement>(null);
	const [playing, setPlaying] = useState(false);
	const [position, setPosition] = useState(0);
	const [duration, setDuration] = useState(0);
	const [error, setError] = useState(false);
	const clock = (seconds: number): string =>
		`${Math.floor(seconds / 60)}:${String(Math.floor(seconds % 60)).padStart(2, "0")}`;
	return (
		<div className="recording-player">
			{/* biome-ignore lint/a11y/useMediaCaption: The transcript disclosure provides the audio-only text alternative. */}
			<audio
				ref={audio}
				preload="none"
				src={`/api/calls/${call.id}/recording`}
				onPlay={(): void => setPlaying(true)}
				onPause={(): void => setPlaying(false)}
				onEnded={(): void => setPlaying(false)}
				onTimeUpdate={(): void => setPosition(audio.current?.currentTime ?? 0)}
				onLoadedMetadata={(): void => {
					const value = audio.current?.duration ?? 0;
					setDuration(Number.isFinite(value) ? value : 0);
				}}
				onError={(): void => setError(true)}
			/>
			<button
				type="button"
				onClick={(): void => {
					const player = audio.current;
					if (!player) return;
					if (playing) player.pause();
					else {
						setError(false);
						void player.play().catch((): void => setError(true));
					}
				}}
			>
				{playing ? "録音を一時停止" : "録音を再生"}
			</button>
			<label>
				再生位置
				<input
					type="range"
					min="0"
					max={duration || 1}
					step="0.1"
					value={position}
					disabled={duration === 0}
					onChange={(event): void => {
						const player = audio.current;
						if (player) {
							player.currentTime = event.currentTarget.valueAsNumber;
							setPosition(player.currentTime);
						}
					}}
				/>
			</label>
			<output aria-label="録音の再生時間">
				{clock(position)} / {clock(duration)}
			</output>
			{error && <p role="alert">録音を再生できませんでした。</p>}
		</div>
	);
}

export function History({
	calls,
	busy,
	perform,
}: {
	calls: readonly Call[];
	busy: boolean;
	perform: Perform;
}): ReactElement {
	const [deleting, setDeleting] = useState<string | null>(null);
	return (
		<section id="history" aria-labelledby="history-title">
			<div className="section-heading">
				<h2 id="history-title">履歴</h2>
				<span>{calls.filter((call) => call.phase === "ended").length}件</span>
			</div>
			{calls.every((call) => call.phase !== "ended") && (
				<p>通話履歴はまだありません。</p>
			)}
			{calls
				.filter((call) => call.phase === "ended")
				.map((call) => (
					<article className="history-call" key={call.id}>
						<div className="section-heading">
							<h3>{call.caller || "非通知"}</h3>
							<time dateTime={new Date(call.started_at * 1000).toISOString()}>
								{callTime(call.started_at)}
							</time>
						</div>
						<p>
							{call.direction === "inbound" ? "着信" : "発信"} ·{" "}
							{call.reason === "ai_spam"
								? "AIが迷惑電話を判定"
								: call.reason === "service_restart"
									? "サービス再起動で終了"
									: "通話終了"}{" "}
							·{" "}
							{call.processing === "complete"
								? "処理完了"
								: call.processing === "failed"
									? "音声処理に失敗"
									: call.processing === "retrying"
										? "音声処理を再試行中"
										: "音声処理待ち"}
						</p>
						{call.summary && <p>{call.summary}</p>}
						<Transcript call={call} />
						{call.recording.status === "available" ? (
							<RecordingPlayer call={call} />
						) : (
							<p className="note">
								{call.recording.status === "deleted"
									? "録音を削除しました。"
									: call.recording.status === "failed"
										? "録音を保存できませんでした。"
										: call.recording.status === "pending"
											? "録音を保存しています。"
											: "録音はありません。"}
							</p>
						)}
						{deleting === call.id ? (
							<div className="actions">
								<p>録音・文字起こし・要約を削除します。</p>
								<button
									disabled={busy}
									type="button"
									onClick={(): void => {
										void perform(async () => {
											await phoneStore.deleteCall(call.id);
											setDeleting(null);
										});
									}}
								>
									履歴の削除を実行する
								</button>
								<button type="button" onClick={(): void => setDeleting(null)}>
									削除を取り消す
								</button>
							</div>
						) : (
							<button type="button" onClick={(): void => setDeleting(call.id)}>
								履歴を削除する
							</button>
						)}
					</article>
				))}
		</section>
	);
}
