import assert from "node:assert/strict";
import { existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join, resolve } from "node:path";
import type { Plugin } from "@opencode/plugin";
import Lint from "../../../../home/dot_config/opencode/plugins/lint";
import ShellGuard from "../../../../home/dot_config/opencode/plugins/shell-guard";
import Verification from "../../../../home/dot_config/opencode/plugins/verification";

const root = Bun.argv[2];
const binary = Bun.argv[3];
assert(root && binary);
const other = resolve(root, "..", "other");
mkdirSync(join(root, "src"), { recursive: true });
mkdirSync(join(root, "tests"));
mkdirSync(other);
writeFileSync(join(root, "src/lib.rs"), "pub fn allowed() -> bool { false }\n");

type Callback = (event: unknown) => void | Promise<void>;
type Message = Awaited<
	ReturnType<Plugin.Context["session"]["context"]>
>[number];
type NativeEvent =
	ReturnType<Plugin.Context["event"]["subscribe"]> extends AsyncIterable<
		infer Event
	>
		? Event
		: never;
const sessionHooks = new Map<string, Callback>();
const toolHooks = new Map<string, Callback>();
const shellHooks = new Map<string, Callback>();
const prompts: { sessionID: string; text: string }[] = [];
const user = (id: string, text: string): Message => ({
	type: "user",
	id,
	text,
	time: { created: 0 },
});
const sessions = new Map([
	[
		"ses_native",
		{
			directory: root,
			messages: [user("user_first", "Implement the requested behavior.")],
		},
	],
	[
		"ses_other",
		{ directory: other, messages: [user("user_other", "Check the claim.")] },
	],
]);

const EVENT_LIMIT_MS = 10000;
const POLL_MS = 10;
async function until(condition: () => boolean): Promise<void> {
	const deadline = Date.now() + EVENT_LIMIT_MS;
	while (!condition()) {
		assert(Date.now() < deadline, "The native event handler did not finish");
		await Bun.sleep(POLL_MS);
	}
}

class Events {
	items: NativeEvent[] = [];
	processed = 0;
	ending = false;
	wake: (() => void) | undefined;
	async *subscribe({
		signal,
	}: {
		signal: AbortSignal;
	}): AsyncIterable<NativeEvent> {
		const wake = (): void => this.wake?.();
		signal.addEventListener("abort", wake, { once: true });
		try {
			while (!signal.aborted && !this.ending) {
				const event = this.items.shift();
				if (event) {
					yield event;
					this.processed++;
				} else {
					await new Promise<void>((resolve) => {
						this.wake = resolve;
					});
				}
			}
		} finally {
			signal.removeEventListener("abort", wake);
		}
	}
	async idle(sessionID: string, directory: string): Promise<void> {
		const expected = this.processed + 1;
		this.items.push({
			id: "event_fixture",
			created: 0,
			type: "session.idle",
			location: { directory },
			data: { sessionID },
		});
		this.wake?.();
		await until(() => this.processed >= expected);
	}
	end(): void {
		this.ending = true;
		this.wake?.();
	}
}
const events = new Events();
const register =
	(hooks: Map<string, Callback>) =>
	async (
		name: string,
		callback: Callback,
	): Promise<{ dispose: () => void }> => {
		assert(!hooks.has(name), "Each native hook registers once");
		hooks.set(name, callback);
		return {
			dispose: (): void => {
				hooks.delete(name);
			},
		};
	};
const session = (
	sessionID: string,
): { directory: string; messages: Message[] } => {
	const value = sessions.get(sessionID);
	assert(value, "The fixture received a foreign session");
	return value;
};
const context = {
	options: { executable: binary },
	location: { directory: resolve(root, "..", "unrelated-location") },
	tool: { hook: register(toolHooks) },
	shell: { hook: register(shellHooks) },
	session: {
		get: async ({
			sessionID,
		}: {
			sessionID: string;
		}): Promise<{ location: { directory: string } }> => ({
			location: { directory: session(sessionID).directory },
		}),
		context: async ({ sessionID }: { sessionID: string }): Promise<Message[]> =>
			session(sessionID).messages,
		prompt: async (input: {
			sessionID: string;
			text: string;
		}): Promise<void> => {
			prompts.push(input);
			session(input.sessionID).messages.push(
				user("user_correction", input.text),
			);
		},
		hook: register(sessionHooks),
	},
	event: {
		subscribe: (options: { signal: AbortSignal }): AsyncIterable<NativeEvent> =>
			events.subscribe(options),
	},
} as unknown as Plugin.Context;
for (const plugin of [ShellGuard, Lint, Verification]) {
	assert(plugin.id.startsWith("dotfiles."));
	assert.equal(typeof plugin.server, "function");
}
await ShellGuard.setup(context);
await Lint.setup(context);
const cleanup = await Verification.setup(context);
assert.equal(typeof cleanup, "function");

