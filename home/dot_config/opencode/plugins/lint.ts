import type { Plugin as NativePlugin } from "@opencode/plugin";
import type { Hooks, Plugin as LegacyPlugin } from "@opencode-ai/plugin";
import { executable, runHook, toolName } from "../adapters/native-runtime";

async function finding(
	binary: string,
	cwd: string,
	tool: string,
	args: unknown,
): Promise<string | undefined> {
	if (!["edit", "write", "apply_patch"].includes(tool)) return;
	const response = await runHook(binary, cwd, ["lint-hook"], {
		hook_event_name: "PostToolUse",
		cwd,
		tool_name: tool,
		tool_input: args,
	});
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
		return result.reason;
	}
	return undefined;
}

export const Lint: LegacyPlugin = async (
	input: Parameters<LegacyPlugin>[0],
	options: Parameters<LegacyPlugin>[1] = {},
): Promise<Hooks> => {
	const binary = executable(options);
	return {
		"tool.execute.after": async (call, output): Promise<void> => {
			const reason = await finding(
				binary,
				input.directory,
				call.tool,
				call.args,
			);
			if (reason) {
				output.output = reason;
				throw new Error(reason);
			}
		},
	};
};

const native: NativePlugin.Plugin = {
	id: "dotfiles.lint",
	async setup(ctx) {
		const binary = executable(ctx.options);
		await ctx.tool.hook("execute.after", async (event) => {
			if (event.status !== "completed") return;
			const tool = toolName(event.tool);
			if (!["edit", "write", "apply_patch"].includes(tool)) return;
			const session = await ctx.session.get({ sessionID: event.sessionID });
			const reason = await finding(
				binary,
				session.location.directory,
				tool,
				event.input,
			);
			if (reason) throw new Error(reason);
		});
	},
};

export default { ...native, server: Lint };
