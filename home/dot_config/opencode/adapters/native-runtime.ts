import { homedir } from "node:os";
import { isAbsolute, join } from "node:path";

const HOOK_TIMEOUT_MS = 120000;

export function executable(options: Readonly<Record<string, unknown>>): string {
	const value =
		options.executable ??
		join(
			homedir(),
			".local",
			"bin",
			`dotfiles${process.platform === "win32" ? ".exe" : ""}`,
		);
	if (typeof value !== "string" || !isAbsolute(value)) {
		throw new Error("The dotfiles executable must be an absolute path");
	}
	return value;
}

export async function runHook(
	binary: string,
	cwd: string,
	args: readonly string[],
	payload: unknown,
): Promise<string> {
	const child = Bun.spawn([binary, ...args], {
		cwd,
		stdin: new Blob([JSON.stringify(payload)]),
		stdout: "pipe",
		stderr: "pipe",
		timeout: HOOK_TIMEOUT_MS,
	});
	const [code, output, diagnostics] = await Promise.all([
		child.exited,
		new Response(child.stdout).text(),
		new Response(child.stderr).text(),
	]);
	if (code !== 0)
		throw new Error(diagnostics || `The native hook failed (${code})`);
	return output;
}

export function toolName(name: string): string {
	return name === "shell" ? "bash" : name === "patch" ? "apply_patch" : name;
}
