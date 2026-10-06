import type { Hooks, Plugin } from "@opencode-ai/plugin";
import { homedir } from "node:os";
import { isAbsolute, join } from "node:path";

type AfterTool = NonNullable<Hooks["tool.execute.after"]>;
const observedTools = new Set(["read", "skill", "bash"]);

export const SkillOperations: Plugin = async (
  _input: Parameters<Plugin>[0],
  options: Parameters<Plugin>[1] = {},
): Promise<Hooks> => {
  const executable = options.executable ?? join(
    homedir(),
    ".local",
    "bin",
    `skill-ops${process.platform === "win32" ? ".exe" : ""}`,
  );
  if (typeof executable !== "string" || !isAbsolute(executable)) {
    throw new Error("The skill-ops executable must be an absolute path");
  }
  return {
    "tool.execute.after": async (
      input: Parameters<AfterTool>[0],
      output: Parameters<AfterTool>[1],
    ): Promise<void> => {
      if (!observedTools.has(input.tool)) return;
      const args: unknown = input.args;
      const child = Bun.spawn([executable, "observe", "--client", "opencode"], {
        stdin: new Blob([
          JSON.stringify({
            hook_event_name: "PostToolUse",
            session_id: input.sessionID,
            tool_use_id: input.callID,
            tool_name: input.tool,
            tool_input: args,
            tool_response: { output: output.output },
          }),
        ]),
        stdout: "ignore",
        stderr: "pipe",
        timeout: 8000,
      });
      const [code, diagnostics] = await Promise.all([
        child.exited,
        new Response(child.stderr).text(),
      ]);
      if (code !== 0) {
        throw new Error(`Skill observation failed (${code}): ${diagnostics}`);
      }
    },
  };
};
