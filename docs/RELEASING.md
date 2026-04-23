# Releasing the Peat Workspace

This document describes how to cut a new release of the crates published from this workspace. It is meant to be repeatable: following the steps in order produces the same result each time.

## What gets published

Only two crates from this workspace go to crates.io:

| Crate | Role |
|-------|------|
| `peat-schema` | Wire format (Protobuf) definitions |
| `peat-protocol` | Public facade — re-exports `peat-schema` and `peat-mesh`; downstream consumers depend on this crate alone |

All other workspace members (`peat-transport`, `peat-persistence`, `peat-discovery`, `peat-ffi`, `examples/*`) share the workspace version but stay internal. They are not published.

`peat-ffi` publishes to **Maven Central** on its own cadence via `.github/workflows/publish-maven.yml`; it is decoupled from the crates.io release flow.

## Versioning

The workspace uses a single version (`[workspace.package].version`) for all published crates. The chosen version must track the `peat-mesh` release it pins against — that is the load-bearing dependency, and version drift between the two is a source of ecosystem confusion.

- **Minor / breaking** — increment when `peat-mesh` cuts a minor (e.g. `0.9.x`)
- **Patch** — internal fixes that keep the same `peat-mesh` pin
- **Pre-release** — use `-rc.N` suffix when soaking a release before promoting to stable (matches the `peat-mesh` release pattern)

Pre-1.0, minor bumps may contain breaking changes per Rust ecosystem convention.

## Pre-flight checklist

Before starting a release PR:

- [ ] `peat-mesh` is already on crates.io at the target version (or the corresponding `-rc.N`)
- [ ] `peat-sim`, `peat-registry`, `peat-node`, `peat-gateway`, `peat-atak-plugin` either already pin the target `peat-mesh` version or have open bump PRs
- [ ] No call sites of removed `peat-mesh` APIs (grep the workspace for the removed symbols listed in the `peat-mesh` CHANGELOG)
- [ ] Feature-tree sanity check passes for size-constrained builds:
      `cargo tree -e features -p peat-protocol --no-default-features --features lite-transport`
      should not pull `automerge`, `redb`, `iroh-blobs`, or `negentropy`

## Release PR

One branch, one PR. Make the release changes on a branch named `chore/release-<version>` (for example, `chore/release-0.9.0-rc.1` or `chore/release-0.9.0`).

1. **Bump the workspace version** in `/Cargo.toml`:
   ```toml
   [workspace.package]
   version = "0.9.0-rc.1"  # target version
   ```

2. **Version-ify path deps between published crates.** `peat-protocol` must reference `peat-schema` with an explicit version matching the bump:
   ```toml
   peat-schema = { path = "../peat-schema", version = "=0.9.0-rc.1" }
   ```
   Without this, `cargo publish` refuses to upload.

3. **Add the CHANGELOG entry.** Update `/CHANGELOG.md` with a `## [<version>] - YYYY-MM-DD` section. Follow Keep a Changelog conventions (`Added`, `Changed`, `Deprecated`, `Removed`, `Fixed`, `Security`). Include a `Pinned` section documenting the `peat-mesh` version and any default-feature decisions.

4. **Run the pre-flight validation:**

   ```bash
   cargo check --workspace --all-features
   cargo test --workspace --exclude peat-ffi --features automerge-backend
   (cd peat-schema && cargo publish --dry-run --allow-dirty)
   (cd peat-protocol && cargo publish --dry-run --allow-dirty)
   ```

   Note that `cargo publish --dry-run` on `peat-protocol` will fail with `no matching package named peat-schema found` until `peat-schema` is actually on crates.io at the new version. That is expected and is the reason publish order matters (see below). The packaging step passing (the error appears on the `Updating crates.io index` step afterward) is sufficient evidence that `peat-protocol` is release-ready.

5. **Open the PR**, let CI go green, get review, merge.

## Publish

**Publish order matters** because `peat-protocol` depends on `peat-schema` by version: `peat-schema` must be on crates.io before `peat-protocol` can be published.

From `main` at the merged release commit:

1. **Tag the release:**
   ```bash
   git tag v0.9.0-rc.1 <merge-commit>
   git push origin v0.9.0-rc.1
   ```

2. **Publish `peat-schema` first:**
   ```bash
   cd peat-schema
   cargo publish
   cd ..
   ```

3. **Wait for the crates.io index to propagate** (usually 30–60 seconds). Verify with:
   ```bash
   curl -s https://crates.io/api/v1/crates/peat-schema | \
     python3 -c "import sys,json;d=json.load(sys.stdin);print([v['num'] for v in d['versions'][:3]])"
   ```
   The target version should be in the list.

4. **Publish `peat-protocol`:**
   ```bash
   cd peat-protocol
   cargo publish
   cd ..
   ```

5. **Create the GitHub release** with the CHANGELOG entry as the body. Either:
   - via `gh release create v0.9.0-rc.1 --notes-file <(awk '/^## \[0.9.0-rc.1\]/{found=1;next} /^## \[/{if(found)exit} found{print}' CHANGELOG.md)`
   - or in the GitHub UI from the new tag.

## After publish

- [ ] Confirm both crates render correctly on crates.io (titles, descriptions, READMEs)
- [ ] Open bump PRs in downstream repos (`peat-sim`, `peat-atak-plugin`, any future SDK consumer) to pin the new `peat-protocol` version
- [ ] Watch for any missing field / metadata issues reported by docs.rs — fix in a follow-up patch release if needed

## RC-to-stable promotion

When a release candidate has soaked sufficiently and no regressions have surfaced:

1. New branch `chore/release-<stable-version>` from `main` (`main` already has the rc pin)
2. Bump `peat-mesh` workspace dep from `=<version>-rc.N` to the stable caret range (`"<major>.<minor>"`)
3. Bump workspace version from `<version>-rc.N` to `<version>`
4. Bump the `peat-schema` path-dep version in `peat-protocol/Cargo.toml` the same way
5. Update `CHANGELOG.md` with a new `## [<version>]` heading
6. Follow the Publish section above (tag, publish `peat-schema`, publish `peat-protocol`, GitHub release)

Only promote to stable after:
- All downstream repos have been on the rc for long enough to surface issues
- At least one round of real-world usage (not just CI) has confirmed no regressions
- There is no pending rc.N+1 in-flight

## Yanking

If a published version turns out to be broken:

```bash
cargo yank --version <bad-version> peat-protocol
cargo yank --version <bad-version> peat-schema   # if applicable
```

Yanking does **not** remove the version — it stops new projects from resolving to it while leaving existing Cargo.lock files intact. Publish a fixed version (patch or rc.N+1) and land a CHANGELOG entry explaining the yank.

## Troubleshooting

**`cargo publish` rejects the crate with "missing description":** the crate's `Cargo.toml` needs `description`, `license`, `repository` (and ideally `homepage`, `documentation`, `keywords`, `categories`). Inherit from workspace where possible (`license.workspace = true`) and set crate-specific fields directly.

**`cargo publish` rejects with "no matching package named <sibling>" for `peat-protocol`:** you tried to publish `peat-protocol` before `peat-schema` (or before the crates.io index propagated). Publish `peat-schema` first and wait ~60 seconds.

**Downstream build fails with "`peat-mesh` features mismatch":** confirm the workspace-level `peat-mesh` dep has `default-features = false` and each consumer's own feature flags opt in to the peat-mesh features they need. See `peat/#789` for the pattern.

**Publishing from a dirty working tree:** `cargo publish` refuses by default. Use `--allow-dirty` only for `--dry-run`. For the real publish, ensure the working tree matches the tagged commit.
