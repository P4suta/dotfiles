import { SkillOperations } from "../../../dot_config/opencode/plugins/skill-ops";
import { createOpencodeClient } from "@opencode-ai/sdk";
import type { WorkspaceAdapter } from "@opencode-ai/plugin";

const root = Bun.argv[2];
if (!root) throw new Error("A fixture root is required");
const executable = Bun.argv[3];
if (!executable) throw new Error("The native executable is required");
const hooks = await SkillOperations({
  client: createOpencodeClient({ baseUrl: "http://127.0.0.1:9" }),
  project: { id: "fixture", worktree: root, time: { created: 0 } },
  directory: root,
  worktree: root,
  experimental_workspace: {
    register(_type: string, _adapter: WorkspaceAdapter): void {},
  },
  serverUrl: new URL("http://127.0.0.1:9"),
  $: Bun.$,
}, { executable });
const collect = hooks["tool.execute.after"];
if (!collect) throw new Error("The native observer is missing");
const policy = Bun.env.SKILL_OPS_POLICY;
if (!policy) throw new Error("The native policy is required");
const policyText = await Bun.file(policy).text();
await Bun.write(policy, "not valid JSON");
try {
  for (const tool of ["edit", "write", "glob", "grep", "webfetch"]) {
    await collect(
      { tool, sessionID: "unused-session", callID: tool, args: {} },
      { title: tool, output: "", metadata: {} },
    );
  }
} finally {
  await Bun.write(policy, policyText);
}
const filePath = `${root}/tree/alpha/SKILL.md`;
await collect(
  {
    tool: "read",
    sessionID: "private-fixture-session",
    callID: "private-fixture-call",
    args: { filePath },
  },
  { title: "Read fixture skill", output: await Bun.file(filePath).text(), metadata: {} },
);
