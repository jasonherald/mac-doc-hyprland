use gtk4::gdk;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::TryRecvError;

/// CSS priority: embedded defaults (base layer).
const CSS_PRIORITY_EMBEDDED: u32 = gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION;
/// CSS priority: programmatic overrides like --opacity (middle layer).
const CSS_PRIORITY_OVERRIDE: u32 = CSS_PRIORITY_EMBEDDED + 1;
/// CSS priority: user CSS file (highest — always wins, including after hot-reload).
const CSS_PRIORITY_USER: u32 = CSS_PRIORITY_EMBEDDED + 2;

/// Debounce interval for CSS file change detection (milliseconds).
const CSS_RELOAD_DEBOUNCE_MS: u64 = 100;

/// Loads a CSS file and applies it at the highest priority (user overrides).
/// Always returns a CssProvider — if the file doesn't exist yet, an empty
/// provider is installed so `watch_css` can hot-load it when created.
///
/// Priority order: embedded defaults < programmatic overrides < user CSS file.
/// This ensures user CSS always wins, including after hot-reload.
pub fn load_css(css_path: &Path) -> gtk4::CssProvider {
    let provider = gtk4::CssProvider::new();

    if css_path.exists() {
        provider.load_from_path(css_path);
        log::info!("Loaded CSS from {}", css_path.display());
    } else {
        log::info!("{} not found — watching for creation", css_path.display());
    }

    apply_provider(&provider, CSS_PRIORITY_USER);
    provider
}

/// Loads CSS from a string as embedded defaults (lowest priority).
///
/// User CSS file and programmatic overrides both take precedence.
pub fn load_css_from_data(css: &str) -> gtk4::CssProvider {
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(css);
    apply_provider(&provider, CSS_PRIORITY_EMBEDDED);
    provider
}

/// Loads CSS from a string as a programmatic override (middle priority).
///
/// Overrides embedded defaults, but user CSS file still wins.
pub fn load_css_override(css: &str) -> gtk4::CssProvider {
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(css);
    apply_provider(&provider, CSS_PRIORITY_OVERRIDE);
    provider
}

/// Watches a CSS file for changes and reloads the provider automatically.
/// Uses inotify (Linux) via the `notify` crate — no polling.
/// The watcher lives on the GLib main loop for the lifetime of the
/// owning application.
///
/// Also watches files referenced via `@import` directives in the main
/// CSS, so theme managers like `tinty` that update imported files
/// (rather than the main CSS directly) trigger hot-reload too
/// (issue #73). On every main-CSS reload, the `@import` set is
/// re-scanned and the underlying `notify` watcher is rebuilt if the
/// set of watched files actually changed (issue #74). Adding or
/// removing an `@import` line while the dock is running now picks
/// up the new target on the next save, without a restart.
pub fn watch_css(css_path: &Path, provider: &gtk4::CssProvider) {
    let path = css_path.to_path_buf();
    let Some(parent) = path.parent() else {
        log::debug!(
            "CSS watch skipped: no parent directory for {}",
            path.display()
        );
        return;
    };
    // Canonicalize the parent directory so path comparisons against
    // notify events work consistently. notify reports canonical paths
    // (dot/dotdot segments resolved, symlinks followed) — if we stored
    // the lexical form (e.g. `/tmp/./dir`) events would arrive as
    // `/tmp/dir` and `HashSet<PathBuf>::contains` would silently
    // miss them, breaking hot-reload for any relative import path.
    // Parent is canonicalized rather than the full path so the watch
    // still works when the main CSS file doesn't exist yet (the
    // "watch for creation" flow in `load_css`).
    let main_dir = match parent.canonicalize() {
        Ok(d) => d,
        Err(e) => {
            log::debug!(
                "CSS watch skipped: can't canonicalize parent dir of {}: {}",
                path.display(),
                e
            );
            return;
        }
    };
    let Some(file_name) = path.file_name() else {
        log::debug!("CSS watch skipped: no filename for {}", path.display());
        return;
    };
    let canonical_path = main_dir.join(file_name);

    let imports = discover_watched_imports(&canonical_path);
    if !imports.is_empty() {
        log::info!(
            "Watching {} CSS @import target{} for hot-reload",
            imports.len(),
            if imports.len() == 1 { "" } else { "s" }
        );
    }

    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let Some(initial) = build_watch_state(&canonical_path, &imports, tx.clone()) else {
        return;
    };
    install_reload_timer(canonical_path, provider.clone(), rx, tx, initial);
}

/// Everything required to keep the `notify` watcher alive and to know
/// which files are currently tracked, so we can diff against a
/// re-scanned set on each reload.
struct WatchState {
    /// Owns the notify worker thread — dropped means "stop watching".
    /// The leading underscore tells both the compiler and future
    /// readers that this field is intentionally never read: its entire
    /// purpose is RAII lifetime management. When a new `WatchState`
    /// replaces this one, dropping the old field stops its worker.
    _watcher: notify::RecommendedWatcher,
    /// Absolute paths we signal reloads for. Compared structurally
    /// to detect `@import` set changes across reloads.
    watched: HashSet<PathBuf>,
}

