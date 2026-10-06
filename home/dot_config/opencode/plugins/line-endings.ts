import type { Hooks, Plugin } from "@opencode-ai/plugin";
import { homedir } from "node:os";
import { isAbsolute, join } from "node:path";

type AfterTool = NonNullable<Hooks["tool.execute.after"]>;

export const LineEndings: Plugin = async (
  input: Parameters<Plugin>[0],
  options: Parameters<Plugin>[1] = {},
): Promise<Hooks> => {
  const executable = options.executable ?? join(
    homedir(),
    ".local",
    "bin",
    `dotfiles-xtask${process.platform === "win32" ? ".exe" : ""}`,
  );
  if (typeof executable !== "string" || !isAbsolute(executable)) {
    throw new Error("The dotfiles-xtask executable must be an absolute path");
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
        timeout: 25000,
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
