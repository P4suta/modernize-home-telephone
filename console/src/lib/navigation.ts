import type { PageId } from "./api";
import type { IconName } from "./icons";
export type Section = {
	id: PageId | "docs";
	title: string;
	native: string;
	icon: IconName;
	path: "/status" | "/basic" | "/advanced" | "/fxs" | "/fxo" | "/docs";
};
export const sections: readonly Section[] = [
	{
		id: "status",
		title: "状態",
		native: "Status",
		icon: "activity-navigation",
		path: "/status",
	},
	{
		id: "basic",
		title: "基本設定",
		native: "Basic Settings",
		icon: "settings-navigation",
		path: "/basic",
	},
	{
		id: "advanced",
		title: "詳細設定",
		native: "Advanced Settings",
		icon: "advanced-navigation",
		path: "/advanced",
	},
	{
		id: "fxs",
		title: "電話機（FXS）",
		native: "FXS Port",
		icon: "phone-navigation",
		path: "/fxs",
	},
	{
		id: "fxo",
		title: "電話回線（FXO）",
		native: "FXO Port",
		icon: "line-navigation",
		path: "/fxo",
	},
	{
		id: "docs",
		title: "ドキュメント",
		native: "Documentation",
		icon: "book",
		path: "/docs",
	},
];