/// Builds a fresh `WatchState` for the given main CSS path plus the
/// current set of imported files. Subscribes the watcher to the
/// parent directory of the main CSS AND the parent directory of each
/// import (the same dir if they share a parent). Returns `None` if
/// the watcher itself can't be created — callers log-and-continue.
fn build_watch_state(
    main_css: &Path,
    imports: &[PathBuf],
    tx: std::sync::mpsc::Sender<()>,
) -> Option<WatchState> {
    use notify::{RecursiveMode, Watcher};

    let watched = compute_watched_set(main_css, imports);
    let dirs = compute_watched_dirs(main_css, imports);

    let mut watcher = match notify::recommended_watcher(make_css_handler(watched.clone(), tx)) {
        Ok(w) => w,
        Err(e) => {
            log::warn!("Failed to create CSS watcher: {}", e);
            return None;
        }
    };
    for dir in &dirs {
        if let Err(e) = watcher.watch(dir, RecursiveMode::NonRecursive) {
            log::warn!("Failed to watch CSS directory '{}': {}", dir.display(), e);
        }
    }
    Some(WatchState {
        _watcher: watcher,
        watched,
    })
}

/// Computes the full set of absolute paths we want to fire reloads for:
/// the main CSS and every currently-discovered `@import` target.
/// Pure; testable without notify or the filesystem.
fn compute_watched_set(main_css: &Path, imports: &[PathBuf]) -> HashSet<PathBuf> {
    let mut out = HashSet::with_capacity(imports.len() + 1);
    out.insert(main_css.to_path_buf());
    for imp in imports {
        out.insert(imp.clone());
    }
    out
}

/// Computes the set of parent directories that notify needs to subscribe
/// to in order to observe every watched file. One notify watch per
/// directory suffices — events are then matched against the absolute
/// path set built by `compute_watched_set`.
fn compute_watched_dirs(main_css: &Path, imports: &[PathBuf]) -> HashSet<PathBuf> {
    let mut dirs: HashSet<PathBuf> = HashSet::new();
    if let Some(parent) = main_css.parent() {
        dirs.insert(parent.to_path_buf());
    }
    for imp in imports {
        if let Some(parent) = imp.parent() {
            dirs.insert(parent.to_path_buf());
        }
    }
    dirs
}

/// Installs a debounced GLib timer that reloads the provider on file
/// change and rebuilds the underlying watcher if the `@import` set
/// has shifted since the last reload. The timer closure owns the
/// active `WatchState` so the watcher's worker thread stays alive for
/// the lifetime of the GLib main loop.
///
/// Rebuilding the watcher is opt-in: we construct the new state first
/// and only then drop the old one, which creates a brief overlap
/// where both watchers may fire for the same event. The debounce in
/// `drain_events` folds duplicates, so the extra event is harmless.
fn install_reload_timer(
    path: std::path::PathBuf,
    provider: gtk4::CssProvider,
    rx: std::sync::mpsc::Receiver<()>,
    tx: std::sync::mpsc::Sender<()>,
    initial: WatchState,
) {
    let mut state = initial;
    gtk4::glib::timeout_add_local(
        std::time::Duration::from_millis(CSS_RELOAD_DEBOUNCE_MS),
        move || match drain_events(&rx) {
            DrainResult::Changed => {
                reload_provider(&provider, &path);
                maybe_rebuild_watcher(&path, &tx, &mut state);
                gtk4::glib::ControlFlow::Continue
            }
            DrainResult::Empty => gtk4::glib::ControlFlow::Continue,
            DrainResult::Disconnected => {
                log::warn!("CSS watcher disconnected; stopping hot-reload");
                gtk4::glib::ControlFlow::Break
            }
        },
    );
}

/// Re-discovers the `@import` set from the main CSS and, if it differs
/// from what the current watcher is tracking, replaces the watcher.
/// No-op (and fast) in the common case where the user changed a file
/// we already watch without touching any `@import` lines.
fn maybe_rebuild_watcher(
    main_css: &Path,
    tx: &std::sync::mpsc::Sender<()>,
    state: &mut WatchState,
) {
    let new_imports = discover_watched_imports(main_css);
    let new_watched = compute_watched_set(main_css, &new_imports);
    if new_watched == state.watched {
        return;
    }
    log::info!(
        "CSS @import set changed ({} → {} tracked file{}); rebuilding watcher",
        state.watched.len(),
        new_watched.len(),
        if new_watched.len() == 1 { "" } else { "s" }
    );
    // Build the new state BEFORE dropping the old one so we don't have
    // a window where nothing is watching. The old `state.watcher` is
    // dropped at the assignment below, which stops its worker thread.
    if let Some(new_state) = build_watch_state(main_css, &new_imports, tx.clone()) {
        *state = new_state;
    } else {
        log::warn!("Failed to rebuild CSS watcher; keeping previous watch set");
    }
}

enum DrainResult {
    Changed,
    Empty,
    Disconnected,
}

/// Drains all pending events from the watcher channel.
fn drain_events(rx: &std::sync::mpsc::Receiver<()>) -> DrainResult {
    let mut changed = false;
    loop {
        match rx.try_recv() {
            Ok(()) => changed = true,
            Err(TryRecvError::Empty) => {
                return if changed {
                    DrainResult::Changed
                } else {
                    DrainResult::Empty
                };
            }
            Err(TryRecvError::Disconnected) => return DrainResult::Disconnected,
        }
    }
}

/// Reloads the CSS provider from the file, or clears it if the file is gone.
fn reload_provider(provider: &gtk4::CssProvider, path: &Path) {
    log::info!("CSS file changed, reloading: {}", path.display());
    if path.exists() {
        provider.load_from_path(path);
    } else {
        // File deleted — clear user styles so defaults show through
        provider.load_from_data("");
    }
}

