import type { ReactElement } from "react";
import type { Call } from "./contracts";

export function CallerName({ call }: { call: Call }): ReactElement {
	return (
		<div className="caller">
			{call.caller_label && (
				<p className="caller-label">公共電話帳: {call.caller_label}</p>
			)}
			<h3>{call.caller || "非通知"}</h3>
		</div>
	);
}
