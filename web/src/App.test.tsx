import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { App } from "./App";

describe("the telephone workspace", (): void => {
	it("names the service and exposes navigation and connection status", (): void => {
		render(<App />);
		expect(
			screen.getByRole("heading", { level: 1, name: "Home Telephone" }),
		).toBeVisible();
		expect(screen.getByRole("navigation", { name: "操作画面" })).toBeVisible();
		expect(screen.getByRole("link", { name: "通話" })).toHaveAttribute(
			"href",
			"#calls",
		);
		expect(screen.getByRole("link", { name: "履歴" })).toHaveAttribute(
			"href",
			"#history",
		);
		expect(screen.getByRole("link", { name: "設定" })).toHaveAttribute(
			"href",
			"#settings",
		);
		expect(screen.getByRole("status")).toHaveTextContent("接続");
	});
});
