import { homedir } from "node:os";
import { isAbsolute, join } from "node:path";
import type { Hooks, Plugin } from "@opencode-ai/plugin";

type SystemTransform = NonNullable<Hooks["experimental.chat.system.transform"]>;
type JsonObject = { [key: string]: unknown };

/** How long a verification hook may run before the plugin stops waiting on it. */
const HOOK_TIMEOUT_MS = 60000;
/** How many sessions the plugin remembers before it forgets the oldest. */
const MAX_SESSIONS = 32;

function object(value: unknown): JsonObject | undefined {
  return typeof value === "object" && value !== null && !Array.isArray(value) ? value as JsonObject : undefined;
}

function text(value: unknown): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

/**
 * Runs the verification session the other clients run from their hooks.
 * A session's system prompt carries the start report, which lists the code shapes nothing in the repository verifies.
 * When a turn ends with such a shape in a changed file, the plugin sends the refusal back once, and a second refusal ends the turn with a warning.
 */
export const Verification: Plugin = async (
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
  const reports = new Map<string, string>();
  const continued = new Set<string>();
  const answerContexts = new Map<string, string>();
  const answerParts = new Map<string, { message: string; parts: Map<string, string> }>();
  const continuations = new Map<string, string>();
  const remember = (session: string, report: string): void => {
    if (reports.size >= MAX_SESSIONS) {
      const oldest = reports.keys().next().value;
      if (oldest !== undefined) {
        reports.delete(oldest);
        continued.delete(oldest);
        answerContexts.delete(oldest);
        answerParts.delete(oldest);
        continuations.delete(oldest);
      }
    }
    reports.set(session, report);
  };
  const runtime = async (command: "assure" | "answer", point: "start" | "begin" | "stop", stdin: string): Promise<string> => {
    const child = Bun.spawn([executable, command, point], {
      cwd: input.directory,
      stdin: new Blob([stdin]),
      stdout: "pipe",
      stderr: "pipe",
      timeout: HOOK_TIMEOUT_MS,
    });
    const [code, output, diagnostics] = await Promise.all([
      child.exited,
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
    ]);
    if (code !== 0) {
      throw new Error(`The ${command} ${point} hook failed (${code}): ${diagnostics}`);
    }
    return output;
  };
  return {
    "chat.message": async (
      call: Parameters<NonNullable<Hooks["chat.message"]>>[0],
      output: Parameters<NonNullable<Hooks["chat.message"]>>[1],
    ): Promise<void> => {
      const prompt = output.parts.filter((part) => part.type === "text").map((part) => part.text).join("\n");
      if (continuations.get(call.sessionID) === prompt) return;
      const context = await runtime("answer", "begin", JSON.stringify({ session_id: call.sessionID, prompt }));
      if (context.trimStart().startsWith("{")) {
        const refusal = object(JSON.parse(context));
        if (refusal?.decision === "block") throw new Error(text(refusal.reason) ?? "The answer request could not start");
      }
      answerContexts.set(call.sessionID, context.trim());
      answerParts.delete(call.sessionID);
      continuations.delete(call.sessionID);
      continued.delete(call.sessionID);
    },
    "experimental.text.complete": async (
      call: Parameters<NonNullable<Hooks["experimental.text.complete"]>>[0],
      output: Parameters<NonNullable<Hooks["experimental.text.complete"]>>[1],
    ): Promise<void> => {
      let current = answerParts.get(call.sessionID);
      if (!current || current.message !== call.messageID) {
        current = { message: call.messageID, parts: new Map<string, string>() };
        answerParts.set(call.sessionID, current);
      }
      current.parts.set(call.partID, output.text);
    },
    "experimental.chat.system.transform": async (
      call: Parameters<SystemTransform>[0],
      output: Parameters<SystemTransform>[1],
    ): Promise<void> => {
      const session = call.sessionID;
      if (!session) return;
      let report = reports.get(session);
      if (report === undefined) {
        report = (await runtime("assure", "start", "")).trim();
        remember(session, report);
      }
      if (report) output.system.push(report);
      const answerContext = answerContexts.get(session);
      if (answerContext) output.system.push(answerContext);
    },
    event: async ({ event }: Parameters<NonNullable<Hooks["event"]>>[0]): Promise<void> => {
      const raw: unknown = event;
      const bus = object(raw);
      if (bus?.type !== "session.idle") return;
      const session = text(object(bus.properties)?.sessionID);
      if (!session) return;
      const again = continued.has(session);
      const reply = [...(answerParts.get(session)?.parts.values() ?? [])].join("\n\n");
      const payload = JSON.stringify({ session_id: session, stop_hook_active: again, last_assistant_message: reply });
      const [verification, evidence] = await Promise.all([
        runtime("assure", "stop", payload),
        runtime("answer", "stop", payload),
      ]);
      const decisions = [object(JSON.parse(verification)), object(JSON.parse(evidence))];
      const end = decisions.find((decision) => decision?.continue === false);
      const reason = decisions.filter((decision) => decision?.decision === "block").map((decision) => text(decision?.reason)).filter((reason): reason is string => reason !== undefined).join("\n\n");
      const answer = end ?? (reason ? { decision: "block", reason } : {});
      if (answer.decision === "block" && reason) {
        continued.add(session);
        continuations.set(session, reason);
        await input.client.session.prompt({
          path: { id: session },
          body: { parts: [{ type: "text", text: reason }] },
        });
        return;
      }
      continued.delete(session);
      continuations.delete(session);
      const stopped = text(answer?.stopReason);
      if (answer?.continue === false && stopped) {
        console.error(`${text(answer.systemMessage) ?? "Verification gaps remain"}\n${stopped}`);
      }
    },
  };
};