/// Creates a notify event handler that sends on the channel when any of
/// the `watched` absolute paths is affected (by any save strategy,
/// including deletion). Each path should be the absolute path of a
/// CSS file we care about — the main stylesheet or an `@import`
/// target.
fn make_css_handler(
    watched: HashSet<PathBuf>,
    tx: std::sync::mpsc::Sender<()>,
) -> impl FnMut(Result<notify::Event, notify::Error>) {
    move |event| {
        let ev = match event {
            Ok(ev) => ev,
            Err(e) => {
                log::warn!("CSS watcher error: {}", e);
                return;
            }
        };
        if !is_content_change(&ev.kind) {
            return;
        }
        let matches = ev.paths.iter().any(|p| watched.contains(p));
        if matches && let Err(e) = tx.send(()) {
            log::warn!("CSS watcher channel closed: {}", e);
        }
    }
}

/// Filters notify event kinds to just the ones that indicate the file's
/// *content* changed (or the file was created/removed). Access events
/// (`Open`, `Close(Read)`, etc.) fire when *any* reader opens the
/// file — including our own `load_from_path` / `read_to_string` calls
/// in the reload path. Treating those as change signals creates an
/// infinite feedback loop: reload opens file → Access event fires →
/// reload opens file → … The `notify` crate's default inotify filter
/// includes Access events on some backends, so this kind-based guard
/// is required even though the path-set filter normally constrains
/// which files we react to.
fn is_content_change(kind: &notify::EventKind) -> bool {
    use notify::EventKind;
    matches!(
        kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    )
}

fn apply_provider(provider: &gtk4::CssProvider, priority: u32) {
    let Some(display) = gdk::Display::default() else {
        log::error!("Cannot apply CSS: GTK display not available — is GTK initialized?");
        return;
    };
    gtk4::style_context_add_provider_for_display(&display, provider, priority);
}

// ─── @import discovery (issue #73) ────────────────────────────────────────
//
// The CSS watcher fires on a stat change of the main stylesheet, but
// `@import`-referenced files live wherever the user wants them — theme
// managers like tinty keep color-scheme CSS under `~/.local/share/...`
// and reference it from `~/.config/.../style.css`. Without this, the
// user changes their color scheme, the imported file changes on disk,
// and the dock looks stale until the user manually touches their main
// CSS file. Parsing `@import` directives lets us watch the target files
// too, at the cost of a tiny CSS mini-parser below.
//
// The parser is lenient: anything it can't recognize is silently
// skipped, which means we might miss an exotic `@import` form (we don't
// hot-reload the target) but we never crash or corrupt user CSS. Real
// CSS evaluation is still done by GTK via `load_from_path`; we only
// peek at the file to find out what else to watch.

/// Reads the main CSS file and returns the absolute paths of every
/// `@import` target that currently exists on disk. Safe against read
/// failure (returns empty) — the caller still watches the main path,
/// so the user can create or repair the file to recover.
fn discover_watched_imports(main_css: &Path) -> Vec<PathBuf> {
    let Some(base_dir) = main_css.parent() else {
        return Vec::new();
    };
    let content = match std::fs::read_to_string(main_css) {
        Ok(c) => c,
        Err(e) => {
            log::debug!(
                "CSS @import discovery: can't read {} ({}); continuing without imports",
                main_css.display(),
                e
            );
            return Vec::new();
        }
    };
    let mut out = Vec::new();
    for raw in parse_css_imports(&content) {
        let Some(resolved) = resolve_import_path(&raw, base_dir) else {
            continue;
        };
        // Canonicalize for the same reason as `watch_css` does for the
        // main path — notify events arrive with canonical paths, so the
        // comparison set must store canonical paths to match. This also
        // doubles as the existence check (canonicalize errors if the
        // target is missing), replacing the earlier `exists()` guard.
        match resolved.canonicalize() {
            Ok(canonical) => out.push(canonical),
            Err(e) => {
                log::debug!(
                    "CSS @import target not accessible ({}): {}",
                    e,
                    resolved.display()
                );
            }
        }
    }
    out
}

/// Extracts the raw path string from every `@import` directive in the
/// supplied CSS source. Strips `/* ... */` comments first so commented-
/// out imports don't count. Malformed directives are skipped silently
/// (see module-level rationale).
fn parse_css_imports(css: &str) -> Vec<String> {
    let stripped = strip_css_comments(css);
    let mut imports = Vec::new();
    let mut rest = stripped.as_str();

    while let Some(pos) = rest.find("@import") {
        // Advance past this @import whether or not we can parse its
        // argument — otherwise a single malformed directive would
        // loop us forever.
        let after_kw = &rest[pos + "@import".len()..];
        rest = after_kw.trim_start();
        if let Some((path, after)) = take_import_path(rest) {
            if !path.trim().is_empty() {
                imports.push(path);
            }
            rest = after;
        }
    }

    imports
}

