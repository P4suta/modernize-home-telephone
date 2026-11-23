import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import type { Status } from "./contracts";
import { Preferences } from "./Preferences";
import { phoneStore } from "./service";

test("NAS retention stays manual and the quota describes temporary local storage", async (): Promise<void> => {
	const status: Status = {
		instance_id: "20dd9efa-244c-47d3-8653-52bb05fab038",
		mode: "production",
		gateway_ready: true,
		ai_configured: false,
		vapid_public_key: null,
		recording: {
			enabled: true,
			destination: "nas",
			max_seconds: 7200,
			minimum_bytes: 460816384,
		},
	};
	const load = vi.spyOn(phoneStore, "settings").mockResolvedValue({
		rules: {},
		retention_days: 0,
		recording_limit_bytes: 536870912,
		webhook_url: null,
	});
	render(
		<Preferences
			status={status}
			busy={false}
			perform={async (operation): Promise<void> => {
				await operation();
			}}
		/>,
	);
	fireEvent.click(screen.getByRole("button", { name: "設定を読み込む" }));
	await screen.findByText("手動で削除するまで保存");
	expect(
		screen.queryByLabelText("履歴と録音の保存日数"),
	).not.toBeInTheDocument();
	expect(screen.getByLabelText("本体の一時保存上限（GiB）")).toHaveValue(0.5);
	expect(
		screen.getByText("新たな録音を見合わせます。通話は続けられます。"),
	).toBeInTheDocument();
	expect(
		screen.queryByText("容量を超えると終了済みの古い録音から削除します。"),
	).not.toBeInTheDocument();
	load.mockRestore();
});
