import { z } from "zod";

export const pushPayload = z.object({
	id: z.number().int().nonnegative(),
	call_id: z.uuid().nullable(),
	title: z.string().min(1).max(100),
	body: z.string().min(1).max(500),
});