/// Parses the path portion of an `@import` directive, returning the
/// extracted path and the text that follows it. Recognized forms:
///
///   `"path"` · `'path'` · `url("path")` · `url('path')` · `url(path)`
///
/// Returns `None` if the input doesn't start with a recognized form.
fn take_import_path(s: &str) -> Option<(String, &str)> {
    // Helper: reject captured paths that contain a raw newline. CSS
    // string literals don't legally span lines unescaped, so a "quoted"
    // path with a newline in it almost always means the user forgot
    // the closing quote — and our lenient scan would otherwise
    // happily swallow text clear up to the NEXT unrelated `"` or `'`,
    // potentially eating the next `@import` directive wholesale. The
    // check keeps the parser from pathologically consuming good
    // imports because of a typo in an earlier one.
    fn finalize_quoted<'a>(path: &'a str, after: &'a str) -> Option<(String, &'a str)> {
        if path.contains('\n') {
            return None;
        }
        Some((path.to_string(), after))
    }

    if let Some(rest) = s.strip_prefix('"') {
        let end = rest.find('"')?;
        return finalize_quoted(&rest[..end], &rest[end + 1..]);
    }
    if let Some(rest) = s.strip_prefix('\'') {
        let end = rest.find('\'')?;
        return finalize_quoted(&rest[..end], &rest[end + 1..]);
    }
    if let Some(rest) = s.strip_prefix("url") {
        let rest = rest.trim_start().strip_prefix('(')?.trim_start();
        if let Some(inner) = rest.strip_prefix('"') {
            let end = inner.find('"')?;
            let after = inner[end + 1..].trim_start();
            let after = after.strip_prefix(')').unwrap_or(after);
            return finalize_quoted(&inner[..end], after);
        }
        if let Some(inner) = rest.strip_prefix('\'') {
            let end = inner.find('\'')?;
            let after = inner[end + 1..].trim_start();
            let after = after.strip_prefix(')').unwrap_or(after);
            return finalize_quoted(&inner[..end], after);
        }
        let end = rest.find(')')?;
        return Some((rest[..end].trim().to_string(), &rest[end + 1..]));
    }
    None
}

