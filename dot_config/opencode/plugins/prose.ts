import type { Hooks, Plugin } from "@opencode-ai/plugin";
import { homedir } from "node:os";
import { isAbsolute, join } from "node:path";

type EventHook = NonNullable<Hooks["event"]>;

function blockReason(response: unknown): string | undefined {
  if (typeof response !== "object" || response === null) return undefined;
  if (!("decision" in response) || response.decision !== "block") return undefined;
  if (!("reason" in response) || typeof response.reason !== "string") return undefined;
  return response.reason;
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
  const event: EventHook = async ({ event }: Parameters<EventHook>[0]): Promise<void> => {
    if (event.type !== "session.idle") return;
    const sessionID = event.properties.sessionID;
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
    const reason = blockReason(JSON.parse(output));
    if (reason === undefined) {
      rewriting.delete(sessionID);
      return;
    }
    rewriting.add(sessionID);
    await input.client.session.promptAsync({
      path: { id: sessionID },
      body: { parts: [{ type: "text", text: reason }] },
    });
  };
  return { event };
};
