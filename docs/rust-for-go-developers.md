# Rust for Go Developers

A guide to reading and contributing to this codebase, written for someone who knows Go well but is newer to Rust. All examples are from the actual code in this project.

## Quick orientation

| Go | Rust | Where you'll see it |
|----|------|-------------------|
| `package main` | `fn main()` in a `[[bin]]` crate | `nwg-dock/src/main.rs` |
| `package lib` | `lib.rs` in a library crate | `nwg-dock-common/src/lib.rs` |
| `go.mod` workspace | `Cargo.toml` workspace | Root `Cargo.toml` |
| `go build` | `cargo build` | |
| `go test ./...` | `cargo test --workspace` | |
| `golangci-lint` | `cargo clippy` | |

## Error handling

Go uses `if err != nil` everywhere. Rust uses the `Result<T, E>` type and the `?` operator.

**Go pattern:**
```go
monitors, err := hyprctl("j/monitors")
if err != nil {
    log.Error("Error listing monitors:", err)
    return
}
```

**Rust equivalent** (from `nwg-dock-common/src/compositor/hyprland.rs`):
```rust
fn list_monitors(&self) -> Result<Vec<WmMonitor>> {
    Ok(ipc::list_monitors()?.into_iter().map(to_wm_monitor).collect())
}
```

The `?` after `list_monitors()` does exactly what `if err != nil { return err }` does — it returns the error early. The difference is that `?` is an operator, not a pattern you write out each time.

When we want to log and continue instead of returning:
```rust
// nwg-dock/src/main.rs
if let Err(e) = state.borrow_mut().refresh_clients() {
    log::error!("Couldn't list clients: {}", e);
}
```

This is equivalent to Go's `if err != nil { log.Error(...) }` without returning.

## Interfaces → Traits

Go interfaces are implicit (duck typing). Rust traits are explicit (you declare `impl Trait for Type`).

**Go:**
```go
type Compositor interface {
    ListClients() ([]Client, error)
    FocusWindow(id string) error
}
```

**Rust** (from `nwg-dock-common/src/compositor/traits.rs`):
```rust
pub trait Compositor {
    fn list_clients(&self) -> Result<Vec<WmClient>>;
    fn focus_window(&self, id: &str) -> Result<()>;
    // ...
}
```

The implementation (from `compositor/hyprland.rs`):
```rust
impl Compositor for HyprlandBackend {
    fn list_clients(&self) -> Result<Vec<WmClient>> {
        Ok(ipc::list_clients()?.into_iter().map(to_wm_client).collect())
    }
    // ...
}
```

