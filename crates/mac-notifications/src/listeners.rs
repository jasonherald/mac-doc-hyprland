use crate::state::NotificationState;
use crate::ui::panel::NotificationPanel;
use dock_common::signals;
use gtk4::glib;
use nix::sys::signal::{self, Signal};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;

/// Commands the notification daemon responds to via signals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationCommand {
    TogglePanel,
    ToggleDnd,
}

/// Starts signal listener for notification-specific signals.
///
/// Must be called BEFORE app.connect_activate (before GTK starts).
/// Uses SIGRTMIN+4 (panel toggle) and SIGRTMIN+5 (DND toggle).
pub fn start_signal_listener() -> Rc<mpsc::Receiver<NotificationCommand>> {
    let (tx, rx) = mpsc::channel();

    let sig_panel = signals::sig_notification_toggle();
    let sig_dnd = signals::sig_notification_dnd();

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

    // Block notification signals in the main thread BEFORE spawning.
    // Uses raw libc because nix's Signal enum doesn't support RT signals.
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, sig_panel);
        libc::sigaddset(&mut set, sig_dnd);
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
    }

    // Sigwait thread — inherits the blocked signal mask
    std::thread::spawn(move || {
        loop {
            let mut sig: i32 = 0;
            // SAFETY: sigwait blocks until a signal from the set is pending.
            unsafe {
                let mut set: libc::sigset_t = std::mem::zeroed();
                libc::sigemptyset(&mut set);
                libc::sigaddset(&mut set, sig_panel);
                libc::sigaddset(&mut set, sig_dnd);
                libc::sigwait(&set, &mut sig);
            }

            let cmd = if sig == sig_panel {
                Some(NotificationCommand::TogglePanel)
            } else if sig == sig_dnd {
                Some(NotificationCommand::ToggleDnd)
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

    Rc::new(rx)
}

/// Polls the signal receiver on the GTK main thread.
pub fn poll_signals(
    sig_rx: &Rc<mpsc::Receiver<NotificationCommand>>,
    panel: &Rc<RefCell<NotificationPanel>>,
    state: &Rc<RefCell<NotificationState>>,
) {
    let panel = Rc::clone(panel);
    let state = Rc::clone(state);
    let rx = Rc::clone(sig_rx);

    glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                NotificationCommand::TogglePanel => {
                    log::debug!("Signal: toggle panel");
                    panel.borrow().toggle();
                }
                NotificationCommand::ToggleDnd => {
                    let new_dnd = !state.borrow().dnd;
                    state.borrow_mut().dnd = new_dnd;
                    log::info!(
                        "DND {} via signal",
                        if new_dnd { "enabled" } else { "disabled" }
                    );
                }
            }
        }
        glib::ControlFlow::Continue
    });
}

extern "C" fn sigterm_handler(_: i32) {
    log::info!("SIGTERM received, bye bye!");
    std::process::exit(0);
}
