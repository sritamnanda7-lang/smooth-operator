// SPDX-License-Identifier: FSL-1.1-MIT
// Stands in for desktop/src/bindings.ts in the step-4 UI tests (aliased by brainSection.test.ts). Every
// command is forwarded to `globalThis.__so.commands`, which each test sets up. No IPC, no network.
type Cmds = Record<string, (...a: unknown[]) => unknown>;
const table = (): Cmds => (globalThis as unknown as { __so: { commands: Cmds } }).__so.commands;

export const commands: Cmds = new Proxy({} as Cmds, {
  get: (_t, name: string) => (...args: unknown[]) => {
    const f = table()[name];
    if (!f) throw new Error(`command ${name} is not mocked`);
    return f(...args);
  },
});
export const events = {};
