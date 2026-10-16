import { spyOn } from "bun:test";
import { homedir } from "node:os";
import { join, resolve } from "node:path";
import { mkdir, rename, rmdir, unlink, writeFile } from "node:fs/promises";
import { createOpencodeClient } from "@opencode-ai/sdk";
import type { Plugin, WorkspaceAdapter } from "@opencode-ai/plugin";

const root = Bun.argv[2];
if (!root || resolve(homedir()) !== resolve(root)) throw new Error("The observer fixture must own its isolated home");
const { SkillOperations } = await import("../../../../home/dot_config/opencode/plugins/skill-ops");
const input: Parameters<Plugin>[0] = {
  client: createOpencodeClient({ baseUrl: "http://127.0.0.1:9" }),
  project: { id: "fixture", worktree: root, time: { created: 0 } },
  directory: root,
  worktree: root,
  experimental_workspace: { register(_type: string, _adapter: WorkspaceAdapter): void {} },
  serverUrl: new URL("http://127.0.0.1:9"),
  $: Bun.$,
};
const config = join(root, ".config", "skill-ops");
const optIn = join(config, "experimental");
const stop = join(config, "disabled");
const spawn = spyOn(Bun, "spawn").mockImplementation(() => { throw new Error("fixture spawn boundary"); });
const spawnCount = (): number => spawn.mock.calls.length;
const assertNoHooks = async (): Promise<void> => {
  const hooks = await SkillOperations(input, { executable: "invalid-relative-executable" });
  if (Object.keys(hooks).length !== 0 || spawnCount() !== 0) throw new Error("A disabled plugin registered or spawned an observer");
};
try {
  await unlink(optIn);
  await assertNoHooks();
  for (const content of ["enabled", "enabled\r\n", "true\n", " enabled\n"]) {
    await writeFile(optIn, content);
    await assertNoHooks();
  }
  await unlink(optIn);
  await mkdir(optIn);
  await assertNoHooks();
  await rmdir(optIn);
  await rename(config, `${config}.saved`);
  try {
    await writeFile(config, "not a directory");
    await assertNoHooks();
  } finally {
    await unlink(config);
    await rename(`${config}.saved`, config);
  }
  await writeFile(optIn, "enabled\n");
  const hooks = await SkillOperations(input);
  const collect = hooks["tool.execute.after"];
  if (!collect) throw new Error("Explicit consent did not restore the observer");
  const event = { tool: "read", sessionID: "fixture", callID: "fixture", args: {} };
  const output = { title: "fixture", output: "", metadata: {} };
  try {
    await collect(event, output);
    throw new Error("The enabled control missed the actual spawn boundary");
  } catch (error: unknown) {
    if (!(error instanceof Error) || error.message !== "fixture spawn boundary" || spawnCount() !== 1) throw error;
  }
  spawn.mockClear();
  await writeFile(stop, "owner stop");
  await assertNoHooks();
  for (const tool of ["read", "skill", "bash"]) {
    await collect({ ...event, tool }, output);
  }
  await unlink(stop);
  await unlink(optIn);
  await collect(event, output);
  await writeFile(optIn, "enabled\r\n");
  await collect(event, output);
  if (spawnCount() !== 0) throw new Error("A cached callback spawned after consent was revoked");
} finally {
  spawn.mockRestore();
}