/// Removes `/* ... */` blocks from CSS source. Unterminated comments
/// consume the rest of the text (matches browser behavior). Leaves
/// everything else — including strings that happen to contain `/*` —
/// untouched. String-quoting awareness isn't required here because
/// the downstream `@import` parser only matches the directive
/// *outside* strings, and comment stripping only removes genuine
/// comment blocks.
fn strip_css_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        rest = &rest[start + 2..];
        match rest.find("*/") {
            Some(end) => rest = &rest[end + 2..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Converts a raw `@import` path string into an absolute filesystem
/// path, relative paths resolved against `base_dir`. Returns `None`
/// for URLs we can't watch on the local filesystem (`http://`,
/// `https://`, `data:`, `file://` — the last is a valid file URL but
/// we'd need to strip the scheme and this hasn't surfaced as a
/// real-world need yet). Empty or whitespace-only input also returns
/// `None`.
fn resolve_import_path(raw: &str, base_dir: &Path) -> Option<PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if is_unwatchable_url(trimmed) {
        return None;
    }
    let p = Path::new(trimmed);
    Some(if p.is_absolute() {
        p.to_path_buf()
    } else {
        base_dir.join(p)
    })
}

/// URL schemes we intentionally don't try to treat as filesystem paths.
fn is_unwatchable_url(s: &str) -> bool {
    s.starts_with("http://")
        || s.starts_with("https://")
        || s.starts_with("data:")
        || s.starts_with("file://")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── strip_css_comments ──────────────────────────────────────────────

    #[test]
    fn strip_comments_none() {
        assert_eq!(
            strip_css_comments("window { color: red; }"),
            "window { color: red; }"
        );
    }

    #[test]
    fn strip_single_comment() {
        assert_eq!(strip_css_comments("a /* b */ c"), "a  c");
    }

    #[test]
    fn strip_multiple_comments() {
        assert_eq!(strip_css_comments("/* one */ middle /* two */"), " middle ");
    }

    #[test]
    fn strip_comment_containing_import_directive() {
        // The whole point: a commented-out @import must not be matched later.
        assert!(!strip_css_comments("/* @import \"fake.css\"; */ real").contains("@import"));
    }

    #[test]
    fn strip_unterminated_comment_consumes_rest() {
        assert_eq!(strip_css_comments("before /* oops"), "before ");
    }

    #[test]
    fn strip_empty_input() {
        assert_eq!(strip_css_comments(""), "");
    }

    #[test]
    fn strip_adjacent_comments() {
        assert_eq!(strip_css_comments("/*a*//*b*/c"), "c");
    }

    // ─── take_import_path ────────────────────────────────────────────────

    #[test]
    fn take_double_quoted_path() {
        let (p, rest) = take_import_path("\"theme.css\"; window { }").unwrap();
        assert_eq!(p, "theme.css");
        assert_eq!(rest, "; window { }");
    }

    #[test]
    fn take_single_quoted_path() {
        let (p, rest) = take_import_path("'theme.css'").unwrap();
        assert_eq!(p, "theme.css");
        assert_eq!(rest, "");
    }

    #[test]
    fn take_url_double_quoted() {
        let (p, rest) = take_import_path("url(\"theme.css\") screen;").unwrap();
        assert_eq!(p, "theme.css");
        // The trailing media query / semicolon is left to the caller.
        assert!(rest.contains("screen"));
    }

    #[test]
    fn take_url_single_quoted() {
        let (p, _) = take_import_path("url('theme.css')").unwrap();
        assert_eq!(p, "theme.css");
    }

    #[test]
    fn take_url_unquoted() {
        let (p, _) = take_import_path("url(theme.css)").unwrap();
        assert_eq!(p, "theme.css");
    }

    #[test]
    fn take_url_with_inner_whitespace() {
        let (p, _) = take_import_path("url(  theme.css  )").unwrap();
        assert_eq!(p, "theme.css");
    }

    #[test]
    fn take_unterminated_quote_returns_none() {
        assert!(take_import_path("\"unterminated").is_none());
    }

    #[test]
    fn take_non_import_returns_none() {
        assert!(take_import_path("window { color: red; }").is_none());
    }

    #[test]
    fn take_empty_returns_none() {
        assert!(take_import_path("").is_none());
    }

    // ─── parse_css_imports ───────────────────────────────────────────────

    #[test]
    fn parse_no_imports() {
        assert!(parse_css_imports("window { color: red; }").is_empty());
    }

    #[test]
    fn parse_single_double_quoted() {
        assert_eq!(
            parse_css_imports("@import \"theme.css\";\nwindow { }"),
            vec!["theme.css"]
        );
    }

    #[test]
    fn parse_single_single_quoted() {
        assert_eq!(parse_css_imports("@import 'theme.css';"), vec!["theme.css"]);
    }

    #[test]
    fn parse_url_forms() {
        assert_eq!(
            parse_css_imports("@import url(\"a.css\"); @import url('b.css'); @import url(c.css);"),
            vec!["a.css", "b.css", "c.css"]
        );
    }

    #[test]
    fn parse_multiple_mixed_imports() {
        let css = r#"
            @import "one.css";
            @import 'two.css';
            @import url("three.css");
            window { color: red; }
        "#;
        assert_eq!(
            parse_css_imports(css),
            vec!["one.css", "two.css", "three.css"]
        );
    }

    #[test]
    fn parse_ignores_commented_imports() {
        let css = r#"
            /* @import "fake.css"; */
            @import "real.css";
        "#;
        assert_eq!(parse_css_imports(css), vec!["real.css"]);
    }

    #[test]
    fn parse_import_with_media_query_suffix() {
        // CSS permits a media query after @import — we keep the path, drop
        // the media query. GTK doesn't honor media queries anyway.
        let css = r#"@import "print.css" print;"#;
        assert_eq!(parse_css_imports(css), vec!["print.css"]);
    }

    #[test]
    fn parse_malformed_continues_past() {
        // A broken @import shouldn't prevent finding later good ones.
        let css = r#"
            @import "unterminated
            @import "good.css";
        "#;
        let imports = parse_css_imports(css);
        assert!(imports.contains(&"good.css".to_string()));
    }

    #[test]
    fn parse_path_with_spaces_in_quotes() {
        // Users with spaces in their paths should still work via quoting.
        assert_eq!(
            parse_css_imports("@import \"my themes/base16.css\";"),
            vec!["my themes/base16.css"]
        );
    }

    #[test]
    fn parse_empty_string_paths_skipped() {
        assert!(parse_css_imports("@import \"\"; @import ' ';").is_empty());
    }

    #[test]
    fn parse_real_world_tinty_example() {
        // BlueInGreen68's reported stylesheet (issue #73).
        let css = r#"
            /* Color scheme */
            @import "/home/blueingreen68/.local/share/tinted-theming/tinty/base16-nwg-dock-themes-file.css";

            window {
              border-width: 3px;
              border-style: solid;
            }
        "#;
        assert_eq!(
            parse_css_imports(css),
            vec![
                "/home/blueingreen68/.local/share/tinted-theming/tinty/base16-nwg-dock-themes-file.css"
            ]
        );
    }

    // ─── resolve_import_path ─────────────────────────────────────────────

    #[test]
    fn resolve_absolute_path_unchanged() {
        let base = Path::new("/home/user/.config/nwg-dock-hyprland");
        assert_eq!(
            resolve_import_path("/abs/path/theme.css", base).unwrap(),
            PathBuf::from("/abs/path/theme.css")
        );
    }

    #[test]
    fn resolve_relative_path_against_base() {
        let base = Path::new("/home/user/.config/nwg-dock-hyprland");
        assert_eq!(
            resolve_import_path("theme.css", base).unwrap(),
            PathBuf::from("/home/user/.config/nwg-dock-hyprland/theme.css")
        );
    }

    #[test]
    fn resolve_nested_relative_path() {
        let base = Path::new("/home/user/.config/nwg-dock-hyprland");
        assert_eq!(
            resolve_import_path("themes/dark.css", base).unwrap(),
            PathBuf::from("/home/user/.config/nwg-dock-hyprland/themes/dark.css")
        );
    }

    #[test]
    fn resolve_http_is_none() {
        assert!(resolve_import_path("http://example.com/style.css", Path::new("/tmp")).is_none());
    }

    #[test]
    fn resolve_https_is_none() {
        assert!(resolve_import_path("https://example.com/style.css", Path::new("/tmp")).is_none());
    }

    #[test]
    fn resolve_data_url_is_none() {
        assert!(resolve_import_path("data:text/css,body{}", Path::new("/tmp")).is_none());
    }

    #[test]
    fn resolve_file_url_is_none() {
        // Could be supported later by stripping the scheme; today we skip.
        assert!(resolve_import_path("file:///etc/passwd", Path::new("/tmp")).is_none());
    }

    #[test]
    fn resolve_empty_is_none() {
        assert!(resolve_import_path("", Path::new("/tmp")).is_none());
    }

    #[test]
    fn resolve_whitespace_only_is_none() {
        assert!(resolve_import_path("   \t\n", Path::new("/tmp")).is_none());
    }

    // ─── discover_watched_imports (I/O; uses tempdir) ─────────────────────
    //
    // Each test carves a uniquely-named subdirectory under the OS temp
    // dir so parallel `cargo test` runs don't collide. `create_dir_all`
    // and `remove_dir_all` are wrapped with `.expect(...)` so filesystem
    // setup or cleanup errors fail loudly rather than quietly polluting
    // subsequent runs — per CodeRabbit review on #75 and the project
    // coding guideline against silent `let _ =` discards.

    /// Builds a fresh temp subdirectory for one of the I/O tests below.
    /// The directory name includes the test name and process id so a
    /// concurrent test can't trample it.
    fn make_test_dir(test_name: &str) -> std::path::PathBuf {
        let tmp =
            std::env::temp_dir().join(format!("nwg-css-test-{}-{}", test_name, std::process::id()));
        // Start clean in case a prior test run crashed before cleanup.
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp)
            .unwrap_or_else(|e| panic!("create test dir {}: {}", tmp.display(), e));
        tmp
    }

    fn cleanup_test_dir(dir: &Path) {
        std::fs::remove_dir_all(dir)
            .unwrap_or_else(|e| panic!("remove test dir {}: {}", dir.display(), e));
    }

    #[test]
    fn discover_no_file_returns_empty() {
        let p = Path::new("/nonexistent/path/style.css");
        assert!(discover_watched_imports(p).is_empty());
    }

    #[test]
    fn discover_file_without_imports_returns_empty() {
        let tmp = make_test_dir("no-imports");
        let css = tmp.join("style.css");
        std::fs::write(&css, "window { color: red; }").expect("write style.css");
        assert!(discover_watched_imports(&css).is_empty());
        cleanup_test_dir(&tmp);
    }

    #[test]
    fn discover_relative_import_resolved_and_existing() {
        let tmp = make_test_dir("rel-import");
        let css = tmp.join("style.css");
        let import = tmp.join("theme.css");
        std::fs::write(&import, "").expect("write theme.css");
        std::fs::write(&css, "@import \"theme.css\";").expect("write style.css");
        let found = discover_watched_imports(&css);
        // `discover_watched_imports` canonicalizes — compare against the
        // canonical form of the import path so symlink-under-/tmp setups
        // (e.g. macOS /tmp → /private/tmp) still match.
        let expected = import.canonicalize().expect("canonicalize import");
        assert_eq!(found, vec![expected]);
        cleanup_test_dir(&tmp);
    }

    /// Regression for the CodeRabbit catch on #75: a relative import
    /// containing `.` segments used to be stored lexically (e.g.
    /// `/dir/./theme.css`) but notify events always use the canonical
    /// form (`/dir/theme.css`), so the `HashSet::contains` match
    /// silently failed and hot-reload never fired. Canonicalizing both
    /// the watched set entry and the (implicit) event path fixes it;
    /// this test pins the canonical form by construction.
    #[test]
    fn discover_dot_segment_import_canonicalized() {
        let tmp = make_test_dir("dot-segment");
        let css = tmp.join("style.css");
        let import = tmp.join("theme.css");
        std::fs::write(&import, "").expect("write theme.css");
        std::fs::write(&css, "@import \"./theme.css\";").expect("write style.css");
        let found = discover_watched_imports(&css);
        let expected = import.canonicalize().expect("canonicalize import");
        assert_eq!(found, vec![expected]);
        // Ensure no stray `.` segment survived into the stored path.
        assert!(
            !found[0].components().any(|c| matches!(
                c,
                std::path::Component::CurDir | std::path::Component::ParentDir
            )),
            "stored path should not contain `.` or `..` segments: {}",
            found[0].display()
        );
        cleanup_test_dir(&tmp);
    }

    #[test]
    fn discover_skips_nonexistent_imports() {
        let tmp = make_test_dir("missing-import");
        let css = tmp.join("style.css");
        std::fs::write(&css, "@import \"missing-theme.css\";").expect("write style.css");
        assert!(discover_watched_imports(&css).is_empty());
        cleanup_test_dir(&tmp);
    }

    #[test]
    fn discover_skips_http_imports() {
        let tmp = make_test_dir("http-import");
        let css = tmp.join("style.css");
        std::fs::write(&css, "@import \"https://example.com/theme.css\";")
            .expect("write style.css");
        assert!(discover_watched_imports(&css).is_empty());
        cleanup_test_dir(&tmp);
    }

    // ─── is_content_change (feedback-loop guard) ─────────────────────────
    //
    // Regression test for the loop that showed up during #74 smoke
    // testing: GTK's `load_from_path` and our own `read_to_string`
    // both fire `Access(Open)` inotify events on the CSS file they
    // read, which used to match the watched set and trigger a reload,
    // which opened the file again. `is_content_change` narrows the
    // handler to create/modify/remove kinds so self-reads don't
    // re-enter the reload path.

    #[test]
    fn is_content_change_accepts_create_modify_remove() {
        use notify::EventKind;
        use notify::event::{CreateKind, ModifyKind, RemoveKind};
        assert!(is_content_change(&EventKind::Create(CreateKind::File)));
        assert!(is_content_change(&EventKind::Modify(ModifyKind::Data(
            notify::event::DataChange::Any
        ))));
        assert!(is_content_change(&EventKind::Remove(RemoveKind::File)));
    }

    #[test]
    fn is_content_change_rejects_access_events() {
        use notify::EventKind;
        use notify::event::{AccessKind, AccessMode};
        // These are the kinds our own reload cycle generates when we
        // open the CSS file to reload it. They must NOT count as
        // content changes, otherwise we self-trigger a reload loop.
        assert!(!is_content_change(&EventKind::Access(AccessKind::Open(
            AccessMode::Any
        ))));
        assert!(!is_content_change(&EventKind::Access(AccessKind::Close(
            AccessMode::Read
        ))));
        assert!(!is_content_change(&EventKind::Access(AccessKind::Read)));
    }

    #[test]
    fn is_content_change_rejects_any_and_other() {
        use notify::EventKind;
        assert!(!is_content_change(&EventKind::Any));
        assert!(!is_content_change(&EventKind::Other));
    }

    // ─── make_css_handler (end-to-end event routing) ─────────────────────
    //
    // Exercises the full handler contract — content-change kind check
    // AND watched-path match AND channel send — by feeding synthetic
    // `notify::Event` values into the closure and reading from the
    // receiver. This is the layer where we missed the feedback-loop
    // bug during #74 smoke testing; the tests below pin down every
    // combination that should / shouldn't send.

    fn modify_event(path: &Path) -> Result<notify::Event, notify::Error> {
        use notify::event::{DataChange, ModifyKind};
        use notify::{Event, EventKind};
        Ok(
            Event::new(EventKind::Modify(ModifyKind::Data(DataChange::Any)))
                .add_path(path.to_path_buf()),
        )
    }

    fn access_event(path: &Path) -> Result<notify::Event, notify::Error> {
        use notify::event::{AccessKind, AccessMode};
        use notify::{Event, EventKind};
        Ok(
            Event::new(EventKind::Access(AccessKind::Open(AccessMode::Any)))
                .add_path(path.to_path_buf()),
        )
    }

    fn create_event(path: &Path) -> Result<notify::Event, notify::Error> {
        use notify::event::CreateKind;
        use notify::{Event, EventKind};
        Ok(Event::new(EventKind::Create(CreateKind::File)).add_path(path.to_path_buf()))
    }

    fn remove_event(path: &Path) -> Result<notify::Event, notify::Error> {
        use notify::event::RemoveKind;
        use notify::{Event, EventKind};
        Ok(Event::new(EventKind::Remove(RemoveKind::File)).add_path(path.to_path_buf()))
    }

    #[test]
    fn handler_sends_on_modify_to_watched_path() {
        let watched_path = PathBuf::from("/tmp/style.css");
        let mut watched = HashSet::new();
        watched.insert(watched_path.clone());
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let mut handler = make_css_handler(watched, tx);
        handler(modify_event(&watched_path));
        assert!(rx.try_recv().is_ok(), "Modify on watched path must send");
    }

    #[test]
    fn handler_sends_on_create_and_remove_of_watched_path() {
        let watched_path = PathBuf::from("/tmp/style.css");
        let mut watched = HashSet::new();
        watched.insert(watched_path.clone());
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let mut handler = make_css_handler(watched, tx);
        handler(create_event(&watched_path));
        handler(remove_event(&watched_path));
        // Two events → two sends (debounce happens downstream in
        // `drain_events`, not in the handler).
        assert!(rx.try_recv().is_ok());
        assert!(rx.try_recv().is_ok());
    }

    /// Regression for the #74 smoke-test bug: Access events on watched
    /// paths were firing reloads, which re-opened the file via
    /// `load_from_path`, which generated more Access events, which
    /// triggered more reloads. The handler must drop Access events
    /// on the floor even when the path matches.
    #[test]
    fn handler_ignores_access_events_on_watched_path() {
        let watched_path = PathBuf::from("/tmp/style.css");
        let mut watched = HashSet::new();
        watched.insert(watched_path.clone());
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let mut handler = make_css_handler(watched, tx);
        // Fire a bunch of Access events — none should reach the channel.
        for _ in 0..5 {
            handler(access_event(&watched_path));
        }
        assert!(
            rx.try_recv().is_err(),
            "Access events must not send — they're our own reload's self-feedback"
        );
    }

    #[test]
    fn handler_ignores_modify_on_unwatched_path() {
        let watched_path = PathBuf::from("/tmp/style.css");
        let unrelated = PathBuf::from("/tmp/gdk-pixbuf-glycin-tmp.XYZ");
        let mut watched = HashSet::new();
        watched.insert(watched_path);
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let mut handler = make_css_handler(watched, tx);
        // Glycin constantly churns temp files in /tmp; those must not
        // trigger reloads even though their parent dir is watched.
        handler(modify_event(&unrelated));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn handler_sends_when_any_event_path_matches() {
        // Some notify events carry multiple paths (e.g. rename). If any
        // one matches the watched set, the event still counts.
        use notify::event::{DataChange, ModifyKind};
        use notify::{Event, EventKind};
        let watched_path = PathBuf::from("/tmp/style.css");
        let unrelated = PathBuf::from("/tmp/unrelated.tmp");
        let mut watched = HashSet::new();
        watched.insert(watched_path.clone());
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let mut handler = make_css_handler(watched, tx);
        let ev = Event::new(EventKind::Modify(ModifyKind::Data(DataChange::Any)))
            .add_path(unrelated)
            .add_path(watched_path);
        handler(Ok(ev));
        assert!(rx.try_recv().is_ok());
    }

    #[test]
    fn handler_does_not_panic_on_error_event() {
        let mut watched = HashSet::new();
        watched.insert(PathBuf::from("/tmp/style.css"));
        let (tx, _rx) = std::sync::mpsc::channel::<()>();
        let mut handler = make_css_handler(watched, tx);
        // `notify::Error` isn't easy to construct directly; use the
        // generic io-error path. This proves the handler's match arm
        // for `Err` is reachable and doesn't panic.
        let err = notify::Error::io(std::io::Error::other("synthetic test error"));
        handler(Err(err));
        // No assertion on channel — just prove the call returned cleanly.
    }

    // ─── compute_watched_set / compute_watched_dirs (issue #74) ────────────
    //
    // Pure helpers used by `maybe_rebuild_watcher` to diff old-vs-new
    // `@import` sets across reloads. Tested without notify or GTK so we
    // can assert the equality semantics that decide whether to rebuild.

    #[test]
    fn watched_set_contains_main_css_when_no_imports() {
        let main = PathBuf::from("/home/user/.config/dock/style.css");
        let set = compute_watched_set(&main, &[]);
        assert_eq!(set.len(), 1);
        assert!(set.contains(&main));
    }

    #[test]
    fn watched_set_contains_main_and_all_imports() {
        let main = PathBuf::from("/home/user/.config/dock/style.css");
        let imports = vec![
            PathBuf::from("/home/user/.local/share/theme/base16.css"),
            PathBuf::from("/home/user/.config/dock/extras.css"),
        ];
        let set = compute_watched_set(&main, &imports);
        assert_eq!(set.len(), 3);
        assert!(set.contains(&main));
        for imp in &imports {
            assert!(set.contains(imp));
        }
    }

    /// Regression for the #74 rebuild decision: the equality check
    /// between old and new sets must treat "same imports" as "no
    /// rebuild needed", even if the order in which imports were
    /// passed to `compute_watched_set` differs.
    #[test]
    fn watched_set_equality_is_order_independent() {
        let main = PathBuf::from("/style.css");
        let a = PathBuf::from("/a.css");
        let b = PathBuf::from("/b.css");
        let set1 = compute_watched_set(&main, &[a.clone(), b.clone()]);
        let set2 = compute_watched_set(&main, &[b.clone(), a.clone()]);
        assert_eq!(set1, set2);
    }

    #[test]
    fn watched_set_differs_when_import_added_or_removed() {
        let main = PathBuf::from("/style.css");
        let a = PathBuf::from("/a.css");
        let b = PathBuf::from("/b.css");
        let before = compute_watched_set(&main, std::slice::from_ref(&a));
        let after_added = compute_watched_set(&main, &[a.clone(), b.clone()]);
        let after_removed = compute_watched_set(&main, &[]);
        assert_ne!(before, after_added);
        assert_ne!(before, after_removed);
        assert_ne!(after_added, after_removed);
    }

    #[test]
    fn watched_dirs_collapses_shared_parent() {
        // Two imports under the same directory should produce one
        // notify watch, not two — notify subscribes to a dir, not a
        // file, and double-watching the same dir wastes file handles.
        let main = PathBuf::from("/home/user/style.css");
        let imports = vec![
            PathBuf::from("/home/user/a.css"),
            PathBuf::from("/home/user/b.css"),
        ];
        let dirs = compute_watched_dirs(&main, &imports);
        assert_eq!(dirs.len(), 1);
        assert!(dirs.contains(Path::new("/home/user")));
    }

    #[test]
    fn watched_dirs_includes_all_distinct_parents() {
        let main = PathBuf::from("/home/user/.config/dock/style.css");
        let imports = vec![
            PathBuf::from("/home/user/.local/share/theme/base16.css"),
            PathBuf::from("/home/user/.cache/dock/colors.css"),
        ];
        let dirs = compute_watched_dirs(&main, &imports);
        assert_eq!(dirs.len(), 3);
        assert!(dirs.contains(Path::new("/home/user/.config/dock")));
        assert!(dirs.contains(Path::new("/home/user/.local/share/theme")));
        assert!(dirs.contains(Path::new("/home/user/.cache/dock")));
    }

    /// End-to-end regression for #74: the sequence of user actions
    /// (save main CSS with one set of imports, then save with a
    /// different set) must produce different watched sets so
    /// `maybe_rebuild_watcher` triggers a rebuild.
    #[test]
    fn discover_tracks_changing_import_set_across_rewrites() {
        let tmp = make_test_dir("dynamic-rescan");
        let css = tmp.join("style.css");
        let theme_a = tmp.join("theme-a.css");
        let theme_b = tmp.join("theme-b.css");
        std::fs::write(&theme_a, "").expect("write theme-a.css");
        std::fs::write(&theme_b, "").expect("write theme-b.css");

        // Initial state: imports theme-a.
        std::fs::write(&css, "@import \"theme-a.css\";").expect("write style.css");
        let set_a = compute_watched_set(&css, &discover_watched_imports(&css));

        // User edits main CSS to import theme-b instead.
        std::fs::write(&css, "@import \"theme-b.css\";").expect("rewrite style.css");
        let set_b = compute_watched_set(&css, &discover_watched_imports(&css));

        // User edits main CSS to import both.
        std::fs::write(&css, "@import \"theme-a.css\"; @import \"theme-b.css\";")
            .expect("rewrite style.css");
        let set_both = compute_watched_set(&css, &discover_watched_imports(&css));

        // User edits main CSS to drop all imports.
        std::fs::write(&css, "window { color: red; }").expect("rewrite style.css");
        let set_none = compute_watched_set(&css, &discover_watched_imports(&css));

        // Every transition must surface as a set change so the
        // rebuild guard fires.
        assert_ne!(set_a, set_b);
        assert_ne!(set_a, set_both);
        assert_ne!(set_a, set_none);
        assert_ne!(set_b, set_both);
        assert_ne!(set_b, set_none);
        assert_ne!(set_both, set_none);

        cleanup_test_dir(&tmp);
    }
}
