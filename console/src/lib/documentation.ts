import { z } from "zod";
import {
	type Help,
	type HelpTopic,
	helpSchema,
	helpTopicSchema,
	type ParameterReference,
	parameterReferenceSchema,
} from "./api";
import firmwareSource from "./firmware-parameters.json" with { type: "json" };
import referenceSource from "./parameters.json" with { type: "json" };
import topicSource from "./settings.json" with { type: "json" };

const sectionTitles: Record<string, string> = {
	basic: "基本設定",
	advanced: "詳細設定",
	fxs: "電話機（FXS）",
	fxo: "電話回線（FXO）",
};

export function buildCatalogue(
	references: readonly ParameterReference[],
	topics: readonly HelpTopic[],
): readonly Help[] {
	const topicsByKey = new Map(topics.map((topic) => [topic.key, topic]));
	if (
		references.length === 0 ||
		topicsByKey.size !== topics.length ||
		new Set(references.map((reference) => reference.id)).size !== references.length
	)
		throw new Error("Empty or duplicated parameter documentation");
	return references.map((parameter) => {
		const topic = topicsByKey.get(parameter.topic);
		const section = sectionTitles[parameter.section];
		if (!topic || !section) throw new Error("Missing parameter explanation");
		const condition = parameter.ranges
			.map((range) => (range.min === range.max ? String(range.min) : `${range.min}〜${range.max}`))
			.join("、");
		return helpSchema.parse({
			key: `parameter-${parameter.id.toLowerCase()}`,
			title: parameter.title,
			aliases: topic.aliases,
			parameters: [parameter.id === "8467" ? "P8467" : parameter.id],
			category: `${section} / ${topic.title}`,
			summary: topic.summary,
			input: condition ? `資料の数値範囲: ${condition}。\n${topic.input}` : topic.input,
			default_value:
				parameter.default_value ??
				"資料で工場出荷時の値を確定できません。\nテンプレートの記載値と接続中の値を区別して確認してください。",
			recommendation: topic.recommendation,
			impact: topic.impact,
			source: topic.source,
			template_value: parameter.template_value,
			reference: { kind: "template", firmware: "1.0.17.3", line: parameter.source_line },
			ranges: parameter.ranges,
			note: parameter.note,
			read_only: parameter.read_only,
		});
	});
}
export function appendFirmwareHelp(
	base: readonly Help[],
	firmware: readonly Help[],
): readonly Help[] {
	const keys = new Set(base.map((help) => help.key));
	const parameters = new Set(base.flatMap((help) => help.parameters));
	if (
		firmware.length === 0 ||
		firmware.some((help) => {
			const valid =
				help.reference.kind === "device" &&
				help.reference.firmware === "1.0.19.6" &&
				help.template_value === null &&
				help.parameters.length === 1 &&
				!keys.has(help.key) &&
				!parameters.has(help.parameters[0] ?? "");
			keys.add(help.key);
			for (const parameter of help.parameters) parameters.add(parameter);
			return !valid;
		})
	)
		throw new Error("Invalid or duplicate firmware documentation");
	return [...base, ...firmware];
}
const references = z.array(parameterReferenceSchema).min(1).parse(referenceSource);
export const catalogue: readonly Help[] = appendFirmwareHelp(
	buildCatalogue(references, z.array(helpTopicSchema).min(1).parse(topicSource)),
	z.array(helpSchema).min(1).parse(firmwareSource),
);

const rawParameterIds = new Set(
	references.map((parameter) => parameter.id).filter((id) => /^[0-9]+$/.test(id)),
);

export function searchHelp(query: string): readonly Help[] {
	const terms = query
		.normalize("NFKC")
		.toLocaleLowerCase("en-US")
		.trim()
		.split(/\s+/)
		.filter(Boolean);
	return catalogue.filter((help) => {
		const text = [
			help.title,
			help.summary,
			help.category,
			help.input,
			help.recommendation,
			help.impact,
			help.default_value,
			help.template_value ?? "",
			help.note,
			...help.aliases,
		]
			.join(" ")
			.normalize("NFKC")
			.toLocaleLowerCase("en-US");
		return terms.every((term) =>
			/^p[0-9]+$/.test(term) || rawParameterIds.has(term)
				? help.parameters.some(
						(parameter) =>
							parameter.toLowerCase() === term || parameter.toLowerCase() === `p${term}`,
					)
				: text.includes(term),
		);
	});
}