async function hook(
	hooks: Map<string, Callback>,
	name: string,
	event: unknown,
): Promise<void> {
	const callback = hooks.get(name);
	assert(callback, "The supported native hook is missing");
	await callback(event);
}
const callContext = (sessionID: string): Promise<void> =>
	hook(sessionHooks, "context", { sessionID, system: [] });
const tool = (name: string, input: unknown): object => ({
	tool: name,
	sessionID: "ses_native",
	input,
});
await callContext("ses_native");
await callContext("ses_native");
assert(!existsSync(join(homedir(), ".local/state/dotfiles/answers")));
await hook(toolHooks, "execute.before", {
	tool: "read",
	sessionID: "foreign",
	input: {},
});
await hook(
	toolHooks,
	"execute.before",
	tool("write", { filePath: "src/lib.rs", content: "changed" }),
);
await hook(shellHooks, "create.before", {
	cwd: root,
	command: "git status --short",
});
await assert.rejects(() =>
	hook(shellHooks, "create.before", {
		cwd: root,
		command: "printf blocked > created.txt",
	}),
);

await hook(
	toolHooks,
	"execute.before",
	tool("write", { filePath: "tests/contract.rs", content: "test" }),
);
writeFileSync(
	join(root, "tests/contract.rs"),
	"#[test]\nfn contract() { assert!(true); }\n",
);

const adr = join(root, "docs/adr/require-concise-records.md");
mkdirSync(join(root, "docs/adr"), { recursive: true });
writeFileSync(
	adr,
	`---\ndate: 2026-11-06\nstatus: proposed\n---\n# Require concise records\n\n## Context\n\n${"word ".repeat(210)}\n\n## Decision\n\nRequire checks.\n\n## Alternatives\n\nKeep reminders.\n\n## Consequences\n\nReject incomplete work.\n\n## Confirmation\n\nRun the checks.\n`,
);
await assert.rejects(
	() =>
		hook(toolHooks, "execute.after", {
			...tool("write", { filePath: adr }),
			status: "completed",
			result: {},
		}),
	/200|words|ADR/,
);
rmSync(adr);

await events.idle("ses_native", other);
assert.equal(prompts.length, 0, "Another location cannot trigger a correction");
await events.idle("ses_native", root);
assert.equal(
	prompts.length,
	0,
	"Ordinary completion needs no evidence receipt",
);
await callContext("ses_native");
await events.idle("ses_native", root);
await events.idle("ses_native", root);
assert.equal(
	prompts.length,
	0,
	"Repeated idle events need no evidence submission",
);

await callContext("ses_other");
const reply = "確認済みです。";
session("ses_other").messages.push({
	id: "assistant_other",
	type: "assistant",
	time: { created: 0 },
	agent: "build",
	model: { id: "fixture", providerID: "fixture" },
	content: [{ type: "text", text: reply }],
});
await events.idle("ses_other", other);
assert.equal(
	prompts.length,
	0,
	"Another session's ordinary reply completes independently",
);

const errors: string[] = [];
const originalError = console.error;
console.error = (...values: unknown[]): void => {
	errors.push(values.map(String).join(" "));
	originalError(...values);
};
events.end();
await until(() =>
	errors.some((error) => error.includes("event stream failed")),
);
await assert.rejects(
	() => callContext("ses_native"),
	/event stream is unavailable/,
);
await cleanup?.();
console.error = originalError;
console.log(
	JSON.stringify({ native: true, corrections: prompts.length, cleanup: true }),
);
