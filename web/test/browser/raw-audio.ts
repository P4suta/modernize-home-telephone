import { test as base } from "@playwright/test";

export const test = base.extend<{ rawAudio: undefined }>({
	rawAudio: [
		async ({ context }, use): Promise<void> => {
			await context.addInitScript((): void => {
				const devices = navigator.mediaDevices;
				if (!devices) return;
				const capture = devices.getUserMedia.bind(devices);
				devices.getUserMedia = (
					constraints?: MediaStreamConstraints,
				): Promise<MediaStream> => {
					const audio = constraints?.audio;
					if (!audio) return capture(constraints);
					return capture({
						...constraints,
						audio: {
							...(typeof audio === "object" ? audio : {}),
							echoCancellation: { exact: false },
							noiseSuppression: { exact: false },
							autoGainControl: { exact: false },
						},
					});
				};
			});
			await use(undefined);
		},
		{ auto: true },
	],
});
