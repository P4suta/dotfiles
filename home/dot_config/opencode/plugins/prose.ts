import type { Hooks, Plugin } from "@opencode-ai/plugin";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { isAbsolute, join } from "node:path";

type EventHook = NonNullable<Hooks["event"]>;

type Verdict =
  | { kind: "allow" }
  | { kind: "rewrite"; reason: string }
  | { kind: "end"; reason: string };

function verdict(response: unknown): Verdict {
  if (typeof response !== "object" || response === null) return { kind: "allow" };
  if ("decision" in response && response.decision === "block" && "reason" in response && typeof response.reason === "string") {
    return { kind: "rewrite", reason: response.reason };
  }
  if ("continue" in response && response.continue === false) {
    const reason = "stopReason" in response && typeof response.stopReason === "string" ? response.stopReason : "";
    return { kind: "end", reason };
  }
  return { kind: "allow" };
}

export const ProseReply: Plugin = async (
  input: Parameters<Plugin>[0],
  options: Parameters<Plugin>[1] = {},
): Promise<Hooks> => {
  const executable = options.executable ?? join(
    homedir(),
    ".local",
    "bin",
    `prose${process.platform === "win32" ? ".exe" : ""}`,
  );
  if (typeof executable !== "string" || !isAbsolute(executable)) {
    throw new Error("The prose executable must be an absolute path");
  }
  const rewriting = new Set<string>();
  const warned = new Set<string>();
  const show = async (message: string): Promise<void> => {
    await input.client.tui.showToast({ body: { title: "Prose check", message, variant: "error" } });
  };
  const event: EventHook = async ({ event }: Parameters<EventHook>[0]): Promise<void> => {
    if (event.type !== "session.idle") return;
    const sessionID = event.properties.sessionID;
    if (!existsSync(executable)) {
      if (!warned.has(sessionID)) {
        warned.add(sessionID);
        await show(`The prose checker is not installed at ${executable}; run \`mise run install:prose\` in the dotfiles source.`);
      }
      return;
    }
    const listed = await input.client.session.messages({ path: { id: sessionID } });
    const last = (listed.data ?? []).findLast((message) => message.info.role === "assistant");
    if (!last) return;
    const text = last.parts
      .flatMap((part) => (part.type === "text" && !part.synthetic ? [part.text] : []))
      .join("\n\n");
    const child = Bun.spawn([executable, "reply", "--client", "opencode"], {
      stdin: new Blob([
        JSON.stringify({ last_assistant_message: text, stop_hook_active: rewriting.has(sessionID) }),
      ]),
      stdout: "pipe",
      stderr: "pipe",
      timeout: 120000,
    });
    const [code, output, diagnostics] = await Promise.all([
      child.exited,
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
    ]);
    if (code !== 0) {
      throw new Error(`The reply check failed (${code}): ${diagnostics}`);
    }
    const result = verdict(JSON.parse(output));
    if (result.kind !== "rewrite") {
      rewriting.delete(sessionID);
      if (result.kind === "end") await show(result.reason);
      return;
    }
    rewriting.add(sessionID);
    await input.client.session.promptAsync({
      path: { id: sessionID },
      body: { parts: [{ type: "text", text: result.reason }] },
    });
  };
  return { event };
};
