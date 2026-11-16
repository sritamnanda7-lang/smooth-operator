// SPDX-License-Identifier: FSL-1.1-MIT
// Step 6 stub: runs one page job at a time (yt_play, docs_open, cal_open) and answers with a result code.
export function makeTasks(_env) {
  return {
    run: async (_cmd) => {
      throw new Error("step 6 stub");
    },
    cancel: (_id) => {
      throw new Error("step 6 stub");
    },
    cancelAll: () => {
      throw new Error("step 6 stub");
    },
  };
}
