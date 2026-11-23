export function recordingDuration(seconds: number): string {
	return seconds % 60 === 0 ? `${seconds / 60}分` : `${seconds}秒`;
}

export function minimumRecordingGiB(bytes: number): number {
	return Math.ceil((bytes / 1073741824) * 1000) / 1000;
}
