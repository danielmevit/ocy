use std::fmt;
use std::sync::Arc;

use damt_platform::PlatformError;
use ksni::blocking::TrayMethods;

use crate::pure::{LinuxTrayIconState, decode_tray_icon_argb32, is_sandboxed_env};

/// Menu slot identifiers matching the Windows tray menu order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinuxTrayMenuSlot {
    PauseStatus,
    TakeShortBreakNow,
    TakeLongBreakNow,
    HoldUntilGameCloses,
    Pause1Hour,
    Pause3Hours,
    PauseUntilResumed,
    Resume,
    Settings,
    Quit,
}

/// Presentation of a single tray menu item.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LinuxTrayMenuItem {
    pub text: String,
    pub is_visible: bool,
    pub is_enabled: bool,
    pub is_checked: bool,
}

/// Complete tray menu + tooltip presentation handed to [`LinuxTrayHandle`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LinuxTrayPresentation {
    pub tooltip: String,
    pub pause_status: LinuxTrayMenuItem,
    pub take_short_break_now: LinuxTrayMenuItem,
    pub take_long_break_now: LinuxTrayMenuItem,
    pub hold_until_game_closes: LinuxTrayMenuItem,
    pub pause_1_hour: LinuxTrayMenuItem,
    pub pause_3_hours: LinuxTrayMenuItem,
    pub pause_until_resumed: LinuxTrayMenuItem,
    pub resume: LinuxTrayMenuItem,
    pub settings: LinuxTrayMenuItem,
    pub quit: LinuxTrayMenuItem,
}

/// User actions triggered from the StatusNotifierItem tray icon or its DBusMenu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinuxTrayAction {
    TrayClicked,
    TakeShortBreakNow,
    TakeLongBreakNow,
    HoldUntilGameCloses,
    Pause1Hour,
    Pause3Hours,
    PauseUntilResumed,
    Resume,
    Settings,
    Quit,
}

/// Identity configuration for [`LinuxTray`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinuxTrayConfig {
    pub id: String,
    pub title: String,
}

impl Default for LinuxTrayConfig {
    fn default() -> Self {
        Self {
            id: "xyz.damt.ocy".to_owned(),
            title: "Ocy".to_owned(),
        }
    }
}

type ActionCallback = Arc<dyn Fn(LinuxTrayAction) + Send + Sync>;

/// `ksni`-backed StatusNotifierItem tray service implementation.
pub struct LinuxTray {
    config: LinuxTrayConfig,
    icon_state: LinuxTrayIconState,
    use_light_set: bool,
    icon: Option<ksni::Icon>,
    presentation: LinuxTrayPresentation,
    on_action: ActionCallback,
}

impl fmt::Debug for LinuxTray {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinuxTray")
            .field("config", &self.config)
            .field("icon_state", &self.icon_state)
            .field("use_light_set", &self.use_light_set)
            .finish_non_exhaustive()
    }
}

impl LinuxTray {
    fn new(
        config: LinuxTrayConfig,
        initial_presentation: LinuxTrayPresentation,
        initial_icon_state: LinuxTrayIconState,
        initial_use_light_set: bool,
        on_action: ActionCallback,
    ) -> Result<Self, PlatformError> {
        let icon = load_ksni_icon(initial_icon_state, initial_use_light_set)?;
        Ok(Self {
            config,
            icon_state: initial_icon_state,
            use_light_set: initial_use_light_set,
            icon: Some(icon),
            presentation: initial_presentation,
            on_action,
        })
    }

    fn standard_item(
        &self,
        item: &LinuxTrayMenuItem,
        action: Option<LinuxTrayAction>,
    ) -> ksni::MenuItem<Self> {
        let cb = Arc::clone(&self.on_action);
        ksni::menu::StandardItem {
            label: escape_dbusmenu_label(&item.text),
            enabled: item.is_enabled,
            visible: item.is_visible,
            activate: Box::new(move |_| {
                if let Some(act) = action {
                    cb(act);
                }
            }),
            ..Default::default()
        }
        .into()
    }

    fn checkmark_item(
        &self,
        item: &LinuxTrayMenuItem,
        action: LinuxTrayAction,
    ) -> ksni::MenuItem<Self> {
        let cb = Arc::clone(&self.on_action);
        ksni::menu::CheckmarkItem {
            label: escape_dbusmenu_label(&item.text),
            enabled: item.is_enabled,
            visible: item.is_visible,
            checked: item.is_checked,
            activate: Box::new(move |_| cb(action)),
            ..Default::default()
        }
        .into()
    }
}

impl ksni::Tray for LinuxTray {
    fn id(&self) -> String {
        self.config.id.clone()
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::ApplicationStatus
    }

    fn title(&self) -> String {
        self.config.title.clone()
    }

