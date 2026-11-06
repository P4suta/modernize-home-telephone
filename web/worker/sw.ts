import { pushPayload } from "../src/push-payload";

declare const self: ServiceWorkerGlobalScope;
self.addEventListener("install", (event: ExtendableEvent): void => {
	event.waitUntil(self.skipWaiting());
});
self.addEventListener("activate", (event: ExtendableEvent): void => {
	event.waitUntil(self.clients.claim());
});
self.addEventListener("push", (event: PushEvent): void => {
	event.waitUntil(
		(async (): Promise<void> => {
			let body: unknown;
			try {
				body = event.data?.json();
			} catch {
				return;
			}
			const parsed = pushPayload.safeParse(body);
			if (!parsed.success) return;
			const payload = parsed.data;
			await self.registration.showNotification(payload.title, {
				body: payload.body,
				tag: `homephone-${payload.id}`,
				data: { call_id: payload.call_id },
			});
		})(),
	);
});
self.addEventListener("notificationclick", (event: NotificationEvent): void => {
	event.notification.close();
	event.waitUntil(
		(async (): Promise<void> => {
			const windows = await self.clients.matchAll({
				type: "window",
				includeUncontrolled: true,
			});
			const existing = windows.find(
				(client) => new URL(client.url).origin === self.location.origin,
			);
			if (existing) {
				await existing.focus();
				await existing.navigate("/#history");
			} else await self.clients.openWindow("/#history");
		})(),
	);
});
