import type { Hooks, Plugin } from "@opencode-ai/plugin";
import { homedir } from "node:os";
import { isAbsolute, join } from "node:path";

type AfterTool = NonNullable<Hooks["tool.execute.after"]>;

/** How long the line-ending hook may run before the plugin stops waiting on it. */
const HOOK_TIMEOUT_MS = 25000;

export const LineEndings: Plugin = async (
  input: Parameters<Plugin>[0],
  options: Parameters<Plugin>[1] = {},
): Promise<Hooks> => {
  const executable = options.executable ?? join(
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
      _output: Parameters<AfterTool>[1],
    ): Promise<void> => {
      const args: unknown = call.args;
      const child = Bun.spawn([executable, "line-endings", "hook"], {
        cwd: input.directory,
        stdin: new Blob([
          JSON.stringify({
            hook_event_name: "PostToolUse",
            cwd: input.directory,
            tool_name: call.tool,
            tool_input: args,
          }),
        ]),
        stdout: "ignore",
        stderr: "pipe",
        timeout: HOOK_TIMEOUT_MS,
      });
      const [code, diagnostics] = await Promise.all([
        child.exited,
        new Response(child.stderr).text(),
      ]);
      if (code !== 0) {
        throw new Error(`Line-ending normalization failed (${code}): ${diagnostics}`);
      }
    },
  };
};
