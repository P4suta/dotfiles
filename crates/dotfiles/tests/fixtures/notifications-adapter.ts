import { spyOn } from "bun:test";
import { homedir } from "node:os";
import { join, resolve } from "node:path";
import { writeFile } from "node:fs/promises";
import { createOpencodeClient } from "@opencode-ai/sdk";
import type { Plugin, WorkspaceAdapter } from "@opencode-ai/plugin";

const root = Bun.argv[2];
if (!root || resolve(root) !== resolve(homedir())) throw new Error("The notification fixture must own its isolated home");
const { SpokenNotifications } = await import("../../../../home/dot_config/opencode/plugins/notifications");
const input: Parameters<Plugin>[0] = {
  client: createOpencodeClient({ baseUrl: "http://127.0.0.1:9" }),
  project: { id: "fixture", worktree: root, time: { created: 0 } }, directory: root, worktree: root,
  experimental_workspace: { register(_type: string, _adapter: WorkspaceAdapter): void {} },
  serverUrl: new URL("http://127.0.0.1:9"), $: Bun.$,
};
const calls: string[][] = [];
const warnings: string[] = [];
const warning = "warning: could not deliver the optional spoken notification";
const count = (): number => calls.length;
const warningCount = (): number => warnings.length;
const nativeSpawn = Bun.spawn.bind(Bun);
let failure: "startup" | "terminal" = "startup";
const spawn = spyOn(Bun, "spawn").mockImplementation((...args: unknown[]): Bun.Subprocess<"ignore", "ignore", "ignore"> => {
  const first = args[0];
  if (Array.isArray(first)) calls.push(first.map(String));
  if (failure === "startup") throw new Error("private optional spawn boundary");
  return nativeSpawn([process.execPath, "-e", "process.exit(9)"], { stdin: "ignore", stdout: "ignore", stderr: "ignore" });
});
const diagnostic = spyOn(console, "error").mockImplementation((...messages: unknown[]): void => {
  warnings.push(messages.map(String).join(" "));
});
const clock = spyOn(Date, "now").mockReturnValue(100);
try {
  const hooks = await SpokenNotifications(input);
  const observe = hooks.event;
  if (!observe) throw new Error("The configured notification callback is missing");
  const dispatch = async (type: string, properties: unknown): Promise<void> => {
    const event: Parameters<typeof observe>[0]["event"] = JSON.parse(JSON.stringify({ type, properties }));
    await observe({ event });
  };
  const busy = async (): Promise<void> => {
    await dispatch("message.updated", { info: { id: "user", sessionID: "session", role: "user" } });
    await dispatch("session.status", { sessionID: "session", status: { type: "busy" } });
  };
  const assistant = async (extra: object = {}): Promise<void> => {
    await dispatch("message.updated", { info: { id: "assistant", sessionID: "session", role: "assistant", parentID: "user",
      time: { created: 101, completed: 102 }, finish: "stop", ...extra } });
  };
  const idle = async (): Promise<void> => { await dispatch("session.idle", { sessionID: "session" }); };
  await idle();
  await busy(); await assistant({ time: { created: 1, completed: 2 } }); await idle();
  await busy(); await assistant({ parentID: "older-user" }); await idle();
  await busy(); await assistant({ error: { name: "MessageAbortedError" } }); await idle();
  await busy(); await assistant({ finish: "tool-calls" }); await idle();
  await busy(); await assistant();
  await dispatch("message.part.updated", { part: { id: "tool", sessionID: "session", type: "tool", state: { status: "running" } } });
  await idle();
  if (count() !== 0) throw new Error("A stale, failed, disconnected, or unfinished run announced completion");
  if (warningCount() !== 0) throw new Error("A refused completion emitted a warning");
  await busy(); await assistant(); await idle(); await idle();
  if (count() !== 1 || !calls[0]?.includes("turn-completed")) throw new Error("The genuine run must announce exactly once");
  await dispatch("question.asked", { id: "request", sessionID: "session", questions: [] });
  await dispatch("question.asked", { id: "request", sessionID: "session", questions: [] });
  await dispatch("question.replied", { requestID: "request", sessionID: "session", answers: [] });
  if (count() !== 3 || !calls[1]?.includes("input-waiting") || !calls[2]?.includes("cancel")) throw new Error("Waiting must deduplicate and cancel after resolution");
  if (warningCount() !== 3 || warnings.some((message: string): boolean => message !== warning)) throw new Error("Startup failure diagnostics must be fixed and private");
  failure = "terminal";
  await busy(); await assistant({ id: "terminal-failed-assistant" }); await idle();
  if (count() !== 4 || warningCount() !== 4 || warnings[3] !== warning) throw new Error("A nonzero optional callback must warn without aborting the hook");
  await writeFile(join(root, ".config", "dotfiles", "notifications", "frequency"), "off\n");
  await dispatch("question.asked", { id: "disabled", sessionID: "session" });
  const disabled = await SpokenNotifications(input, { executable: "invalid-relative" });
  if (Object.keys(disabled).length !== 0 || count() !== 4 || warningCount() !== 4) throw new Error("A disabled or cached callback spawned or warned");
} finally { spawn.mockRestore(); clock.mockRestore(); diagnostic.mockRestore(); }
