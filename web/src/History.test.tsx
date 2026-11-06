import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import type { Call } from "./contracts";
import { Transcript } from "./History";
import { phoneStore } from "./service";

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
