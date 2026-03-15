use crate::ui::panel::NotificationPanel;
use dock_common::signals::{self, WindowCommand};
use gtk4::glib;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;

/// Starts signal listener using dock-common's proven signal handler.
///
/// Must be called BEFORE app.connect_activate (before GTK starts).
/// Reuses the same SIGRTMIN+1/+2/+3 signals as the dock — the notification
/// daemon is treated as a resident app that toggles on SIGRTMIN+1.
pub fn start_signal_listener() -> Rc<mpsc::Receiver<WindowCommand>> {
    Rc::new(signals::setup_signal_handlers(true))
}

/// Polls the signal receiver on the GTK main thread.
pub fn poll_signals(
    sig_rx: &Rc<mpsc::Receiver<WindowCommand>>,
    panel: &Rc<RefCell<NotificationPanel>>,
    state: &Rc<RefCell<crate::state::NotificationState>>,
) {
    let panel = Rc::clone(panel);
    let state = Rc::clone(state);
    let rx = Rc::clone(sig_rx);

    glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                WindowCommand::Toggle | WindowCommand::Show => {
                    log::debug!("Signal: toggle panel");
                    panel.borrow().toggle();
                }
                WindowCommand::Hide => {
                    if panel.borrow().is_visible() {
                        panel.borrow().toggle();
                    }
                }
                WindowCommand::Quit => {
                    std::process::exit(0);
                }
            }
        }
        glib::ControlFlow::Continue
    });
}
