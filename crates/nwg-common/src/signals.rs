//! Real-time signal handling for inter-process control.
//!
//! The dock, drawer, and notification daemon use `SIGRTMIN+N` signals for
//! user-driven toggle / show / hide actions. RT signals don't fit into
//! `nix::sys::signal::Signal`, so this module drops to raw libc for the
//! RT-specific parts.

use nix::sys::signal::{self, Signal};
use std::sync::mpsc;

/// Returns the runtime `SIGRTMIN` value.
///
/// Queried via `libc::SIGRTMIN()` rather than hardcoded to 34 because the
/// value differs across libc implementations: glibc reserves the first
/// two RT signals for NPTL (so userspace `SIGRTMIN` = 34), while musl
/// reserves three (so `SIGRTMIN` = 35).
fn sigrtmin() -> i32 {
    libc::SIGRTMIN()
}

/// Signal used to toggle the dock/drawer window (`SIGRTMIN+1`).
pub fn sig_toggle() -> i32 {
    sigrtmin() + 1
}

/// Signal used to show the dock/drawer window (`SIGRTMIN+2`).
pub fn sig_show() -> i32 {
    sigrtmin() + 2
}

/// Signal used to hide the dock/drawer window (`SIGRTMIN+3`).
pub fn sig_hide() -> i32 {
    sigrtmin() + 3
}

/// Signal used to toggle the notification panel (`SIGRTMIN+4`).
pub fn sig_notification_toggle() -> i32 {
    sigrtmin() + 4
}

/// Signal used to toggle Do-Not-Disturb (`SIGRTMIN+5`).
pub fn sig_notification_dnd() -> i32 {
    sigrtmin() + 5
}

/// Signal used to show the DND duration menu (`SIGRTMIN+6`).
pub fn sig_notification_dnd_menu() -> i32 {
    sigrtmin() + 6
}

/// Window visibility commands sent via signal handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowCommand {
    /// Show the window.
    Show,
    /// Hide the window.
    Hide,
    /// Toggle visibility; on non-resident programs this means "quit".
    Toggle,
    /// Quit the program.
    Quit,
}

/// Installs the SIGTERM handler (immediate termination via `_exit`).
/// Shared by all three binaries to avoid duplicating unsafe sigaction setup.
pub fn setup_sigterm_handler() {
    // SAFETY: sigaction requires unsafe. The handler performs only async-signal-safe termination.
    if let Err(e) = unsafe {
        signal::sigaction(
            Signal::SIGTERM,
            &signal::SigAction::new(
                signal::SigHandler::Handler(sigterm_handler),
                signal::SaFlags::SA_RESTART,
                signal::SigSet::empty(),
            ),
        )
    } {
        log::warn!("Failed to set SIGTERM handler: {}", e);
    }
}

/// Sets up signal handlers and returns a receiver for window commands.
///
/// Handles SIGTERM via sigaction, and SIGUSR1 + SIGRTMIN+1/2/3 via
/// raw libc sigwait (nix's Signal enum doesn't support real-time signals).
pub fn setup_signal_handlers(is_resident: bool) -> mpsc::Receiver<WindowCommand> {
    let (tx, rx) = mpsc::channel();

    setup_sigterm_handler();

    // Block SIGUSR1 and SIGRTMIN+1/2/3 in the main thread BEFORE spawning.
    // Uses raw libc because nix's Signal enum doesn't support RT signals.
    let rt_signals = [sig_toggle(), sig_show(), sig_hide()];
    // SAFETY: sigset_t is a POD struct, safe to zero-init. The subsequent
    // sigemptyset/sigaddset/pthread_sigmask calls all take that same stack
    // pointer; pthread_sigmask passes NULL for the old-mask out-param
    // because we discard it. Return codes are checked below — failures
    // only affect our process's signal mask and are non-fatal (the
    // sigwait thread would simply miss a signal the caller sent).
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        if libc::sigemptyset(&mut set) != 0 {
            log::error!("sigemptyset failed");
        }
        if libc::sigaddset(&mut set, libc::SIGUSR1) != 0 {
            log::error!("sigaddset(SIGUSR1) failed");
        }
        for &sig in &rt_signals {
            if libc::sigaddset(&mut set, sig) != 0 {
                log::error!("sigaddset({}) failed", sig);
            }
        }
        if libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut()) != 0 {
            log::error!("pthread_sigmask(SIG_BLOCK) failed");
        }
    }

    // Sigwait thread — inherits the blocked signal mask.
    // Build the signal set once before the loop for efficiency.
    std::thread::spawn(move || {
        // SAFETY: sigset_t is a POD struct, zero-init'd then populated via
        // sigemptyset/sigaddset on the same stack pointer. Returns are
        // checked below; a failure here means the thread's local set may
        // be missing a signal, which will surface as a missed signal —
        // logged, not aborted.
        let mut set: libc::sigset_t = unsafe { std::mem::zeroed() };
        unsafe {
            if libc::sigemptyset(&mut set) != 0 {
                log::error!("sigemptyset failed in sigwait thread");
            }
            if libc::sigaddset(&mut set, libc::SIGUSR1) != 0 {
                log::error!("sigaddset(SIGUSR1) failed in sigwait thread");
            }
            for &s in &rt_signals {
                if libc::sigaddset(&mut set, s) != 0 {
                    log::error!("sigaddset({}) failed in sigwait thread", s);
                }
            }
        }

        loop {
            let mut sig: i32 = 0;
            // SAFETY: sigwait blocks until a signal from `set` is pending;
            // `set` was populated above and lives for the duration of the
            // thread, so the pointer is valid. `sig` is a stack i32 the
            // kernel writes the fired signal number into.
            let ret = unsafe { libc::sigwait(&set, &mut sig) };
            if ret != 0 {
                log::error!("sigwait failed with error code {}", ret);
                break;
            }

            if let Some(cmd) = map_signal_to_command(sig, is_resident)
                && tx.send(cmd).is_err()
            {
                break;
            }
        }
    });

    rx
}

