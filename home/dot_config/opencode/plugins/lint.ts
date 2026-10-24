import { homedir } from "node:os";
import { isAbsolute, join } from "node:path";
import type { Hooks, Plugin } from "@opencode-ai/plugin";

type AfterTool = NonNullable<Hooks["tool.execute.after"]>;

const HOOK_TIMEOUT_MS = 120000;

export const Lint: Plugin = async (
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
	return {
		"tool.execute.after": async (
			call: Parameters<AfterTool>[0],
			output: Parameters<AfterTool>[1],
		): Promise<void> => {
			if (!["edit", "write", "apply_patch"].includes(call.tool)) return;
			const args: unknown = call.args;
			const child = Bun.spawn([executable, "lint-hook"], {
				cwd: input.directory,
				stdin: new Blob([
					JSON.stringify({
						hook_event_name: "PostToolUse",
						cwd: input.directory,
						tool_name: call.tool,
						tool_input: args,
					}),
				]),
				stdout: "pipe",
				stderr: "pipe",
				timeout: HOOK_TIMEOUT_MS,
			});
			const [code, response, diagnostics] = await Promise.all([
				child.exited,
				new Response(child.stdout).text(),
				new Response(child.stderr).text(),
			]);
			if (code !== 0)
				throw new Error(diagnostics || `Lint hook failed (${code})`);
			if (!response.trim()) return;
			const result: unknown = JSON.parse(response);
			if (
				typeof result !== "object" ||
				result === null ||
				!("decision" in result)
			) {
				throw new Error("The lint hook returned an invalid decision");
			}
			if (
				result.decision === "block" &&
				"reason" in result &&
				typeof result.reason === "string"
			) {
				output.output = result.reason;
				throw new Error(result.reason);
			}
		},
	};
};
