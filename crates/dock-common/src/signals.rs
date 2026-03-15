use nix::sys::signal::{self, Signal};
use std::sync::mpsc;

/// SIGRTMIN value on Linux (glibc = 34).
const SIGRTMIN: i32 = 34;

/// Signal numbers used by dock/drawer for control.
pub fn sig_toggle() -> i32 {
    SIGRTMIN + 1
}

pub fn sig_show() -> i32 {
    SIGRTMIN + 2
}

pub fn sig_hide() -> i32 {
    SIGRTMIN + 3
}

/// Signal numbers used by the notification daemon.
pub fn sig_notification_toggle() -> i32 {
    SIGRTMIN + 4
}

pub fn sig_notification_dnd() -> i32 {
    SIGRTMIN + 5
}

/// Window visibility commands sent via signal handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowCommand {
    Show,
    Hide,
    Toggle,
    Quit,
}

/// Sets up signal handlers and returns a receiver for window commands.
///
/// Handles SIGTERM via sigaction, and SIGUSR1 + SIGRTMIN+1/2/3 via
/// raw libc sigwait (nix's Signal enum doesn't support real-time signals).
pub fn setup_signal_handlers(is_resident: bool) -> mpsc::Receiver<WindowCommand> {
    let (tx, rx) = mpsc::channel();

    // SIGTERM → quit
    // SAFETY: sigaction requires unsafe. The handler calls process::exit.
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

    // Block SIGUSR1 and SIGRTMIN+1/2/3 in the main thread BEFORE spawning.
    // Uses raw libc because nix's Signal enum doesn't support RT signals.
    let rt_signals = [sig_toggle(), sig_show(), sig_hide()];
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGUSR1);
        for &sig in &rt_signals {
            libc::sigaddset(&mut set, sig);
        }
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
    }

    // Sigwait thread — inherits the blocked signal mask
    std::thread::spawn(move || {
        loop {
            let mut sig: i32 = 0;
            // SAFETY: sigwait blocks until a signal from the set is pending.
            let mut set: libc::sigset_t = unsafe { std::mem::zeroed() };
            unsafe {
                libc::sigemptyset(&mut set);
                libc::sigaddset(&mut set, libc::SIGUSR1);
                for &s in &rt_signals {
                    libc::sigaddset(&mut set, s);
                }
                libc::sigwait(&set, &mut sig);
            }

            let cmd = if sig == libc::SIGUSR1 {
                log::warn!("SIGUSR1 for toggling is deprecated, use SIGRTMIN+1");
                if is_resident {
                    Some(WindowCommand::Toggle)
                } else {
                    None
                }
            } else if sig == sig_toggle() {
                if is_resident {
                    Some(WindowCommand::Toggle)
                } else {
                    None
                }
            } else if sig == sig_show() {
                if is_resident {
                    Some(WindowCommand::Show)
                } else {
                    None
                }
            } else if sig == sig_hide() {
                if is_resident {
                    Some(WindowCommand::Hide)
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(cmd) = cmd
                && tx.send(cmd).is_err()
            {
                break;
            }
        }
    });

    rx
}

/// Sends a signal to a running instance by PID.
pub fn send_signal_to_pid(pid: u32, sig_num: i32) -> bool {
    // Use raw libc for RT signals since nix doesn't support them
    unsafe { libc::kill(pid as i32, sig_num) == 0 }
}

extern "C" fn sigterm_handler(_: i32) {
    log::info!("SIGTERM received, bye bye!");
    std::process::exit(0);
}
