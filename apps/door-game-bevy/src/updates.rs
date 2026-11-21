use std::{sync::mpsc, thread, time::SystemTime};

use bevy::app::AppExit;
use bevy::prelude::*;
use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
use steelgum_update::{Installation, UpdateCheck, UpdateError};

const VERSION: &str = env!("CARGO_PKG_VERSION");

type CheckResult = Result<UpdateCheck, UpdateError>;
type InstallResult = Result<(), UpdateError>;

#[derive(Default)]
pub(crate) struct Updates {
    installed: Option<Installation>,
    checking: Option<mpsc::Receiver<CheckResult>>,
    installing: Option<mpsc::Receiver<InstallResult>>,
    last_checked: Option<SystemTime>,
    manual: bool,
    auto_check: bool,
}

impl Updates {
    pub(crate) fn new(auto_check: bool) -> Self {
        Self {
            installed: find_installation(),
            auto_check,
            ..default()
        }
    }

    pub(crate) fn window_status(&self) -> &'static str {
        if self.installing.is_some() {
            "Starting update. Steelgum will restart shortly."
        } else if self.checking.is_some() && self.manual {
            "Checking for updates…"
        } else {
            "F6 Updates"
        }
    }

    fn start_check(&mut self) {
        if self.checking.is_some() || self.installing.is_some() {
            return;
        }

        let (sender, receiver) = mpsc::channel();
        match thread::Builder::new()
            .name("update-check".into())
            .spawn(move || {
                let result = steelgum_update::check_for_updates(VERSION);
                if let Err(error) = &result {
                    report(&error.to_string());
                }
                // Closing the window drops the receiver.
                let _ = sender.send(result);
            }) {
            Ok(_) => self.checking = Some(receiver),
            Err(error) => show_error(&format!("Could not start the update check. {error}")),
        }
    }

    fn start_install(&mut self, check: &UpdateCheck) {
        let (Some(installed), Some(version)) =
            (self.installed.take(), check.latest_version.clone())
        else {
            show_error("Install a packaged copy of Steelgum to use self-update.");
            return;
        };

        let (sender, receiver) = mpsc::channel();
        match thread::Builder::new()
            .name("update-install".into())
            .spawn(move || {
                let result = steelgum_update::begin_install(&installed, &version);
                // Closing the window drops the receiver.
                let _ = sender.send(result);
            }) {
            Ok(_) => self.installing = Some(receiver),
            Err(error) => {
                self.installed = find_installation();
                show_error(&format!("Could not start the updater. {error}"));
            }
        }
    }
}

pub(crate) fn startup(mut updates: NonSendMut<Updates>) {
    if updates.auto_check && updates.installed.is_some() {
        updates.start_check();
    }
}

pub(crate) fn poll(
    keys: Res<ButtonInput<KeyCode>>,
    mut updates: NonSendMut<Updates>,
    mut exit: MessageWriter<AppExit>,
) {
    if keys.just_pressed(KeyCode::F6) && updates.installing.is_none() {
        updates.manual = true;
        updates.start_check();
    }

    if let Some(result) = receive(&mut updates.installing) {
        match result {
            Ok(Ok(())) => {
                exit.write(AppExit::Success);
            }
            Ok(Err(error)) => {
                updates.installed = find_installation();
                show_error(&error.to_string());
            }
            Err(error) => {
                updates.installed = find_installation();
                show_error(error);
            }
        }
    }

    if let Some(result) = receive(&mut updates.checking) {
        updates.last_checked = Some(SystemTime::now());
        let manual = std::mem::take(&mut updates.manual);
        match result {
            Ok(Ok(check)) if check.update_available => offer_update(&mut updates, &check),
            Ok(Ok(check)) if manual => {
                let description = if check.latest_version.is_some() {
                    format!("You are already on the latest version ({VERSION}).")
                } else {
                    format!(
                        "No stable Steelgum release has been published. Current build: {VERSION}."
                    )
                };
                MessageDialog::new()
                    .set_title("Steelgum Updates")
                    .set_description(format!("{description}{}", last_checked(&updates)))
                    .show();
            }
            Ok(Err(error)) if manual => show_error(&error.to_string()),
            Err(error) => {
                report(error);
                if manual {
                    show_error(error);
                }
            }
            _ => {}
        }
    }
}

