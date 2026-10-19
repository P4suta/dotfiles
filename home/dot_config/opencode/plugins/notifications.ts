import type { Hooks, Plugin } from "@opencode-ai/plugin";
import { homedir } from "node:os";
import { isAbsolute, join } from "node:path";
import { lstat, readFile } from "node:fs/promises";

const MAX_SESSIONS = 32;
const MAX_PENDING_TOOLS = 128;
const MAX_REQUESTS = 64;
const CALLBACK_TIMEOUT_MS = 1000;
const MAX_TEXT_CODE_UNITS = 256;
const MAX_MODE_FILE_BYTES = 16;
const DELIVERY_WARNING = "warning: could not deliver the optional spoken notification";
const CONFIG = join(homedir(), ".config", "dotfiles", "notifications");
const MODE = join(homedir(), ".local", "state", "dotfiles", "notifications", "mode");
type SpokenEvent = "turn-completed" | "input-waiting";
type Completion = { id: string; parent: string; created: number; completed: number };
type Run = { user?: string; busy: boolean; started: number; failed: boolean; complete?: Completion; tools: Set<string> };
type JsonObject = { [key: string]: unknown };

function object(value: unknown): JsonObject | undefined {
  return typeof value === "object" && value !== null && !Array.isArray(value) ? value as JsonObject : undefined;
}
function text(value: unknown): string | undefined {
  return typeof value === "string" && value.length > 0 && value.length <= MAX_TEXT_CODE_UNITS ? value : undefined;
}
function instant(value: unknown): number | undefined {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0 ? value : undefined;
}

async function configured(): Promise<boolean> {
  if (["SSH_CONNECTION", "SSH_TTY", "CI", "DOTFILES_HOST_CACHE", "WSL_DISTRO_NAME", "DOTFILES_NOTIFICATION_INTERNAL"].some((name: string): boolean => process.env[name] !== undefined)) return false;
  try {
    let mode = MODE;
    try { await lstat(mode); } catch (error: unknown) {
      if (object(error)?.code !== "ENOENT") return false;
      mode = join(CONFIG, "frequency");
    }
    const metadata = await lstat(mode);
    const manifest = await lstat(join(CONFIG, "materials.tsv"));
    return metadata.isFile() && metadata.size <= MAX_MODE_FILE_BYTES && manifest.isFile() && ["all\n", "results\n", "attention\n"].includes(await readFile(mode, "utf8"));
  } catch { return false; }
}

/** Uses observed run metadata rather than an idle or Stop event alone as completion evidence. */
export const SpokenNotifications: Plugin = async (
  _input: Parameters<Plugin>[0],
  options: Parameters<Plugin>[1] = {},
): Promise<Hooks> => {
  if (!await configured()) return {};
  const executable = options.executable ?? join(homedir(), ".local", "bin", `dotfiles${process.platform === "win32" ? ".exe" : ""}`);
  if (typeof executable !== "string" || !isAbsolute(executable)) return {};
  const runs = new Map<string, Run>();
  const requests = new Map<string, string>();
  const completed = new Set<string>();
  const deliver = async (event: SpokenEvent | "cancel", id: string): Promise<void> => {
    if (!await configured()) return;
    try {
      const argv = event === "cancel" ? [executable, "notifications", "cancel", "--id", id]
        : [executable, "notifications", "emit", event, "--id", id];
      const child = Bun.spawn(argv, { stdin: "ignore", stdout: "ignore", stderr: "ignore", timeout: CALLBACK_TIMEOUT_MS });
      if (await child.exited !== 0) console.error(DELIVERY_WARNING);
    } catch { console.error(DELIVERY_WARNING); }
  };
  const runFor = (session: string): Run => {
    const prior = runs.get(session);
    if (prior) return prior;
    if (runs.size >= MAX_SESSIONS) {
      const oldest = runs.keys().next().value;
      if (oldest !== undefined) runs.delete(oldest);
    }
    const fresh: Run = { busy: false, started: 0, failed: false, tools: new Set<string>() };
    runs.set(session, fresh);
    return fresh;
  };
  return {
    event: async ({ event }: Parameters<NonNullable<Hooks["event"]>>[0]): Promise<void> => {
      if (!await configured()) return;
      // The pinned V1 plugin receives the runtime bus and validates documented question fields explicitly.
      const raw: unknown = event;
      const input = object(raw);
      const properties = object(input?.properties);
      const kind = text(input?.type);
      if (!kind || !properties) return;
      if (["question.asked", "permission.asked", "permission.updated"].includes(kind)) {
        const session = text(properties.sessionID);
        const request = text(properties.id);
        if (!session || !request || requests.has(request) || requests.size >= MAX_REQUESTS) return;
        const id = `opencode:waiting:${session}:${request}`;
        requests.set(request, id);
        await deliver("input-waiting", id);
        return;
      }
      if (["question.replied", "question.rejected", "permission.replied"].includes(kind)) {
        const request = text(properties.requestID) ?? text(properties.permissionID);
        if (!request) return;
        const id = requests.get(request);
        requests.delete(request);
        if (id) await deliver("cancel", id);
        return;
      }
      if (kind === "message.updated") {
        const info = object(properties.info);
        const session = text(info?.sessionID);
        const id = text(info?.id);
        if (!session || !id || !info) return;
        const run = runFor(session);
        if (info.role === "user") { run.user = id; return; }
        if (info.role !== "assistant" || !run.busy || info.summary === true) return;
        const time = object(info.time);
        const created = instant(time?.created);
        const finished = instant(time?.completed);
        const parent = text(info.parentID);
        if (info.error !== undefined) { run.failed = true; delete run.complete; return; }
        if (created === undefined || finished === undefined || !parent || created < run.started || finished < created
          || !["stop", "end_turn"].includes(String(info.finish))) return;
        run.complete = { id, parent, created, completed: finished };
        return;
      }
      if (kind === "message.part.updated") {
        const part = object(properties.part);
        const session = text(part?.sessionID);
        const id = text(part?.id);
        if (!session || !id || part?.type !== "tool") return;
        const run = runs.get(session);
        if (!run?.busy) return;
        const state = object(part.state);
        if (state?.status === "pending" || state?.status === "running") {
          if (run.tools.size >= MAX_PENDING_TOOLS) run.failed = true;
          else run.tools.add(id);
        } else if (state?.status === "completed" || state?.status === "error") run.tools.delete(id);
        else run.failed = true;
        return;
      }
      const session = text(properties.sessionID);
      if (!session) return;
      if (kind === "session.error") { const run = runs.get(session); if (run) run.failed = true; return; }
      if (kind === "session.deleted") { runs.delete(session); return; }
      const status = kind === "session.status" ? object(properties.status)?.type : kind === "session.idle" ? "idle" : undefined;
      if (status === "busy") {
        const run = runFor(session);
        if (!run.busy) { run.busy = true; run.started = Date.now(); run.failed = false; delete run.complete; run.tools.clear(); }
        return;
      }
      if (status !== "idle") return;
      const run = runs.get(session);
      if (!run?.busy) return;
      run.busy = false;
      const candidate = run.complete;
      if (!candidate || run.failed || run.tools.size > 0 || candidate.parent !== run.user || completed.has(candidate.id)) return;
      if (completed.size >= MAX_REQUESTS) { const oldest = completed.values().next().value; if (oldest !== undefined) completed.delete(oldest); }
      completed.add(candidate.id);
      await deliver("turn-completed", `opencode:complete:${session}:${candidate.id}`);
    },
  };
};
