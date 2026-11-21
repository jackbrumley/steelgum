use std::ffi::OsString;

use anyhow::{Context as _, Result, bail};
use gpui::{App, AsyncWindowContext, Global, PromptButton, PromptLevel, Window};
use steelgum_update::{Installation, UpdateCheck};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Default)]
struct UpdateActivity {
    busy: bool,
}

impl Global for UpdateActivity {}

pub(crate) fn handle_cli(args: &[OsString]) -> Result<bool> {
    let [flag] = args else {
        return Ok(false);
    };
    match flag.to_str() {
        Some("--version" | "-V") => println!("steelgum {VERSION}"),
        Some("--check-updates") => {
            let check = steelgum_update::check_for_updates(VERSION)?;
            println!("{}", serde_json::to_string_pretty(&check)?);
        }
        Some("--update") => {
            let installation = steelgum_update::installed_app()?
                .context("Use the installed Steelgum to update")?;
            let check = steelgum_update::check_for_updates(VERSION)?;
            if !check.update_available {
                bail!("No newer stable Steelgum version is available");
            }
            let latest = check.latest_version.context("The update has no version")?;
            steelgum_update::begin_install(&installation, &latest)?;
        }
        _ => return Ok(false),
    }
    Ok(true)
}

pub(crate) fn startup(window: &mut Window, cx: &mut App, update_failed: bool) {
    if update_failed {
        window
            .spawn(cx, async move |cx| warn_failed_install(cx).await)
            .detach();
    } else {
        check(window, cx, false);
    }
}

pub(crate) fn check(window: &mut Window, cx: &mut App, manual: bool) {
    let updates = cx.default_global::<UpdateActivity>();
    if updates.busy {
        return;
    }
    updates.busy = true;

    window
        .spawn(cx, async move |cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let installation = steelgum_update::installed_app()?;
                    if !manual && installation.is_none() {
                        return Ok(None);
                    }
                    let check = steelgum_update::check_for_updates(VERSION)?;
                    Ok::<_, steelgum_update::UpdateError>(Some((installation, check)))
                })
                .await;
            match result {
                Ok(Some((installed, check))) if check.update_available => {
                    offer_update(cx, installed, check).await;
                }
                Ok(Some((_, check))) if manual => {
                    let description = if check.latest_version.is_some() {
                        format!("You are already on the latest version ({VERSION}).")
                    } else {
                        format!("No stable Steelgum release has been published. Current build: {VERSION}.")
                    };
                    let _ = cx
                        .prompt(
                            PromptLevel::Info,
                            "Steelgum Updates",
                            Some(&format!("{description}{}", checked_now())),
                            &[PromptButton::ok("Close")],
                        )
                        .await;
                }
                Err(error) => {
                    record_failure(&error.to_string());
                    if manual {
                        show_error(cx, &error.to_string()).await;
                    }
                }
                _ => {}
            }
            let _ = cx.update(|_, cx| cx.default_global::<UpdateActivity>().busy = false);
        })
        .detach();
}

async fn offer_update(
    cx: &mut AsyncWindowContext,
    installation: Option<Installation>,
    check: UpdateCheck,
) {
    let Some(latest) = check.latest_version.as_ref() else {
        return;
    };
    let description = format!(
        "Steelgum {} → {latest}{}",
        check.current_version,
        checked_now()
    );
    let notes = PromptButton::new("View Release Notes");
    let later = PromptButton::cancel("Later");
    let Some(installation) = installation else {
        if cx
            .prompt(
                PromptLevel::Info,
                "Steelgum Update Available",
                Some(&description),
                &[notes, later],
            )
            .await
            == Ok(0)
        {
            view_release_notes(cx, &check).await;
        }
        return;
    };
    let choice = cx
        .prompt(
            PromptLevel::Info,
            "Steelgum Update Available",
            Some(&description),
            &[PromptButton::ok("Update Now"), notes, later],
        )
        .await;
    match choice {
        Ok(0) => {
            let latest = latest.clone();
            let result = cx
                .background_executor()
                .spawn(async move { steelgum_update::begin_install(&installation, &latest) })
                .await;
            match result {
                Ok(()) => {
                    let _ = cx.update(|_, cx| cx.quit());
                }
                Err(error) => {
                    record_failure(&error.to_string());
                    if cx
                        .prompt(
                            PromptLevel::Critical,
                            "Steelgum could not start the updater",
                            Some(&error.to_string()),
                            &[
                                PromptButton::new("Open Download Page"),
                                PromptButton::cancel("Close"),
                            ],
                        )
                        .await
                        == Ok(0)
                    {
                        view_release_notes(cx, &check).await;
                    }
                }
            }
        }
        Ok(1) => view_release_notes(cx, &check).await,
        _ => {}
    }
}

async fn warn_failed_install(cx: &mut AsyncWindowContext) {
    let log = match steelgum_update::update_log_path() {
        Ok(path) => path,
        Err(error) => {
            show_error(cx, &error.to_string()).await;
            return;
        }
    };
    let description = format!(
        "Steelgum could not install the update, so this version is still installed.\n\nUpdate log: {}",
        log.display()
    );
    if cx
        .prompt(
            PromptLevel::Warning,
            "Steelgum Update Failed",
            Some(&description),
            &[
                PromptButton::new("Open Update Log"),
                PromptButton::cancel("Close"),
            ],
        )
        .await
        == Ok(0)
        && let Err(error) = steelgum_update::open_update_log()
    {
        show_error(cx, &error.to_string()).await;
    }
}

async fn view_release_notes(cx: &mut AsyncWindowContext, check: &UpdateCheck) {
    if let Err(error) = steelgum_update::open_release_notes(check) {
        show_error(cx, &error.to_string()).await;
    }
}

async fn show_error(cx: &mut AsyncWindowContext, description: &str) {
    let _ = cx
        .prompt(
            PromptLevel::Critical,
            "Steelgum Updates",
            Some(description),
            &[PromptButton::cancel("Close")],
        )
        .await;
}

fn checked_now() -> String {
    let local = chrono::Local::now();
    format!("\n\nLast checked {}", local.format("%Y-%m-%d %H:%M:%S %Z"))
}

fn record_failure(message: &str) {
    if let Err(error) = steelgum_update::log_message(message) {
        eprintln!("{message}\n{error}");
    }
}