/// Maps a received signal number to a `WindowCommand`, if applicable.
fn map_signal_to_command(sig: i32, is_resident: bool) -> Option<WindowCommand> {
    if sig == libc::SIGUSR1 {
        log::warn!("SIGUSR1 for toggling is deprecated, use SIGRTMIN+1");
    }

    if sig == libc::SIGUSR1 || sig == sig_toggle() {
        // Non-resident: toggle means quit. Resident: toggle means show/hide.
        Some(WindowCommand::Toggle)
    } else if !is_resident {
        // Non-resident only responds to toggle — ignore show/hide signals
        None
    } else if sig == sig_show() {
        Some(WindowCommand::Show)
    } else if sig == sig_hide() {
        Some(WindowCommand::Hide)
    } else {
        None
    }
}

/// Sends a signal to a running instance by PID.
pub fn send_signal_to_pid(pid: u32, sig_num: i32) -> bool {
    // Raw libc is needed because nix's Signal enum doesn't cover RT signals.
    // SAFETY: libc::kill is a safe syscall wrapper — it never derefs caller
    // memory and returns -1 + errno for invalid pid/sig. The u32 → i32 cast
    // can't lose information in practice: kernel PIDs are bounded by
    // /proc/sys/kernel/pid_max (typically 4194304, well below i32::MAX ≈
    // 2.1e9). Callers are expected to pass a PID obtained from our own
    // singleton lock file, validated as a live instance of our binary.
    unsafe { libc::kill(pid as i32, sig_num) == 0 }
}

extern "C" fn sigterm_handler(_: i32) {
    // SAFETY: libc::_exit is async-signal-safe and terminates immediately.
    unsafe { libc::_exit(0) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rt_offsets_are_stable() {
        let base = sigrtmin();
        assert_eq!(sig_toggle(), base + 1);
        assert_eq!(sig_show(), base + 2);
        assert_eq!(sig_hide(), base + 3);
        assert_eq!(sig_notification_toggle(), base + 4);
        assert_eq!(sig_notification_dnd(), base + 5);
        assert_eq!(sig_notification_dnd_menu(), base + 6);
    }

    #[test]
    fn non_resident_only_accepts_toggle() {
        assert_eq!(
            map_signal_to_command(sig_toggle(), false),
            Some(WindowCommand::Toggle)
        );
        assert_eq!(map_signal_to_command(sig_show(), false), None);
        assert_eq!(map_signal_to_command(sig_hide(), false), None);
    }

    #[test]
    fn resident_maps_toggle_show_hide() {
        assert_eq!(
            map_signal_to_command(sig_toggle(), true),
            Some(WindowCommand::Toggle)
        );
        assert_eq!(
            map_signal_to_command(sig_show(), true),
            Some(WindowCommand::Show)
        );
        assert_eq!(
            map_signal_to_command(sig_hide(), true),
            Some(WindowCommand::Hide)
        );
    }

    #[test]
    fn sigusr1_maps_to_toggle_for_compat() {
        // SIGUSR1 is the pre-RT-signals toggle mechanism — still honored
        // (with a deprecation warning) in both resident and non-resident modes.
        assert_eq!(
            map_signal_to_command(libc::SIGUSR1, true),
            Some(WindowCommand::Toggle)
        );
        assert_eq!(
            map_signal_to_command(libc::SIGUSR1, false),
            Some(WindowCommand::Toggle)
        );
    }

    #[test]
    fn unknown_signal_returns_none() {
        assert_eq!(map_signal_to_command(libc::SIGTERM, true), None);
        assert_eq!(map_signal_to_command(libc::SIGTERM, false), None);
        // Higher RT signal we don't dispatch on
        assert_eq!(map_signal_to_command(sigrtmin() + 99, true), None);
    }

    #[test]
    fn send_signal_existence_check_on_self() {
        // `kill(pid, 0)` is the POSIX idiom for "does this PID exist and
        // can we signal it" without actually delivering a signal.
        // Our own PID always qualifies.
        let our_pid = std::process::id();
        assert!(send_signal_to_pid(our_pid, 0));
    }

    #[test]
    fn send_signal_to_invalid_pid_fails() {
        // PID beyond the kernel's pid_max (4194304) can't exist.
        let bogus_pid: u32 = 999_999_999;
        assert!(!send_signal_to_pid(bogus_pid, 0));
    }
}