fn receive<T>(receiver: &mut Option<mpsc::Receiver<T>>) -> Option<Result<T, &'static str>> {
    let value = match receiver.as_ref()?.try_recv() {
        Ok(value) => Ok(value),
        Err(mpsc::TryRecvError::Empty) => return None,
        Err(mpsc::TryRecvError::Disconnected) => Err("The Steelgum updater stopped unexpectedly."),
    };
    *receiver = None;
    Some(value)
}

fn find_installation() -> Option<Installation> {
    match steelgum_update::installed_app() {
        Ok(installation) => installation,
        Err(error) => {
            report(&error.to_string());
            None
        }
    }
}

fn last_checked(updates: &Updates) -> String {
    updates.last_checked.map_or_else(String::new, |time| {
        let local = chrono::DateTime::<chrono::Local>::from(time);
        format!("\n\nLast checked {}", local.format("%Y-%m-%d %H:%M:%S %Z"))
    })
}

fn is_choice(result: MessageDialogResult, standard: MessageDialogResult, label: &str) -> bool {
    result == standard || matches!(result, MessageDialogResult::Custom(button) if button == label)
}

fn offer_update(updates: &mut Updates, check: &UpdateCheck) {
    let Some(latest) = &check.latest_version else {
        return;
    };
    let mut dialog = MessageDialog::new()
        .set_title("Steelgum Update Available")
        .set_description(format!(
            "Steelgum {} → {latest}{}",
            check.current_version,
            last_checked(updates)
        ));
    if updates.installed.is_some() {
        dialog = dialog.set_buttons(MessageButtons::YesNoCancelCustom(
            "Update Now".into(),
            "View Release Notes".into(),
            "Later".into(),
        ));
        match dialog.show() {
            MessageDialogResult::Yes => updates.start_install(check),
            MessageDialogResult::Custom(button) if button == "Update Now" => {
                updates.start_install(check)
            }
            MessageDialogResult::No => release_notes(check),
            MessageDialogResult::Custom(button) if button == "View Release Notes" => {
                release_notes(check)
            }
            _ => {}
        }
    } else {
        let result = dialog
            .set_buttons(MessageButtons::OkCancelCustom(
                "View Release Notes".into(),
                "Close".into(),
            ))
            .show();
        if is_choice(result, MessageDialogResult::Ok, "View Release Notes") {
            release_notes(check);
        }
    }
}

fn release_notes(check: &UpdateCheck) {
    if let Err(error) = steelgum_update::open_release_notes(check) {
        show_error(&error.to_string());
    }
}

pub(crate) fn warn_failed_install() {
    let description = match steelgum_update::update_log_path() {
        Ok(path) => format!(
            "Steelgum could not install the update, so this version is still installed.\n\nUpdate log: {}",
            path.display()
        ),
        Err(error) => {
            show_error(&format!("Steelgum could not install the update. {error}"));
            return;
        }
    };
    let result = MessageDialog::new()
        .set_level(MessageLevel::Warning)
        .set_title("Steelgum Update Failed")
        .set_description(description)
        .set_buttons(MessageButtons::OkCancelCustom(
            "Open Update Log".into(),
            "Close".into(),
        ))
        .show();
    if is_choice(result, MessageDialogResult::Ok, "Open Update Log")
        && let Err(error) = steelgum_update::open_update_log()
    {
        show_error(&error.to_string());
    }
}

fn show_error(description: &str) {
    MessageDialog::new()
        .set_level(MessageLevel::Error)
        .set_title("Steelgum Updates")
        .set_description(description)
        .show();
}

fn report(message: &str) {
    if let Err(error) = steelgum_update::log_message(message) {
        eprintln!("{message}\n{error}");
    }
}
