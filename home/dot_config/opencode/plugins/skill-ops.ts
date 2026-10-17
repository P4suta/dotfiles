import type { Hooks, Plugin } from "@opencode-ai/plugin";
import { homedir } from "node:os";
import { isAbsolute, join } from "node:path";
import { lstat, readFile } from "node:fs/promises";

type AfterTool = NonNullable<Hooks["tool.execute.after"]>;
const observedTools = new Set(["read", "skill", "bash"]);

/** How long an observation may run before the plugin stops waiting on it. */
const OBSERVE_TIMEOUT_MS = 8000;
const DISABLED_MARKER = join(homedir(), ".config", "skill-ops", "disabled");
const EXPERIMENTAL_OPT_IN = join(homedir(), ".config", "skill-ops", "experimental");
const OPT_IN_CONTENT = "enabled\n";

async function experimentalEnabled(): Promise<boolean> {
  try {
    for (const parent of [join(homedir(), ".config"), join(homedir(), ".config", "skill-ops")]) {
      if (!(await lstat(parent)).isDirectory()) return false;
    }
    try {
      await lstat(DISABLED_MARKER);
      return false;
    } catch (error: unknown) {
      if (typeof error !== "object" || error === null || !("code" in error) || error.code !== "ENOENT") return false;
    }
    const consent = await lstat(EXPERIMENTAL_OPT_IN);
    if (!consent.isFile() || consent.size !== OPT_IN_CONTENT.length) return false;
    return await readFile(EXPERIMENTAL_OPT_IN, "utf8") === OPT_IN_CONTENT;
  } catch {
    return false;
  }
}

export const SkillOperations: Plugin = async (
  _input: Parameters<Plugin>[0],
  options: Parameters<Plugin>[1] = {},
): Promise<Hooks> => {
  if (!await experimentalEnabled()) return {};
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
      if (!await experimentalEnabled()) return;
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
        timeout: OBSERVE_TIMEOUT_MS,
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
