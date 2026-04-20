# mac-doc-hyprland (archived)

> [!NOTE]
> **This repository has been archived.** The code was split into four per-tool repositories at v0.3.0. This repo preserves the pre-split git history and closed-issue record; all new issues, PRs, and releases happen in the new repos.

| Tool | Repo | crates.io |
|------|------|-----------|
| `nwg-common` (shared library) | [jasonherald/nwg-common](https://github.com/jasonherald/nwg-common) | [`nwg-common`](https://crates.io/crates/nwg-common) |
| `nwg-dock` (renamed from `nwg-dock-hyprland` — supports Hyprland + Sway) | [jasonherald/nwg-dock](https://github.com/jasonherald/nwg-dock) | [`nwg-dock`](https://crates.io/crates/nwg-dock) |
| `nwg-drawer` | [jasonherald/nwg-drawer](https://github.com/jasonherald/nwg-drawer) | [`nwg-drawer`](https://crates.io/crates/nwg-drawer) |
| `nwg-notifications` | [jasonherald/nwg-notifications](https://github.com/jasonherald/nwg-notifications) | [`nwg-notifications`](https://crates.io/crates/nwg-notifications) |

## Migrating from `nwg-dock-hyprland`?

The Rust port's dock is now called `nwg-dock` and supports both Hyprland and Sway in one binary. Existing `exec-once = nwg-dock-hyprland …` autostart lines keep working: `make install` in the [nwg-dock](https://github.com/jasonherald/nwg-dock) repo installs a `nwg-dock-hyprland` symlink pointing at the renamed binary, so no compositor config edits are needed.

## History

- Git log for everything up to v0.2.0 lives here.
- v0.3.0 is where the split happened — each per-tool repo's CHANGELOG starts at v0.3.0 with a pointer back to this repo for earlier history.
- See the original monorepo commit log for the porting work from [nwg-piotr](https://github.com/nwg-piotr)'s Go implementations.

## License

MIT. Each per-tool repo has its own `LICENSE` file with the same terms.
