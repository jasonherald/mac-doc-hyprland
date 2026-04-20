# Post-copy checklist

Things that don't live in a copy-pasted file — manual GitHub / SonarQube setup each per-tool repo needs after the file-level scaffolding from `docs/security-template/` lands.

Run top to bottom for each of `nwg-common`, `nwg-dock`, `nwg-drawer`, `nwg-notifications`.

## 1. Substitute placeholders in the copied files

```bash
find . -type f -not -path "./target/*" -exec sed -i \
  -e 's|${REPO_OWNER}|jasonherald|g' \
  -e 's|${REPO_NAME}|<this-repo-name>|g' {} +

# Confirm no placeholders remain
grep -rE '\$\{REPO_(OWNER|NAME)\}' . --exclude-dir=target --exclude-dir=.git
```

- `SECURITY.md` — replace `${REPO_OWNER}` + `${REPO_NAME}`.
- `sonar-project.properties` — replace `${REPO_NAME}` in `sonar.projectKey` and `sonar.projectName`.

## 2. Customize `SECURITY.md` Scope section

The template's Scope section lists behaviors across all four tools. Prune to what this specific repo does:

- `nwg-common` — library only; point at the consuming binaries' SECURITY.md for behavioral scope.
- `nwg-dock` / `nwg-drawer` — `.desktop` Exec execution + pin-file I/O + compositor IPC.
- `nwg-notifications` — D-Bus server + compositor IPC for focus signals.

## 3. Prune `.coderabbit.yaml` path_instructions

The template copies every path instruction from the monorepo. Each per-tool repo should delete the ones that don't match files in the target tree:

- `nwg-common` — keep `crates/nwg-common/**`, drop the binary-specific `crates/nwg-dock/...`, `crates/nwg-drawer/...`, `crates/nwg-notifications/...` entries.
- `nwg-dock` — keep `crates/nwg-dock/**` (in the new repo this path collapses to `src/**`; rewrite the glob), drop the other crate-specific entries.
- Same pattern for `nwg-drawer` and `nwg-notifications`.
- All repos keep: `.github/workflows/*.y*ml`, `.github/actions/**/action.y*ml`, `Makefile`, `**/CHANGELOG.md`, `**/Cargo.toml`, and the general-rules block.

## 4. SonarQube project (server-side)

**Required before the first `make sonar` run.**

1. Log into [sonar.aaru.network](https://sonar.aaru.network).
2. Create a new project with `Project key = <this-repo-name>` (match `sonar.projectKey` in `sonar-project.properties`).
3. Generate a project analysis token.
4. Drop the token into the repo's `.env` as `SONAR_TOKEN=...` (gitignored — see `.gitignore`).
5. Regenerate the truststore if using the self-signed cert (see user memory / existing scripts).
6. Run `make sonar` locally to confirm the scanner connects and uploads results.

## 5. CodeRabbit (GitHub App)

CodeRabbit is installed at the GitHub App level but must be enabled **per-repo** in its app settings.

1. Go to <https://app.coderabbit.ai/> → **Repositories**.
2. Toggle the new repo on.
3. First PR in the repo confirms the `.coderabbit.yaml` is being read — look for the summary comment.

## 6. Collaborator access

Add `@nwg-piotr` as a collaborator with the same access level they have on the monorepo:

```bash
gh api -X PUT \
  /repos/jasonherald/<this-repo-name>/collaborators/nwg-piotr \
  -f permission=write
```

Do this **before** the first substantive PR in the repo — @nwg-piotr is part of the review rotation, and waiting until after a few PRs are merged means they miss the context.

## 7. Branch protection on `main`

```bash
gh api -X PUT \
  /repos/jasonherald/<this-repo-name>/branches/main/protection \
  --input - <<'JSON'
{
  "required_pull_request_reviews": {
    "required_approving_review_count": 0,
    "require_code_owner_reviews": false,
    "dismiss_stale_reviews": true
  },
  "required_status_checks": {
    "strict": true,
    "contexts": [
      "Rustfmt / cargo fmt --check",
      "Clippy / cargo clippy -D warnings",
      "Test / cargo test --workspace",
      "Cargo Deny / License & Supply Chain Check",
      "Security Audit / Dependency Audit",
      "CodeQL / CodeQL Analysis"
    ]
  },
  "required_conversation_resolution": true,
  "allow_force_pushes": false,
  "allow_deletions": false,
  "enforce_admins": false,
  "restrictions": null
}
JSON
```

Notes:
- Status-check contexts above are the `name:` values from the workflow YAMLs. If a workflow name changes, the context must be updated here.
- Required checks only appear as selectable in the UI **after** each workflow has run at least once on a PR or push — push an initial no-op PR to trigger first-run before calling this API.
- `required_approving_review_count: 0` because solo maintenance; bump to `1` when a co-maintainer lands.

## 8. `.env` seeding

Each new repo needs its own `.env` for local SonarQube scans:

```bash
# from the new repo's root
cat > .env <<'ENV'
SONAR_TOKEN=<token from step 4>
SONAR_HOST_URL=https://sonar.aaru.network
ENV

# verify gitignored
git check-ignore .env
```

## 9. First CI run

Open a trivial PR (typo fix, README touch) to confirm:

- [ ] All six workflows run and pass (fmt / clippy / test / deny / audit / codeql).
- [ ] CodeRabbit posts a review summary.
- [ ] The PR cannot be merged without the status checks passing (branch-protection working).

## 10. Final

- [ ] Update the root-repo README's migration banner table to link to the new repo and its crates.io page (#88 tracks the banner; Phase 7 flips the monorepo to archive notice).
