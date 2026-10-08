import type { Hooks, Plugin } from "@opencode-ai/plugin";
import { homedir } from "node:os";
import { isAbsolute, join } from "node:path";

type BeforeTool = NonNullable<Hooks["tool.execute.before"]>;

/** How long the pre-execution hook may run before the plugin stops waiting on it. */
const HOOK_TIMEOUT_MS = 10000;

/**
 * Refuses a malformed or file-writing shell command before it runs, through the
 * same pure decision core every managed client shares.
 */
export const ShellGuard: Plugin = async (
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
    "tool.execute.before": async (
      call: Parameters<BeforeTool>[0],
      output: Parameters<BeforeTool>[1],
    ): Promise<void> => {
      if (call.tool !== "bash") {
        return;
      }
      const args: unknown = output.args;
      const child = Bun.spawn([executable, "claude-bash"], {
        cwd: input.directory,
        stdin: new Blob([
          JSON.stringify({
            tool_name: "bash",
            cwd: input.directory,
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
        throw new Error(
          diagnostics.trim() || `The shell guard refused this command (${code})`,
        );
      }
    },
  };
};
