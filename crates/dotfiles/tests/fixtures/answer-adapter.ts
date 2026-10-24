import { join } from "node:path";
import type { Hooks, Plugin, WorkspaceAdapter } from "@opencode-ai/plugin";
import { Verification } from "../../../../home/dot_config/opencode/plugins/verification";

const root = Bun.argv[2];
const executable = Bun.argv[3];
if (!root || !executable) throw new Error("The fixture needs its root and native executable");
const session = "answer-fixture";
const prompts: string[] = [];
const client = {
  session: {
    prompt: async (call: { body: { parts: { text: string }[] } }): Promise<object> => {
      prompts.push(call.body.parts.map((part) => part.text).join(""));
      return {};
    },
  },
} as unknown as Parameters<Plugin>[0]["client"];
const hooks = await Verification({
  client,
  project: { id: "fixture", worktree: root, time: { created: 0 } },
  directory: root,
  worktree: root,
  experimental_workspace: { register(_type: string, _adapter: WorkspaceAdapter): void {} },
  serverUrl: new URL("http://127.0.0.1:9"),
  $: Bun.$,
}, { executable });
const message = hooks["chat.message"];
const complete = hooks["experimental.text.complete"];
const transform = hooks["experimental.chat.system.transform"];
const event = hooks.event;
if (!message || !complete || !transform || !event) throw new Error("Answer hooks are missing");
await message({ sessionID: session }, {
  message: {} as Parameters<NonNullable<Hooks["chat.message"]>>[1]["message"],
  parts: [{ type: "text", text: "Check the claim" } as Parameters<NonNullable<Hooks["chat.message"]>>[1]["parts"][number]],
});
const context = { system: [] as string[] };
await transform({ sessionID: session, model: {} as Parameters<typeof transform>[0]["model"] }, context);
const draft = join(root, "draft.txt");
const evidence = join(root, "evidence.json");
await Bun.write(draft, "Can we test this?\n");
await Bun.write(evidence, JSON.stringify({
  version: 1, atoms: [], readings: [], claims: [{ id: "q", text: "Can we test this?", formula: null, basis: { kind: "question" } }],
}));
const checked = Bun.spawn([executable, "answer", "check", "--session", session, "--draft", draft, "--evidence", evidence], {
  cwd: root, stdout: "pipe", stderr: "pipe",
});
const [code, diagnostics] = await Promise.all([checked.exited, new Response(checked.stderr).text(), new Response(checked.stdout).text()]);
if (code !== 0) throw new Error(`The native draft check failed: ${diagnostics}`);
await complete({ sessionID: session, messageID: "good", partID: "text" }, { text: "Can we test this?" });
const idle = { type: "session.idle", properties: { sessionID: session } } as unknown as Parameters<typeof event>[0]["event"];
await event({ event: idle });
const accepted = prompts.length;
await complete({ sessionID: session, messageID: "bad", partID: "text" }, { text: "Can we test that?" });
await event({ event: idle });
await event({ event: idle });
console.log(JSON.stringify({ context: context.system, accepted, prompts }));
