use nix::sys::signal::{self, Signal};
use std::sync::mpsc;

/// SIGRTMIN value on Linux (typically 34).
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
/// Handles SIGTERM, SIGUSR1 (deprecated toggle), and SIGRTMIN+1/2/3.
pub fn setup_signal_handlers(is_resident: bool) -> mpsc::Receiver<WindowCommand> {
    let (tx, rx) = mpsc::channel();

    // SIGTERM → quit
    // SAFETY: sigaction requires unsafe. The handler is a simple extern "C" fn
    // that calls process::exit — no shared state or complex logic.
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

    // Use a thread to handle signals via sigwait
    std::thread::spawn(move || {
        use nix::sys::signal::SigSet;

        let mut set = SigSet::empty();
        set.add(Signal::SIGUSR1);

        // Add SIGRTMIN+1/2/3
        for sig_num in [sig_toggle(), sig_show(), sig_hide()] {
            if let Ok(sig) = Signal::try_from(sig_num) {
                set.add(sig);
            }
        }

        // Block these signals so sigwait can catch them
        let _ = nix::sys::signal::sigprocmask(
            nix::sys::signal::SigmaskHow::SIG_BLOCK,
            Some(&set),
            None,
        );

        loop {
            match set.wait() {
                Ok(sig) => {
                    let sig_num = sig as i32;
                    let cmd = if sig == Signal::SIGUSR1 {
                        log::warn!("SIGUSR1 for toggling is deprecated, use SIGRTMIN+1");
                        if is_resident {
                            Some(WindowCommand::Toggle)
                        } else {
                            log::debug!("SIGUSR1 received but not resident, ignoring");
                            None
                        }
                    } else if sig_num == sig_toggle() {
                        if is_resident {
                            Some(WindowCommand::Toggle)
                        } else {
                            None
                        }
                    } else if sig_num == sig_show() {
                        if is_resident {
                            Some(WindowCommand::Show)
                        } else {
                            None
                        }
                    } else if sig_num == sig_hide() {
                        if is_resident {
                            Some(WindowCommand::Hide)
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    if let Some(cmd) = cmd
                        && tx.send(cmd).is_err() {
                            break;
                        }
                }
                Err(e) => {
                    log::error!("sigwait error: {}", e);
                    break;
                }
            }
        }
    });

    rx
}

/// Sends a signal to a running instance by PID.
pub fn send_signal_to_pid(pid: u32, sig_num: i32) -> bool {
    if let Ok(sig) = Signal::try_from(sig_num) {
        let pid = nix::unistd::Pid::from_raw(pid as i32);
        signal::kill(pid, sig).is_ok()
    } else {
        false
    }
}

extern "C" fn sigterm_handler(_: i32) {
    log::info!("SIGTERM received, bye bye!");
    std::process::exit(0);
}
