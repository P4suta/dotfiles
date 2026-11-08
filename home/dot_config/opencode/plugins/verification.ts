import { homedir } from "node:os";
import { isAbsolute, join } from "node:path";
import type { Plugin as NativePlugin } from "@opencode/plugin";
import type { Hooks, Plugin } from "@opencode-ai/plugin";
import { setupVerification } from "../adapters/native-verification";

type SystemTransform = NonNullable<Hooks["experimental.chat.system.transform"]>;
type JsonObject = { [key: string]: unknown };

/** How long a verification hook may run before the plugin stops waiting on it. */
const HOOK_TIMEOUT_MS = 60000;
/** How many sessions the plugin remembers before it forgets the oldest. */
const MAX_SESSIONS = 32;

function object(value: unknown): JsonObject | undefined {
	return typeof value === "object" && value !== null && !Array.isArray(value)
		? (value as JsonObject)
		: undefined;
}

function text(value: unknown): string | undefined {
	return typeof value === "string" && value.length > 0 ? value : undefined;
}

/**
 * Runs the verification session the other clients run from their hooks.
 * A session receives the verification reminder and checks reply prose and changed ADRs without submitting evidence.
 * A prose refusal returns once before a second refusal ends the turn with a warning.
 */
export const Verification: Plugin = async (
	input: Parameters<Plugin>[0],
	options: Parameters<Plugin>[1] = {},
): Promise<Hooks> => {
	const executable =
		options.executable ??
		join(
			homedir(),
			".local",
			"bin",
			`dotfiles${process.platform === "win32" ? ".exe" : ""}`,
		);
	if (typeof executable !== "string" || !isAbsolute(executable)) {
		throw new Error("The dotfiles executable must be an absolute path");
	}
	const reports = new Map<string, string>();
	const continued = new Set<string>();
	const answerParts = new Map<
		string,
		{ message: string; parts: Map<string, string> }
	>();
	const continuations = new Map<string, string>();
	const remember = (session: string, report: string): void => {
		if (reports.size >= MAX_SESSIONS) {
			const oldest = reports.keys().next().value;
			if (oldest !== undefined) {
				reports.delete(oldest);
				continued.delete(oldest);
				answerParts.delete(oldest);
				continuations.delete(oldest);
			}
		}
		reports.set(session, report);
	};
	const runtime = async (
		point: "remind" | "reply",
		stdin: string,
	): Promise<string> => {
		const child = Bun.spawn([executable, "assure", point], {
			cwd: input.directory,
			stdin: new Blob([stdin]),
			stdout: "pipe",
			stderr: "pipe",
			timeout: HOOK_TIMEOUT_MS,
		});
		const [code, output, diagnostics] = await Promise.all([
			child.exited,
			new Response(child.stdout).text(),
			new Response(child.stderr).text(),
		]);
		if (code !== 0) {
			throw new Error(
				`The assure ${point} hook failed (${code}): ${diagnostics}`,
			);
		}
		return output;
	};
	return {
		"chat.message": async (
			call: Parameters<NonNullable<Hooks["chat.message"]>>[0],
			output: Parameters<NonNullable<Hooks["chat.message"]>>[1],
		): Promise<void> => {
			const prompt = output.parts
				.filter((part) => part.type === "text")
				.map((part) => part.text)
				.join("\n");
			if (continuations.get(call.sessionID) === prompt) return;
			answerParts.delete(call.sessionID);
			continuations.delete(call.sessionID);
			continued.delete(call.sessionID);
		},
		"experimental.text.complete": async (
			call: Parameters<NonNullable<Hooks["experimental.text.complete"]>>[0],
			output: Parameters<NonNullable<Hooks["experimental.text.complete"]>>[1],
		): Promise<void> => {
			let current = answerParts.get(call.sessionID);
			if (!current || current.message !== call.messageID) {
				current = { message: call.messageID, parts: new Map<string, string>() };
				answerParts.set(call.sessionID, current);
			}
			current.parts.set(call.partID, output.text);
		},
		"experimental.chat.system.transform": async (
			call: Parameters<SystemTransform>[0],
			output: Parameters<SystemTransform>[1],
		): Promise<void> => {
			const session = call.sessionID;
			if (!session) return;
			let report = reports.get(session);
			if (report === undefined) {
				report = (await runtime("remind", "")).trim();
				remember(session, report);
			}
			if (report) output.system.push(report);
		},
		event: async ({
			event,
		}: Parameters<NonNullable<Hooks["event"]>>[0]): Promise<void> => {
			const raw: unknown = event;
			const bus = object(raw);
			if (bus?.type !== "session.idle") return;
			const session = text(object(bus.properties)?.sessionID);
			if (!session) return;
			const again = continued.has(session);
			const reply = [...(answerParts.get(session)?.parts.values() ?? [])].join(
				"\n\n",
			);
			const payload = JSON.stringify({
				session_id: session,
				stop_hook_active: again,
				last_assistant_message: reply,
			});
			const answer = object(JSON.parse(await runtime("reply", payload))) ?? {};
			const reason = text(answer.reason);
			if (answer.decision === "block" && reason) {
				continued.add(session);
				continuations.set(session, reason);
				await input.client.session.prompt({
					path: { id: session },
					body: { parts: [{ type: "text", text: reason }] },
				});
				return;
			}
			continued.delete(session);
			continuations.delete(session);
			const stopped = text(answer?.stopReason);
			if (answer?.continue === false && stopped) {
				console.error(
					`${text(answer.systemMessage) ?? "Verification gaps remain"}\n${stopped}`,
				);
			}
		},
	};
};

const native: NativePlugin.Plugin = {
	id: "dotfiles.verification",
	setup: setupVerification,
};

export default { ...native, server: Verification };
