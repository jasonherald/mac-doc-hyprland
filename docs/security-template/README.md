# `docs/security-template/` — copy-paste-ready per-repo scaffolding

This directory contains the security + CI + review artifacts that every per-tool repo (`nwg-common` / `nwg-dock` / `nwg-drawer` / `nwg-notifications`) needs on day one. The Phase 1–4 extractions in epic #80 each copy this tree into the new repo, substitute placeholders, and commit.

## What's here

| File | Action on copy |
|------|----------------|
| `SECURITY.md` | **Substitute** `${REPO_OWNER}` + `${REPO_NAME}`. **Review** the Scope section — the default covers all four tools; prune to what the target repo actually does (see comment at the top of the file). |
| `sonar-project.properties` | **Substitute** `${REPO_NAME}` in `sonar.projectKey` and `sonar.projectName`. Create the SonarQube project server-side before the first scan. |
| `.coderabbit.yaml` | Copy verbatim. **Prune** the `path_instructions:` entries that reference files not present in the target repo (e.g. the compositor-specific instructions only belong in `nwg-common`; the UI-specific ones only in the matching binary repo). |
| `deny.toml` | Copy verbatim. Binary repos can likely adopt as-is; the library repo may drop some `[bans.skip]` entries once the transitive dep graph shrinks. |
| `rust-toolchain.toml` | Copy verbatim. Pins the Rust channel + `rustfmt` / `clippy` components so CI and local `make lint` both pick up the right toolchain automatically. |
| `.gitignore` | Copy verbatim. Standard Rust ignores + `.env` + truststore + editor cruft. `Cargo.lock` is explicitly NOT ignored. |
| `.github/workflows/*.yml` | Copy verbatim (audit / codeql / deny / fmt / clippy / test). Each new repo gets the full CI set — no backfilling later. |
| `.github/actions/setup-gtk4/action.yml` | Copy verbatim **into binary repos that link GTK4** (dock, drawer, notifications). The library repo (`nwg-common`) doesn't need it since the library itself is compiler-checked without the GTK4 system libs on clippy/test runners. |

After the file-level copy, follow [`CHECKLIST.md`](./CHECKLIST.md) for the manual GitHub-side steps (SonarQube project, CodeRabbit app enable, collaborator, branch protection, `.env` token).

## Why verbatim

Copying verbatim keeps the four repos drift-free. When a security control changes (new workflow, tightened deny.toml rule, updated rabbit config), the monorepo's copy here is the source of truth; a follow-up PR propagates the change to each per-tool repo by running `cp -r docs/security-template/… ../other-repo/…` from the monorepo checkout.

This is explicitly **not** a cargo-generate template or a scripted generator — it's a directory tree plus a substitution checklist. Two reasons:

1. **Inspection before commit.** A human sees every line that lands in each new repo; there's no hidden codegen.
2. **Low tooling surface.** No generator install, no template-engine knowledge required. `cp` + `sed -i 's/${REPO_NAME}/nwg-dock/g'` is enough.

## Dry-run substitution

Before the first phase extraction, run through the substitution on a throwaway target to confirm the template produces a working config:

```bash
TARGET=/tmp/security-template-dryrun
rm -rf "$TARGET" && mkdir -p "$TARGET"
cp -r docs/security-template/. "$TARGET/"
# strip template-only files
rm "$TARGET/README.md" "$TARGET/CHECKLIST.md"
# substitute placeholders
find "$TARGET" -type f -exec sed -i \
  -e 's|${REPO_OWNER}|jasonherald|g' \
  -e 's|${REPO_NAME}|nwg-common|g' {} +
# what ran
grep -rE '\$\{REPO_(OWNER|NAME)\}' "$TARGET" && echo "FAIL: leftover placeholders" || echo "OK: no placeholders left"
```

If the final `grep` prints "OK", the substitution is complete.
