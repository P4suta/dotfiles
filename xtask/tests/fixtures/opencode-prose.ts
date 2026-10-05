import { ProseReply } from "../../../dot_config/opencode/plugins/prose";
import type { PluginInput, WorkspaceAdapter } from "@opencode-ai/plugin";

const executable = Bun.argv[2];
if (!executable) throw new Error("The native executable is required");
const reply = Bun.argv[3];
if (reply === undefined) throw new Error("The reply text is required");

const prompts: string[] = [];
const client = {
  session: {
    messages: async () => ({
      data: [
        { info: { role: "user" }, parts: [{ type: "text", text: "Fix the parser." }] },
        { info: { role: "assistant" }, parts: [{ type: "text", text: reply }] },
      ],
    }),
    promptAsync: async (request: { body: { parts: { text: string }[] } }) => {
      prompts.push(request.body.parts.map((part) => part.text).join("\n"));
      return { data: undefined };
    },
  },
} as unknown as PluginInput["client"];

const hooks = await ProseReply({
  client,
  project: { id: "fixture", worktree: ".", time: { created: 0 } },
  directory: ".",
  worktree: ".",
  experimental_workspace: {
    register(_type: string, _adapter: WorkspaceAdapter): void {},
  },
  serverUrl: new URL("http://127.0.0.1:9"),
  $: Bun.$,
}, { executable });
const event = hooks.event;
if (!event) throw new Error("The reply check is missing");
const idle = { event: { type: "session.idle" as const, properties: { sessionID: "fixture" } } };
await event(idle);
await event(idle);
console.log(JSON.stringify(prompts));
