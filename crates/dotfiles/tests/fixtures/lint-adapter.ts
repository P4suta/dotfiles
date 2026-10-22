import type { WorkspaceAdapter } from "@opencode-ai/plugin";
import { createOpencodeClient } from "@opencode-ai/sdk";
import { Lint } from "../../../../home/dot_config/opencode/plugins/lint";

const FIXTURE_URL = "http://127.0.0.1:9";
const root = Bun.argv[2];
const executable = Bun.argv[3];
const expected = Bun.argv[4];
if (!root || !executable || !expected)
	throw new Error("Missing fixture arguments");

const hooks = await Lint(
	{
		client: createOpencodeClient({ baseUrl: FIXTURE_URL }),
		project: { id: "fixture", worktree: root, time: { created: 0 } },
		directory: root,
		worktree: root,
		experimental_workspace: {
			register(_type: string, _adapter: WorkspaceAdapter): void {},
		},
		serverUrl: new URL(FIXTURE_URL),
		$: Bun.$,
	},
	{ executable },
);

const lint = hooks["tool.execute.after"];
if (!lint) throw new Error("Missing lint hook");
let blocked = false;
try {
	await lint(
		{
			tool: "write",
			sessionID: "fixture",
			callID: "fixture",
			args: { filePath: `${root}/probe.sh` },
		},
		{ title: "Write", output: "", metadata: {} },
	);
} catch (error: unknown) {
	if (!String(error).includes("shellcheck")) throw error;
	blocked = true;
}
if (blocked !== (expected === "blocked")) {
	throw new Error(`Expected ${expected}, observed blocked=${blocked}`);
}
