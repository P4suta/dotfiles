import type { Plugin } from "@opencode/plugin";
import { executable, runHook } from "./native-runtime";

const MAX_SESSIONS = 32;

interface State {
	prompt: string;
	directory: string;
	report?: string;
	again: boolean;
	continuation?: string;
}

function decision(response: string): Record<string, unknown> {
	const value: unknown = JSON.parse(response);
	if (typeof value !== "object" || value === null || Array.isArray(value)) {
		throw new Error("The native hook returned an invalid decision");
	}
	const record = value as Record<string, unknown>;
	if (record.decision === "block") {
		if (typeof record.reason !== "string" || !record.reason)
			throw new Error("A refusal has no reason");
	} else if (record.continue === false) {
		if (typeof record.stopReason !== "string" || !record.stopReason)
			throw new Error("A stopped turn has no reason");
	} else if (Object.keys(record).length) {
		throw new Error("The native hook returned an unsupported decision");
	}
	return record;
}

export async function setupVerification(
	ctx: Plugin.Context,
): Promise<Plugin.Cleanup> {
	const binary = executable(ctx.options);
	const states = new Map<string, State>();
	const controller = new AbortController();
	let streamFailure: unknown;
	await ctx.session.hook("context", async (event) => {
		if (streamFailure)
			throw new Error("The verification event stream is unavailable", {
				cause: streamFailure,
			});
		const session = await ctx.session.get({ sessionID: event.sessionID });
		const messages = await ctx.session.context({ sessionID: event.sessionID });
		const prompt = messages.findLast((message) => message.type === "user");
		if (!prompt)
			throw new Error("Verification requires a persisted user prompt");
		let state = states.get(event.sessionID);
		if (
			state &&
			state.prompt !== prompt.id &&
			state.continuation === prompt.text
		) {
			state.prompt = prompt.id;
			delete state.continuation;
		}
		if (!state || state.prompt !== prompt.id) {
			if (!state && states.size >= MAX_SESSIONS)
				throw new Error("Too many unfinished verification sessions");
			const directory = session.location.directory;
			state = {
				prompt: prompt.id,
				directory,
				again: false,
			};
			states.set(event.sessionID, state);
		}
		if (state.directory !== session.location.directory) {
			state.directory = session.location.directory;
			delete state.report;
		}
		state.report ??= (
			await runHook(binary, state.directory, ["assure", "remind"], {})
		).trim();
		if (state.report) event.system.push({ type: "text", text: state.report });
	});
	const watch = (async (): Promise<void> => {
		try {
			for await (const event of ctx.event.subscribe({
				signal: controller.signal,
			})) {
				if (event.type !== "session.idle") continue;
				const sessionID = event.data.sessionID;
				const state = states.get(sessionID);
				if (!state) continue;
				if (
					event.location?.directory &&
					event.location.directory !== state.directory
				)
					continue;
				const messages = await ctx.session.context({ sessionID });
				const message = messages.findLast((item) => item.type === "assistant");
				const reply =
					message?.content
						.filter((part) => part.type === "text")
						.map((part) => part.text)
						.join("\n\n") ?? "";
				const payload = {
					session_id: sessionID,
					cwd: state.directory,
					stop_hook_active: state.again,
					last_assistant_message: reply,
				};
				const verification = decision(
					await runHook(binary, state.directory, ["assure", "reply"], payload),
				);
				const outcomes = [verification];
				const end = outcomes.find((outcome) => outcome.continue === false);
				if (end) {
					console.error(
						`${String(end.systemMessage ?? "Verification remains incomplete")}\n${String(end.stopReason)}`,
					);
					continue;
				}
				const reason = outcomes
					.filter((outcome) => outcome.decision === "block")
					.map((outcome) => String(outcome.reason))
					.join("\n\n");
				if (reason) {
					state.again = true;
					state.continuation = reason;
					await ctx.session.prompt({ sessionID, text: reason });
				} else {
					states.delete(sessionID);
				}
			}
			if (!controller.signal.aborted)
				throw new Error("The verification event stream ended");
		} catch (error) {
			if (!controller.signal.aborted) {
				streamFailure = error;
				console.error("The verification event stream failed", error);
			}
		}
	})();
	return async () => {
		controller.abort();
		await watch;
	};
}
