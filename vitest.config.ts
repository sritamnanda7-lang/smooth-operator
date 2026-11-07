// SPDX-License-Identifier: FSL-1.1-MIT
import { defineConfig } from "vitest/config";

export default defineConfig({
  test: { include: ["tests/so/**/*.test.ts"], environment: "node" },
});
