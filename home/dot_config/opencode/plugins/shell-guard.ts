import type { Plugin as NativePlugin } from "@opencode/plugin";
import type { Hooks, Plugin as LegacyPlugin } from "@opencode-ai/plugin";
import { executable, runHook, toolName } from "../adapters/native-runtime";

async function guard(
	binary: string,
	cwd: string,
	tool: string,
	args: unknown,
	session?: string,
): Promise<void> {
	const mutation = ["edit", "write", "apply_patch"].includes(tool);
	if (tool !== "bash" && !mutation) return;
	await runHook(binary, cwd, [mutation ? "issue-guard" : "claude-bash"], {
		session_id: session,
		tool_name: tool,
		cwd,
		tool_input: args,
	});
}

export const ShellGuard: LegacyPlugin = async (
	input: Parameters<LegacyPlugin>[0],
	options: Parameters<LegacyPlugin>[1] = {},
): Promise<Hooks> => {
	const binary = executable(options);
	return {
		"tool.execute.before": async (call, output): Promise<void> => {
			await guard(
				binary,
				input.directory,
				call.tool,
				output.args,
				call.sessionID,
			);
		},
	};
};

const native: NativePlugin.Plugin = {
	id: "dotfiles.shell-guard",
	async setup(ctx) {
		const binary = executable(ctx.options);
		await ctx.tool.hook("execute.before", async (event) => {
			const tool = toolName(event.tool);
			if (!["bash", "edit", "write", "apply_patch"].includes(tool)) return;
			const session = await ctx.session.get({ sessionID: event.sessionID });
			await guard(
				binary,
				session.location.directory,
				tool,
				event.input,
				event.sessionID,
			);
		});
		await ctx.shell.hook("create.before", async (event) => {
			await guard(binary, event.cwd, "bash", { command: event.command });
		});
	},
};

export default { ...native, server: ShellGuard };
