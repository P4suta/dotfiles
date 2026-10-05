import { LineEndings } from "../../../dot_config/opencode/plugins/line-endings";
import { createOpencodeClient } from "@opencode-ai/sdk";
import type { WorkspaceAdapter } from "@opencode-ai/plugin";

const root = Bun.argv[2];
if (!root) throw new Error("A repository root is required");
const executable = Bun.argv[3];
if (!executable) throw new Error("The native executable is required");
const hooks = await LineEndings({
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
const normalize = hooks["tool.execute.after"];
if (!normalize) throw new Error("The line-ending hook is missing");
await normalize(
  {
    tool: "write",
    sessionID: "fixture-session",
    callID: "fixture-call",
    args: { filePath: `${root}/written.txt` },
  },
  { title: "Write fixture", output: "", metadata: {} },
);