**Using it as a dynamic type** (like Go's `interface{}`):
```rust
// Box<dyn Compositor> is like Go's interface value
let compositor: Box<dyn Compositor> = Box::new(HyprlandBackend::new()?);

// Rc<dyn Compositor> is the same but reference-counted (shared ownership)
let compositor: Rc<dyn Compositor> = Rc::from(compositor);
```

## Shared state: pointers vs Rc\<RefCell\<T\>\>

In Go, you pass pointers freely. Multiple goroutines can hold `*State` and read/write through it.

Rust's borrow checker prevents this at compile time — you can't have multiple mutable references. The solution for GTK callbacks (which need shared mutable state) is `Rc<RefCell<T>>`.

**Go:**
```go
type DockState struct {
    clients []Client
    pinned  []string
}

func refresh(state *DockState) {
    state.clients = listClients()
}
```

**Rust** (from `nwg-dock/src/state.rs`):
```rust
pub struct DockState {
    pub clients: Vec<WmClient>,
    pub pinned: Vec<String>,
    // ...
}
```

Creating and sharing it:
```rust
// Rc = reference-counted pointer (like shared_ptr in C++)
// RefCell = runtime borrow checking (since GTK callbacks prevent compile-time checking)
let state = Rc::new(RefCell::new(DockState::new(app_dirs, compositor)));

// Clone the Rc (cheap — just increments a counter, like Go's pointer copy)
let state_for_callback = Rc::clone(&state);

// Borrow it mutably inside a callback
button.connect_clicked(move |_| {
    let mut s = state_for_callback.borrow_mut();  // Like Go's mutex lock
    s.pinned.push("firefox".to_string());
});
```

**Mental model:** `Rc::clone()` is Go's pointer copy. `.borrow()` is `RLock()`. `.borrow_mut()` is `Lock()`. But instead of deadlocking, it panics if you borrow wrong — so you keep borrows short.

## Goroutines → threads + channels

Go has goroutines with channels. Rust has OS threads with `mpsc` channels.

**Go:**
```go
ch := make(chan string)
go func() {
    for {
        event := readEvent()
        ch <- event.Address
    }
}()
```

**Rust** (from `nwg-dock/src/events.rs`):
```rust
let (sender, receiver) = mpsc::channel::<String>();

std::thread::spawn(move || {
    loop {
        match stream.next_event() {
            Ok(WmEvent::ActiveWindowChanged(id)) => {
                if sender.send(id).is_err() {
                    break;
                }
            }
            // ...
        }
    }
});
```

Key difference: Rust's `move ||` closure *takes ownership* of `sender` and `stream`. They're moved into the thread. The original variables become unusable. Go doesn't have this — goroutines just capture by reference.

**Polling the channel on the GTK main thread** (from `events.rs`):
```rust
glib::timeout_add_local(Duration::from_millis(100), move || {
    while let Ok(win_addr) = receiver.try_recv() {
        // Process events...
    }
    glib::ControlFlow::Continue
});
```

This is equivalent to Go's `select` with a timer, but integrated into the GTK event loop.

## Enums instead of string constants

Where Go uses string constants, Rust uses enums with compile-time checking.

**Go:**
```go
var position = flag.String("position", "bottom", "Position: bottom, top, left, right")
// Later: if *position == "bottom" { ... }
```

**Rust** (from `nwg-dock/src/config.rs`):
```rust
#[derive(ValueEnum)]
pub enum Position {
    Bottom,
    Top,
    Left,
    Right,
}

// Later: match config.position { Position::Bottom => ..., ... }
```

The Rust compiler ensures you handle all cases. If you add a new variant, every `match` that doesn't handle it becomes a compile error. In Go, you'd miss the new string value silently.

## CLI parsing: flag → clap

Go uses `flag.String()`, `flag.Bool()`, etc. Rust uses the `clap` crate with derive macros.

**Go:**
```go
var iconSize = flag.Int("i", 48, "Icon size")
var autohide = flag.Bool("d", false, "Auto-hide mode")
```

**Rust** (from `nwg-dock/src/config.rs`):
```rust
#[derive(Parser)]
pub struct DockConfig {
    #[arg(short = 'i', long, default_value_t = 48)]
    pub icon_size: i32,

    #[arg(short = 'd', long)]
    pub autohide: bool,
}
```

Usage is the same: `config.icon_size` instead of `*iconSize`.

## String types: &str vs String

Go has one string type. Rust has two: `String` (owned, heap-allocated, like Go's `string`) and `&str` (borrowed reference, a view into a string).

**Rule of thumb:**
- Function parameters: use `&str` (accepts both `String` and `&str`)
- Struct fields and return values: use `String` (owned data)
- Converting: `&str` → `String` with `.to_string()`, `String` → `&str` with `&s` or `s.as_str()`

```rust
// Takes a reference — doesn't need to own the string
fn is_pinned(pinned: &[String], task_id: &str) -> bool {
    pinned.iter().any(|p| p.trim() == task_id.trim())
}

// Stores owned strings — the struct owns its data
pub struct WmClient {
    pub id: String,
    pub class: String,
}
```

## Option\<T\> instead of nil

Go uses `nil` for "no value." Rust uses `Option<T>` — it's either `Some(value)` or `None`.

**Go:**
```go
func getActiveWindow() (*Client, error) {
    // might return nil
}

if client != nil {
    focus(client.Address)
}
```

**Rust:**
```rust
fn get_active_window(&self) -> Result<WmClient> { ... }

// Option from a lookup:
if let Some(mon) = focused_gdk_monitor(&compositor) {
    win.set_monitor(Some(&mon));
}
```

`if let Some(x) = expr` is the Rust equivalent of `if x != nil`.

## The module system

Go uses packages and directory structure. Rust uses modules declared explicitly.

**Go:** A file in `hyprland/` package is automatically part of that package.

**Rust:** You must declare modules in the parent:
```rust
// nwg-dock-common/src/lib.rs
pub mod compositor;   // → loads compositor/mod.rs
pub mod desktop;      // → loads desktop/mod.rs
pub mod hyprland;     // → loads hyprland/mod.rs
pub mod launch;       // → loads launch.rs
```

Inside `compositor/mod.rs`:
```rust
mod hyprland;         // private — only used internally
mod sway;             // private
pub mod traits;       // public — other crates can use it
pub mod types;        // public
```

## GTK4 callbacks and closures

GTK callbacks are the trickiest part for Go developers. In Go, you'd pass a function that captures variables by reference. In Rust, closures must `move` their captures.

**Go (GTK3):**
```go
button.Connect("clicked", func() {
    state.pinned = append(state.pinned, appID)
})
```

**Rust (GTK4)** (from `nwg-dock/src/ui/menus.rs`):
```rust
let comp = Rc::clone(compositor);  // Clone the Rc BEFORE the closure
let id = id.clone();               // Clone the String BEFORE the closure

btn.connect_clicked(move |_| {     // `move` takes ownership of clones
    let _ = comp.close_window(&id);
});
```

The pattern is always:
1. `Rc::clone()` or `.clone()` the data you need
2. `move ||` to transfer ownership into the closure
3. Use the cloned values inside

This is the #1 thing that feels different from Go. You'll see `Rc::clone()` + `move ||` hundreds of times in the UI code.

## Unsafe code

Go doesn't have `unsafe`. Rust requires it for raw pointer operations, FFI calls, etc.

In this codebase, `unsafe` appears only in `signals.rs` for raw libc signal handling (because Rust's `nix` crate doesn't support real-time signals like SIGRTMIN+N). Every unsafe block has a `// SAFETY:` comment explaining why it's safe:

```rust
// SAFETY: sigset_t is a plain C struct; zeroing + sigemptyset initializes it.
let mut set: libc::sigset_t = unsafe { std::mem::zeroed() };
unsafe {
    libc::sigemptyset(&mut set);
    libc::sigaddset(&mut set, libc::SIGUSR1);
}
```

## Build and test

```bash
cargo build                    # Debug build (fast compile, slow runtime)
cargo build --release          # Release build (slow compile, fast runtime)
cargo test --workspace         # Run all 68 tests
cargo clippy --all-targets     # Lint (like golangci-lint)
cargo fmt --all                # Format (like gofmt)
```

## Project structure

```
crates/
├── nwg-dock-common/       # Shared library (like a Go internal package)
│   └── src/
│       ├── compositor/    # Trait + Hyprland/Sway backends
│       ├── desktop/       # .desktop file parsing
│       ├── hyprland/      # Raw Hyprland IPC (used by compositor/hyprland.rs)
│       ├── config/        # XDG paths, CSS loading
│       ├── launch.rs      # Command execution
│       ├── signals.rs     # RT signal handling (unsafe)
│       ├── singleton.rs   # Lock file management
│       └── pinning.rs     # Pin file I/O
├── nwg-dock/              # Dock binary
│   └── src/
│       ├── main.rs        # Entry point, compositor init, GTK app setup
│       ├── config.rs      # CLI args (clap)
│       ├── state.rs       # DockState (clients, monitors, pins)
│       ├── context.rs     # DockContext (bundles shared refs for UI)
│       ├── events.rs      # Compositor event stream → rebuild
│       ├── monitor.rs     # Multi-monitor mapping
│       ├── rebuild.rs     # Self-referential rebuild via Weak
│       └── ui/            # GTK4 widgets, menus, drag-drop, hotspot
├── nwg-drawer/            # Drawer binary
│   └── src/
│       ├── main.rs        # Entry point
│       ├── config.rs      # CLI args
│       ├── state.rs       # DrawerState (apps, pins, compositor)
│       ├── desktop_loader.rs  # .desktop scanning + categories
│       ├── listeners.rs   # Keyboard, focus detector, file watcher
│       └── ui/            # Grid, search, power bar, math eval
└── nwg-notifications/     # Notification daemon
    └── src/
        ├── main.rs        # Entry point
        ├── dbus.rs        # org.freedesktop.Notifications server
        ├── state.rs       # NotificationState (history, DND)
        ├── persistence.rs # JSON history save/load
        ├── waybar.rs      # Status file + SIGRTMIN+11
        └── ui/            # Popups, panel, DND menu
```

## Where to start reading

1. **`nwg-dock-common/src/compositor/traits.rs`** — the core abstraction. Every compositor operation goes through this trait.
2. **`nwg-dock-common/src/compositor/hyprland.rs`** — familiar territory if you wrote the Go version. Maps 1:1 to hyprctl calls.
3. **`nwg-dock/src/main.rs`** — entry point. Short (~190 lines), shows the full initialization flow.
4. **`nwg-dock/src/ui/dock_box.rs`** — the main UI builder. Equivalent to Go's `refreshMainBox()`.
