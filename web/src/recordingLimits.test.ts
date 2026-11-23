import { expect, test } from "vitest";
import {
	minimumRecordingGiB,
	recordingDuration,
	recordingGiB,
} from "./recordingLimits";

test("an entered limit displays again at the precision it was entered", (): void => {
	for (const entered of [0.43, 0.5, 0.501, 1, 3.999]) {
		expect(recordingGiB(Math.round(entered * 1073741824))).toBe(entered);
	}
	expect(
		recordingGiB(Math.round(minimumRecordingGiB(460816384) * 1073741824)),
	).toBeGreaterThanOrEqual(minimumRecordingGiB(460816384));
	expect(recordingGiB(4294967296)).toBe(4);
});

test("recording durations preserve seconds and show whole minutes", (): void => {
	expect(recordingDuration(1)).toBe("1秒");
	expect(recordingDuration(59)).toBe("59秒");
	expect(recordingDuration(60)).toBe("1分");
	expect(recordingDuration(61)).toBe("61秒");
	expect(recordingDuration(7200)).toBe("120分");
});

test("the input minimum rounds upward to a valid three decimal GiB value", (): void => {
	expect(minimumRecordingGiB(1)).toBe(0.001);
	expect(minimumRecordingGiB(536870912)).toBe(0.5);
	expect(minimumRecordingGiB(536870913)).toBe(0.501);
	expect(minimumRecordingGiB(460816384)).toBe(0.43);
	expect(minimumRecordingGiB(1073741824)).toBe(1);
});
