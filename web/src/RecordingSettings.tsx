import type { ReactElement } from "react";
import type { Settings, Status } from "./contracts";
import {
	minimumRecordingGiB,
	recordingDuration,
	recordingGiB,
} from "./recordingLimits";

function StorageIcon(): ReactElement {
	return (
		<svg
			aria-hidden="true"
			width="24"
			height="24"
			viewBox="0 0 24 24"
			fill="none"
			stroke="currentColor"
			strokeWidth="2"
		>
			<rect x="3" y="3" width="18" height="18" rx="2" />
			<path d="M3 12h18M7 7h2M7 17h2" />
		</svg>
	);
}

export function RecordingSettings({
	settings,
	recording,
	onChange,
}: {
	settings: Settings;
	recording: Status["recording"] | undefined;
	onChange: (settings: Settings) => void;
}): ReactElement {
	if (!recording) return <p>録音設定を取得しています。</p>;
	const nas = recording.destination === "nas";
	const minimum = minimumRecordingGiB(recording.minimum_bytes);
	const duration = recordingDuration(recording.max_seconds);
	return (
		<fieldset className="recording-settings">
			<legend>録音と保存</legend>
			<p>
				{recording.enabled
					? nas
						? "応答した着信と発信の通話を録音し、NASに保存します。"
						: "応答した通話を本体に録音します。"
					: "通話の録音は無効です。"}
			</p>
			<dl>
				<dt>保存先</dt>
				<dd>{nas ? "NAS" : "本体"}</dd>
				{nas && (
					<>
						<dt>保存期間</dt>
						<dd>手動で削除するまで保存</dd>
					</>
				)}
				<dt>1通話の録音上限</dt>
				<dd>{duration}</dd>
			</dl>
			{nas ? (
				<p>録音と履歴は、履歴画面から削除できます。</p>
			) : (
				<>
					<label htmlFor="retention">
						<StorageIcon />
						履歴と録音の保存日数
					</label>
					<input
						id="retention"
						type="number"
						min={0}
						max={365}
						required
						aria-describedby="retention-help"
						value={settings.retention_days ?? 30}
						onChange={(event): void =>
							onChange({
								...settings,
								retention_days: Number(event.target.value),
							})
						}
					/>
					<p id="retention-help">
						0を指定すると、手動で削除するまで保存します。
					</p>
				</>
			)}
			<div className="recording-field-heading">
				<label htmlFor="quota">
					<StorageIcon />
					{nas ? "本体の一時保存上限（GiB）" : "録音の保存容量（GiB）"}
				</label>
				<button
					type="button"
					className="recording-help-button"
					popoverTarget="recording-storage-help"
					aria-label="一時保存上限の説明"
				>
					<svg
						aria-hidden="true"
						width="24"
						height="24"
						viewBox="0 0 24 24"
						fill="none"
						stroke="currentColor"
						strokeWidth="2"
					>
						<circle cx="12" cy="12" r="9" />
						<path d="M12 11v6M12 7v1" />
					</svg>
				</button>
			</div>
			<input
				id="quota"
				type="number"
				min={minimum}
				step={0.001}
				required
				aria-describedby="quota-help"
				value={recordingGiB(settings.recording_limit_bytes ?? 4294967296)}
				onChange={(event): void =>
					onChange({
						...settings,
						recording_limit_bytes: Math.round(
							Number(event.target.value) * 1073741824,
						),
					})
				}
			/>
			<p id="quota-help">
				{nas
					? "NASの空き容量の範囲で保存します。"
					: "容量を超えると終了済みの古い録音から削除します。"}
			</p>
			{nas && (
				<>
					<p>NASに保存できない録音は、本体で一時保存します。</p>
					<div className="panel">
						<h3>一時領域が埋まった場合</h3>
						<p>新たな録音を見合わせます。通話は続けられます。</p>
					</div>
					<p>NASに接続できない間の録音削除は、接続復旧後に実行します。</p>
				</>
			)}
			<p>{duration}で録音を終了しても、通話は継続します。</p>
			<section
				id="recording-storage-help"
				className="recording-help"
				popover="auto"
				aria-labelledby="recording-storage-help-title"
			>
				<h3 id="recording-storage-help-title">一時保存上限</h3>
				<p>録音の保存、転送後の照合、再生に必要な空き容量を確保します。</p>
				<p>上限は{minimum} GiB以上に設定してください。</p>
				<button
					type="button"
					popoverTarget="recording-storage-help"
					popoverTargetAction="hide"
				>
					× 閉じる
				</button>
			</section>
		</fieldset>
	);
}
