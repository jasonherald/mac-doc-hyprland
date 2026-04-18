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
/// The watcher thread runs until the provider is dropped.
///
/// Also watches files referenced via `@import` directives in the main
/// CSS, so theme managers like `tinty` that update imported files
/// (rather than the main CSS directly) trigger hot-reload too
/// (issue #73). Imports are discovered at startup; adding or removing
/// an `@import` line mid-session currently requires a restart — that
/// improvement is tracked in #74.
pub fn watch_css(css_path: &Path, provider: &gtk4::CssProvider) {
    let path = css_path.to_path_buf();
    let Some(main_dir) = path.parent().map(Path::to_path_buf) else {
        log::debug!(
            "CSS watch skipped: no parent directory for {}",
            path.display()
        );
        return;
    };

    let imports = discover_watched_imports(&path);
    if !imports.is_empty() {
        log::info!(
            "Watching {} CSS @import target{} for hot-reload",
            imports.len(),
            if imports.len() == 1 { "" } else { "s" }
        );
    }

    let (tx, rx) = std::sync::mpsc::channel::<()>();
    spawn_watcher_thread(main_dir, path.clone(), imports, tx);
    install_reload_timer(path, provider.clone(), rx);
}

/// Spawns the notify watcher on a background thread. Watches the main
/// CSS file's parent directory plus the parent directory of every
/// imported file that exists on disk. Events are filtered to the set of
/// absolute paths we care about before signaling a reload.
///
/// The thread runs until the process exits — there's no cleanup path,
/// which is fine for a daemon.
fn spawn_watcher_thread(
    main_dir: PathBuf,
    main_css: PathBuf,
    imports: Vec<PathBuf>,
    tx: std::sync::mpsc::Sender<()>,
) {
    std::thread::spawn(move || {
        use notify::{RecursiveMode, Watcher};

        let mut dirs: HashSet<PathBuf> = HashSet::new();
        dirs.insert(main_dir);
        for imp in &imports {
            if let Some(parent) = imp.parent() {
                dirs.insert(parent.to_path_buf());
            }
        }

        let mut watched: HashSet<PathBuf> = HashSet::new();
        watched.insert(main_css);
        for imp in imports {
            watched.insert(imp);
        }

        let Ok(mut watcher) = notify::recommended_watcher(make_css_handler(watched, tx))
            .inspect_err(|e| log::warn!("Failed to create CSS watcher: {}", e))
        else {
            return;
        };
        for dir in &dirs {
            if let Err(e) = watcher.watch(dir, RecursiveMode::NonRecursive) {
                log::warn!("Failed to watch CSS directory '{}': {}", dir.display(), e);
            }
        }
        // Block forever — watcher stops if thread exits
        loop {
            std::thread::park();
        }
    });
}

/// Installs a debounced GLib timer that reloads the provider when
/// the watcher signals a file change. Stops the source on disconnect.
fn install_reload_timer(
    path: std::path::PathBuf,
    provider: gtk4::CssProvider,
    rx: std::sync::mpsc::Receiver<()>,
) {
    gtk4::glib::timeout_add_local(
        std::time::Duration::from_millis(CSS_RELOAD_DEBOUNCE_MS),
        move || match drain_events(&rx) {
            DrainResult::Changed => {
                reload_provider(&provider, &path);
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
        let matches = ev.paths.iter().any(|p| watched.contains(p));
        if matches && let Err(e) = tx.send(()) {
            log::warn!("CSS watcher channel closed: {}", e);
        }
    }
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
        if resolved.exists() {
            out.push(resolved);
        } else {
            log::debug!(
                "CSS @import target does not exist on disk: {}",
                resolved.display()
            );
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

    #[test]
    fn discover_no_file_returns_empty() {
        let p = Path::new("/nonexistent/path/style.css");
        assert!(discover_watched_imports(p).is_empty());
    }

    #[test]
    fn discover_file_without_imports_returns_empty() {
        let tmp = std::env::temp_dir().join(format!("nwg-css-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp);
        let css = tmp.join("style.css");
        std::fs::write(&css, "window { color: red; }").unwrap();
        assert!(discover_watched_imports(&css).is_empty());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn discover_relative_import_resolved_and_existing() {
        let tmp = std::env::temp_dir().join(format!("nwg-css-test-rel-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp);
        let css = tmp.join("style.css");
        let import = tmp.join("theme.css");
        std::fs::write(&import, "").unwrap();
        std::fs::write(&css, "@import \"theme.css\";").unwrap();
        let found = discover_watched_imports(&css);
        assert_eq!(found, vec![import.clone()]);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn discover_skips_nonexistent_imports() {
        let tmp = std::env::temp_dir().join(format!("nwg-css-test-missing-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp);
        let css = tmp.join("style.css");
        std::fs::write(&css, "@import \"missing-theme.css\";").unwrap();
        assert!(discover_watched_imports(&css).is_empty());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn discover_skips_http_imports() {
        let tmp = std::env::temp_dir().join(format!("nwg-css-test-http-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp);
        let css = tmp.join("style.css");
        std::fs::write(&css, "@import \"https://example.com/theme.css\";").unwrap();
        assert!(discover_watched_imports(&css).is_empty());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
