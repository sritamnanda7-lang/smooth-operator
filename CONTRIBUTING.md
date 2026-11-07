# Contributing

Smooth Operator is source-available under FSL-1.1-MIT (see LICENSE and NOTICE).

## Outside pull requests

- Pull requests into FSL-licensed paths (everything except Handy files under
  `desktop/`) are not accepted until a Contributor Licence Agreement (CLA) is
  in place. The CLA text will be published before the first outside PR is
  merged. Until then, open an issue instead of a PR.
- Small fixes to unmodified Handy files are better sent upstream to
  https://github.com/cjpais/Handy.

## Rules for every change

- Every file you add or modify carries an SPDX header: new files
  `SPDX-License-Identifier: FSL-1.1-MIT`; modified Handy files
  `SPDX-License-Identifier: MIT`. Files that cannot hold a comment are listed
  in `REUSE.toml`.
- Keep Handy changes thin. New code goes in our own directories
  (`desktop/src-tauri/src/so/`, `desktop/src/so/`). `scripts/fork-size.sh`
  must stay within 15 modified Handy files and under 1,500 added lines.
- Never commit secrets, keys (`*.key`, `*.p12`), or `.env` files.
- Changes to `.github/workflows` need a security review.

## Reporting a security issue

Use GitHub's private vulnerability reporting on this repository, not a public
issue.
