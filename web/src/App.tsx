import {
	type ReactElement,
	useEffect,
	useState,
	useSyncExternalStore,
} from "react";
import { browserPhone } from "./browser-phone";
import { Calls } from "./Calls";
import { History } from "./History";
import { Preferences } from "./Preferences";
import { phoneStore } from "./service";

export type Perform = (operation: () => Promise<unknown>) => Promise<void>;

export function App(): ReactElement {
	const workspace = useSyncExternalStore(
		phoneStore.subscribe,
		phoneStore.getSnapshot,
	);
	const phone = useSyncExternalStore(
		browserPhone.subscribe,
		browserPhone.getSnapshot,
	);
	const [busy, setBusy] = useState(false);
	const [error, setError] = useState<string | null>(null);
	useEffect((): (() => void) => {
		phoneStore.start();
		return phoneStore.stop;
	}, []);
	const perform: Perform = async (operation): Promise<void> => {
		setBusy(true);
		setError(null);
		try {
			await operation();
		} catch (error: unknown) {
			setError(
				error instanceof Error ? error.message : "操作を完了できませんでした。",
			);
		} finally {
			setBusy(false);
		}
	};
	return (
		<>
			<a className="skip" href="#calls">
				通話操作へ移動
			</a>
			<header>
				<div>
					<h1>Home Telephone</h1>
				</div>
				<nav aria-label="操作画面">
					<a href="#calls">通話</a>
					<a href="#history">履歴</a>
					<a href="#settings">設定</a>
				</nav>
			</header>
			<main>
				<div className="connection" role="status" aria-label="接続状態">
					<span>{workspace.connected ? "接続済み" : "接続待ち"}</span>
					<span>
						通話サーバー：
						{workspace.status?.gateway_ready ? "接続済み" : "接続待ち"}
					</span>
					<span>
						AI設定：{workspace.status?.ai_configured ? "設定済み" : "未設定"}
					</span>
					<span>
						{workspace.status?.mode === "lab"
							? "仮想回線・模擬AI"
							: workspace.status
								? "自宅電話"
								: "接続確認中"}
					</span>
				</div>
				{(error || workspace.error) && (
					<p role="alert" className="error">
						{error ?? workspace.error}
					</p>
				)}
				<Calls
					workspace={workspace}
					phone={phone}
					busy={busy}
					perform={perform}
				/>
				<History calls={workspace.calls} busy={busy} perform={perform} />
				<Preferences status={workspace.status} busy={busy} perform={perform} />
			</main>
		</>
	);
}
