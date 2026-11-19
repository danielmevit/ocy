//! Slint `SystemTrayIcon` component and [`TraySink`] implementation.

use std::fmt;

use damt_app::ErrorLog;
use slint::{Image, Rgba8Pixel, SharedPixelBuffer, SharedString};

use crate::tray::{TrayIconState, TrayPresentation, TraySink, decode_tray_icon_rgba};

slint::slint! {
    #[edition = "1.19"]
    export component OcyTray inherits SystemTrayIcon {
        in property <image> tray-icon;
        in property <string> tray-tooltip;

        in property <string> pause-status-text;
        in property <bool> pause-status-visible: false;
        in property <bool> pause-status-enabled: false;

        in property <string> take-short-break-text;
        in property <bool> take-short-break-enabled: true;

        in property <string> take-long-break-text;
        in property <bool> take-long-break-enabled: true;

        in property <string> pause-1h-text;
        in property <bool> pause-1h-enabled: true;
        in-out property <bool> pause-1h-checked: false;

        in property <string> pause-3h-text;
        in property <bool> pause-3h-enabled: true;
        in-out property <bool> pause-3h-checked: false;

        in property <string> pause-until-resumed-text;
        in property <bool> pause-until-resumed-enabled: true;
        in-out property <bool> pause-until-resumed-checked: false;

        in property <string> resume-text;
        in property <bool> resume-visible: false;
        in property <bool> resume-enabled: false;

        in property <string> settings-text;
        in property <bool> settings-enabled: true;

        in property <string> quit-text;
        in property <bool> quit-enabled: true;

        callback tray-clicked();
        callback take-short-break-clicked();
        callback take-long-break-clicked();
        callback pause-1h-clicked();
        callback pause-3h-clicked();
        callback pause-until-resumed-clicked();
        callback resume-clicked();
        callback settings-clicked();
        callback quit-clicked();

        icon: tray-icon;
        tooltip: tray-tooltip;
        clicked => { root.tray-clicked(); }

        Menu {
            if root.pause-status-visible : MenuItem {
                title: root.pause-status-text;
                enabled: root.pause-status-enabled;
            }
            MenuItem {
                title: root.take-short-break-text;
                enabled: root.take-short-break-enabled;
                activated => { root.take-short-break-clicked(); }
            }
            MenuItem {
                title: root.take-long-break-text;
                enabled: root.take-long-break-enabled;
                activated => { root.take-long-break-clicked(); }
            }
            MenuSeparator {}
            MenuItem {
                title: root.pause-1h-text;
                enabled: root.pause-1h-enabled;
                checkable: true;
                checked <=> root.pause-1h-checked;
                activated => { root.pause-1h-clicked(); }
            }
            MenuItem {
                title: root.pause-3h-text;
                enabled: root.pause-3h-enabled;
                checkable: true;
                checked <=> root.pause-3h-checked;
                activated => { root.pause-3h-clicked(); }
            }
            MenuItem {
                title: root.pause-until-resumed-text;
                enabled: root.pause-until-resumed-enabled;
                checkable: true;
                checked <=> root.pause-until-resumed-checked;
                activated => { root.pause-until-resumed-clicked(); }
            }
            if root.resume-visible : MenuItem {
                title: root.resume-text;
                enabled: root.resume-enabled;
                activated => { root.resume-clicked(); }
            }
            MenuSeparator {}
            MenuItem {
                title: root.settings-text;
                enabled: root.settings-enabled;
                activated => { root.settings-clicked(); }
            }
            MenuItem {
                title: root.quit-text;
                enabled: root.quit-enabled;
                activated => { root.quit-clicked(); }
            }
        }
    }
}

/// [`TraySink`] backed by a Slint [`OcyTray`] `SystemTrayIcon`.
pub struct SlintTraySink {
    tray: OcyTray,
    error_log: ErrorLog,
}

impl fmt::Debug for SlintTraySink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SlintTraySink").finish_non_exhaustive()
    }
}

impl SlintTraySink {
    /// Wraps an initialized [`OcyTray`] handle.
    #[must_use]
    pub const fn new(tray: OcyTray, error_log: ErrorLog) -> Self {
        Self { tray, error_log }
    }

    /// Borrows the underlying [`OcyTray`] handle.
    #[must_use]
    pub const fn component(&self) -> &OcyTray {
        &self.tray
    }
}

impl TraySink for SlintTraySink {
    fn set_tooltip(&mut self, tooltip: &str) {
        self.tray.set_tray_tooltip(SharedString::from(tooltip));
    }

    fn set_icon(&mut self, state: TrayIconState, use_light_set: bool) {
        match decode_tray_icon_rgba(state, use_light_set) {
            Ok((width, height, pixels)) => {
                let buf = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&pixels, width, height);
                self.tray
                    .set_tray_icon(Image::from_rgba8_premultiplied(buf));
            }
            Err(err) => {
                self.error_log.log(Some("tray_icon"), &err);
            }
        }
    }

    fn set_menu(&mut self, presentation: &TrayPresentation) {
        self.tray
            .set_pause_status_text(SharedString::from(presentation.pause_status.text.as_str()));
        self.tray
            .set_pause_status_visible(presentation.pause_status.is_visible);
        self.tray
            .set_pause_status_enabled(presentation.pause_status.is_enabled);

        self.tray.set_take_short_break_text(SharedString::from(
            presentation.take_short_break_now.text.as_str(),
        ));
        self.tray
            .set_take_short_break_enabled(presentation.take_short_break_now.is_enabled);

        self.tray.set_take_long_break_text(SharedString::from(
            presentation.take_long_break_now.text.as_str(),
        ));
        self.tray
            .set_take_long_break_enabled(presentation.take_long_break_now.is_enabled);

        self.tray
            .set_pause_1h_text(SharedString::from(presentation.pause_1_hour.text.as_str()));
        self.tray
            .set_pause_1h_enabled(presentation.pause_1_hour.is_enabled);
        self.tray
            .set_pause_1h_checked(presentation.pause_1_hour.is_checked);

        self.tray
            .set_pause_3h_text(SharedString::from(presentation.pause_3_hours.text.as_str()));
        self.tray
            .set_pause_3h_enabled(presentation.pause_3_hours.is_enabled);
        self.tray
            .set_pause_3h_checked(presentation.pause_3_hours.is_checked);

        self.tray.set_pause_until_resumed_text(SharedString::from(
            presentation.pause_until_resumed.text.as_str(),
        ));
        self.tray
            .set_pause_until_resumed_enabled(presentation.pause_until_resumed.is_enabled);
        self.tray
            .set_pause_until_resumed_checked(presentation.pause_until_resumed.is_checked);

        self.tray
            .set_resume_text(SharedString::from(presentation.resume.text.as_str()));
        self.tray.set_resume_visible(presentation.resume.is_visible);
        self.tray.set_resume_enabled(presentation.resume.is_enabled);

        self.tray
            .set_settings_text(SharedString::from(presentation.settings.text.as_str()));
        self.tray
            .set_settings_enabled(presentation.settings.is_enabled);

        self.tray
            .set_quit_text(SharedString::from(presentation.quit.text.as_str()));
        self.tray.set_quit_enabled(presentation.quit.is_enabled);
    }
}
