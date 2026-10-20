import { Verification } from "../../../../home/dot_config/opencode/plugins/verification";
import type { Plugin, WorkspaceAdapter } from "@opencode-ai/plugin";

type Prompt = { path: { id: string }; body: { parts: { type: string; text: string }[] } };

const root = Bun.argv[2];
if (!root) throw new Error("A repository root is required");
const executable = Bun.argv[3];
if (!executable) throw new Error("The native executable is required");
const session = "fixture-session";
const prompts: string[] = [];
const client = {
  session: {
    prompt: async (options: Prompt): Promise<object> => {
      prompts.push(`${options.path.id}: ${options.body.parts.map((part) => part.text).join("")}`);
      return {};
    },
  },
} as unknown as Parameters<Plugin>[0]["client"];
const hooks = await Verification({
  client,
  project: { id: "fixture", worktree: root, time: { created: 0 } },
  directory: root,
  worktree: root,
  experimental_workspace: {
    register(_type: string, _adapter: WorkspaceAdapter): void {},
  },
  serverUrl: new URL("http://127.0.0.1:9"),
  $: Bun.$,
}, { executable });
const transform = hooks["experimental.chat.system.transform"];
const event = hooks.event;
if (!transform || !event) throw new Error("The verification hooks are missing");
const model = {} as Parameters<typeof transform>[0]["model"];
const first = { system: [] as string[] };
await transform({ sessionID: session, model }, first);
const repeated = { system: [] as string[] };
await transform({ sessionID: session, model }, repeated);
const idle = { type: "session.idle", properties: { sessionID: session } } as unknown as Parameters<typeof event>[0]["event"];
await event({ event: idle });
const afterFirstStop = prompts.length;
await event({ event: idle });
console.log(JSON.stringify({ system: first.system, repeated: repeated.system, prompts, afterFirstStop }));
