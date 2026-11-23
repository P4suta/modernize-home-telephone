import { getContext, setContext } from "svelte";
import type { PageId } from "./api";
import type { Draft } from "./draft";
import { changedValues } from "./draft";

const key = Symbol("console-state");
export class ConsoleState {
	drafts = $state<Partial<Record<PageId, Draft>>>({});
	get dirty(): boolean {
		return Object.values(this.drafts).some((draft) => Object.keys(changedValues(draft)).length > 0);
	}
	clear(): void {
		this.drafts = {};
	}
}
export function provideConsole(): ConsoleState {
	const state = new ConsoleState();
	setContext(key, state);
	return state;
}
export function useConsole(): ConsoleState {
	return getContext<ConsoleState>(key);
}
