import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import type { Call } from "./contracts";
import { History, RecordingPlayer, Transcript } from "./History";
import { phoneStore } from "./service";

test("history distinguishes archive retries from completed local capture", (): void => {
	const call: Call = {
		id: "20dd9efa-244c-47d3-8653-52bb05fab038",
		direction: "inbound",
		caller: "123",
		phase: "ended",
		started_at: 1,
		ended_at: 2,
		reason: null,
		transcript: [],
		summary: null,
		recording: { status: "available", bytes: 64 },
		processing: "complete",
		last_error: "nas_archive_unavailable",
	};
	const perform = async (operation: () => Promise<unknown>): Promise<void> => {
		await operation();
	};
	const view = render(
		<History calls={[call]} busy={false} perform={perform} />,
	);
	expect(
		screen.getByRole("status", { name: "録音の保存状況" }),
	).toHaveTextContent("録音は本体で保持しています。");
	view.rerender(
		<History
			calls={[{ ...call, last_error: null }]}
			busy={false}
			perform={perform}
		/>,
	);
	expect(
		screen.queryByRole("status", { name: "録音の保存状況" }),
	).not.toBeInTheDocument();
	expect(
		screen.getByRole("button", { name: "録音を再生" }),
	).toBeInTheDocument();
});

test("history shows a directory label above the caller number", (): void => {
	const call: Call = {
		id: "20dd9efa-244c-47d3-8653-52bb05fab038",
		direction: "inbound",
		caller: "0775511234",
		phase: "ended",
		started_at: 1,
		ended_at: 2,
		reason: null,
		transcript: [],
		summary: null,
		recording: { status: "none" },
		processing: "complete",
		last_error: null,
	};
	const perform = async (operation: () => Promise<unknown>): Promise<void> => {
		await operation();
	};
	const view = render(
		<History
			calls={[{ ...call, caller_label: "さくら病院（病院・診療所）" }]}
			busy={false}
			perform={perform}
		/>,
	);
	const number = screen.getByRole("heading", { level: 3, name: "0775511234" });
	const label = screen.getByText("公共電話帳: さくら病院（病院・診療所）");
	expect(
		label.compareDocumentPosition(number) & Node.DOCUMENT_POSITION_FOLLOWING,
	).toBeTruthy();
	view.rerender(<History calls={[call]} busy={false} perform={perform} />);
	expect(screen.queryByText(/公共電話帳/)).not.toBeInTheDocument();
	expect(
		screen.getByRole("heading", { level: 3, name: "0775511234" }),
	).toBeInTheDocument();
});

test("recording playback reloads the source after a storage outage", async (): Promise<void> => {
	const call: Call = {
		id: "20dd9efa-244c-47d3-8653-52bb05fab038",
		direction: "inbound",
		caller: "123",
		phase: "ended",
		started_at: 1,
		ended_at: 2,
		reason: null,
		transcript: [],
		summary: null,
		recording: { status: "available", bytes: 64 },
		processing: "complete",
		last_error: null,
	};
	const play = vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue();
	const load = vi
		.spyOn(HTMLMediaElement.prototype, "load")
		.mockImplementation((): void => {});
	const view = render(<RecordingPlayer call={call} />);
	const audio = view.container.querySelector("audio");
	if (!audio) throw new Error("missing recording audio");
	fireEvent.error(audio);
	await screen.findByRole("alert");
	fireEvent.click(screen.getByRole("button", { name: "録音を再生" }));
	expect(load).toHaveBeenCalledTimes(1);
	expect(play).toHaveBeenCalledTimes(1);
	expect(load.mock.invocationCallOrder[0]).toBeLessThan(
		play.mock.invocationCallOrder[0] ?? 0,
	);
	load.mockRestore();
	play.mockRestore();
});

test("loads transcript only when opened and reloads changed metadata", async (): Promise<void> => {
	const call: Call = {
		id: "20dd9efa-244c-47d3-8653-52bb05fab038",
		direction: "inbound",
		caller: "123",
		phase: "ended",
		started_at: 1,
		ended_at: 2,
		reason: null,
		transcript: [],
		summary: null,
		recording: { status: "none" },
		processing: "waiting",
		last_error: null,
	};
	const request = vi.spyOn(phoneStore, "call").mockResolvedValue({
		...call,
		transcript: [{ item_id: "a", speaker: "caller", text: "用件を伝えます。" }],
	});
	const view = render(<Transcript call={call} />);
	expect(request).not.toHaveBeenCalled();
	const details = screen.getByText("文字起こしを表示").closest("details");
	if (!details) throw new Error("missing disclosure");
	details.open = true;
	fireEvent(details, new Event("toggle"));
	await screen.findByText("用件を伝えます。");
	expect(request).toHaveBeenCalledTimes(1);
	view.rerender(<Transcript call={{ ...call, processing: "complete" }} />);
	await waitFor(() => expect(request).toHaveBeenCalledTimes(2));
	request.mockRestore();
});
