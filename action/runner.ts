// Commands that GitHub and Forgejo runners understand.

import { randomUUID } from "node:crypto";
import { appendFileSync } from "node:fs";

/** Reads an action input, or an empty string when it isn't set. */
export function input(
  name: string,
  env: NodeJS.ProcessEnv = process.env,
): string {
  return env[`INPUT_${name.replaceAll(" ", "_").toUpperCase()}`]?.trim() ?? "";
}

/** Reads an input that must be set. */
export function requiredInput(
  name: string,
  env: NodeJS.ProcessEnv = process.env,
): string {
  const value = input(name, env);
  if (value === "") {
    throw new Error(`the input ${name} is required`);
  }
  return value;
}

/** Reads a boolean input. */
export function booleanInput(
  name: string,
  env: NodeJS.ProcessEnv = process.env,
): boolean {
  return ["true", "yes", "1"].includes(input(name, env).toLowerCase());
}

/** Appends `name=value` to the runner file named by `fileVariable`. */
export function writeFileCommand(
  fileVariable: "GITHUB_ENV" | "GITHUB_OUTPUT" | "GITHUB_STATE",
  name: string,
  value: string,
  env: NodeJS.ProcessEnv = process.env,
): void {
  const file = env[fileVariable];
  if (file === undefined || file === "") {
    throw new Error(`the runner didn't set ${fileVariable}`);
  }
  const delimiter = `granary_${randomUUID()}`;
  appendFileSync(file, `${name}<<${delimiter}\n${value}\n${delimiter}\n`);
}

/** Sets an environment variable for the following steps. */
export function exportVariable(name: string, value: string): void {
  writeFileCommand("GITHUB_ENV", name, value);
}

/** Sets a step output. */
export function setOutput(name: string, value: string): void {
  writeFileCommand("GITHUB_OUTPUT", name, value);
}

/** Keeps a value for the post step. */
export function saveState(name: string, value: string): void {
  writeFileCommand("GITHUB_STATE", name, value);
}

/** Reads a value that the main step kept. */
export function state(
  name: string,
  env: NodeJS.ProcessEnv = process.env,
): string {
  return env[`STATE_${name}`] ?? "";
}

/** Hides a value in the job log. */
export function setSecret(value: string): void {
  process.stdout.write(`::add-mask::${value}\n`);
}

/** Logs a line. */
export function info(message: string): void {
  process.stdout.write(`${message}\n`);
}

/** Starts a collapsed group of log lines. */
export function startGroup(name: string): void {
  process.stdout.write(`::group::${escape(name)}\n`);
}

/** Ends the group of log lines. */
export function endGroup(): void {
  process.stdout.write("::endgroup::\n");
}

/** Logs a warning that the job summary shows. */
export function warning(message: string): void {
  process.stdout.write(`::warning::${escape(message)}\n`);
}

/** Logs an error and makes the step fail. */
export function fail(message: string): void {
  process.stdout.write(`::error::${escape(message)}\n`);
  process.exitCode = 1;
}

/** Escapes a message for a workflow command. */
function escape(message: string): string {
  return message
    .replaceAll("%", "%25")
    .replaceAll("\r", "%0D")
    .replaceAll("\n", "%0A");
}