    fn status(&self) -> ksni::Status {
        match self.icon_state {
            LinuxTrayIconState::Attention => ksni::Status::NeedsAttention,
            _ => ksni::Status::Active,
        }
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.icon.iter().cloned().collect()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: self.presentation.tooltip.clone(),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        (self.on_action)(LinuxTrayAction::TrayClicked);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        vec![
            self.standard_item(&self.presentation.pause_status, None),
            self.standard_item(
                &self.presentation.take_short_break_now,
                Some(LinuxTrayAction::TakeShortBreakNow),
            ),
            self.standard_item(
                &self.presentation.take_long_break_now,
                Some(LinuxTrayAction::TakeLongBreakNow),
            ),
            self.standard_item(
                &self.presentation.hold_until_game_closes,
                Some(LinuxTrayAction::HoldUntilGameCloses),
            ),
            ksni::MenuItem::Separator,
            self.checkmark_item(&self.presentation.pause_1_hour, LinuxTrayAction::Pause1Hour),
            self.checkmark_item(
                &self.presentation.pause_3_hours,
                LinuxTrayAction::Pause3Hours,
            ),
            self.checkmark_item(
                &self.presentation.pause_until_resumed,
                LinuxTrayAction::PauseUntilResumed,
            ),
            self.standard_item(&self.presentation.resume, Some(LinuxTrayAction::Resume)),
            ksni::MenuItem::Separator,
            self.standard_item(&self.presentation.settings, Some(LinuxTrayAction::Settings)),
            self.standard_item(&self.presentation.quit, Some(LinuxTrayAction::Quit)),
        ]
    }
}

/// Owning handle to a running StatusNotifierItem tray service. Shuts down cleanly on drop.
pub struct LinuxTrayHandle {
    handle: Option<ksni::blocking::Handle<LinuxTray>>,
}

impl fmt::Debug for LinuxTrayHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinuxTrayHandle")
            .field("active", &self.handle.is_some())
            .finish()
    }
}

impl LinuxTrayHandle {
    /// Spawns the `ksni` StatusNotifierItem service on a background thread.
    pub fn spawn<F>(
        config: LinuxTrayConfig,
        initial_presentation: LinuxTrayPresentation,
        initial_icon_state: LinuxTrayIconState,
        initial_use_light_set: bool,
        on_action: F,
    ) -> Result<Self, PlatformError>
    where
        F: Fn(LinuxTrayAction) + Send + Sync + 'static,
    {
        let tray = LinuxTray::new(
            config,
            initial_presentation,
            initial_icon_state,
            initial_use_light_set,
            Arc::new(on_action),
        )?;
        let sandboxed = is_sandboxed_process();
        let handle = tray
            .disable_dbus_name(sandboxed)
            .assume_sni_available(true)
            .spawn()
            .map_err(|err| PlatformError(format!("ksni tray spawn failed: {err}")))?;
        Ok(Self {
            handle: Some(handle),
        })
    }

    /// Updates the tray hover tooltip text.
    pub fn set_tooltip(&self, tooltip: &str) {
        if let Some(handle) = &self.handle {
            let text = tooltip.to_owned();
            handle.update(move |tray| {
                tray.presentation.tooltip = text;
            });
        }
    }

    /// Updates the tray icon from `assets/icons/tray` for `(state, use_light_set)`.
    pub fn set_icon(
        &self,
        state: LinuxTrayIconState,
        use_light_set: bool,
    ) -> Result<(), PlatformError> {
        let icon = load_ksni_icon(state, use_light_set)?;
        if let Some(handle) = &self.handle {
            handle.update(move |tray| {
                tray.icon_state = state;
                tray.use_light_set = use_light_set;
                tray.icon = Some(icon);
            });
        }
        Ok(())
    }

    /// Replaces the tray menu presentation and tooltip.
    pub fn set_menu(&self, presentation: LinuxTrayPresentation) {
        if let Some(handle) = &self.handle {
            handle.update(move |tray| {
                tray.presentation = presentation;
            });
        }
    }
}

impl Drop for LinuxTrayHandle {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.shutdown().wait();
        }
    }
}

fn load_ksni_icon(
    state: LinuxTrayIconState,
    use_light_set: bool,
) -> Result<ksni::Icon, PlatformError> {
    let (width, height, data) =
        decode_tray_icon_argb32(state, use_light_set).map_err(PlatformError)?;
    Ok(ksni::Icon {
        width,
        height,
        data,
    })
}

fn escape_dbusmenu_label(label: &str) -> String {
    label.replace('_', "__")
}

fn is_sandboxed_process() -> bool {
    let flatpak_id = std::env::var("FLATPAK_ID").ok();
    let flatpak_info = std::path::Path::new("/.flatpak-info").exists();
    let cgroup = std::fs::read_to_string("/proc/self/cgroup").ok();
    is_sandboxed_env(flatpak_id.as_deref(), flatpak_info, cgroup.as_deref())
}
