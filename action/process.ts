// Running other programs.

import { spawn } from "node:child_process";

/** Runs a program with the job's output and fails on a non-zero exit. */
export async function run(
  program: string,
  args: readonly string[],
  env: NodeJS.ProcessEnv = process.env,
): Promise<void> {
  await new Promise<void>((resolve, reject) => {
    const child = spawn(program, args, { env, stdio: "inherit" });
    child.on("error", reject);
    child.on("close", (code) => {
      if (code === 0) {
        resolve();
      } else {
        reject(new Error(`${program} exited with ${String(code)}`));
      }
    });
  });
}

/** Runs a program and returns what it printed. */
export async function output(
  program: string,
  args: readonly string[],
): Promise<string> {
  return await new Promise<string>((resolve, reject) => {
    const child = spawn(program, args, {
      stdio: ["ignore", "pipe", "inherit"],
    });
    const chunks: Buffer[] = [];
    child.stdout.on("data", (chunk: Buffer) => chunks.push(chunk));
    child.on("error", reject);
    child.on("close", (code) => {
      if (code === 0) {
        resolve(Buffer.concat(chunks).toString("utf8"));
      } else {
        reject(new Error(`${program} exited with ${String(code)}`));
      }
    });
  });
}
