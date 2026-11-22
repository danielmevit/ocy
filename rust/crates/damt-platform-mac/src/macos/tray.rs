//! Native macOS menu-bar icon (`NSStatusItem` + `NSMenu`) via `objc2-app-kit`.

use std::fmt;
use std::sync::Arc;

use damt_platform::PlatformError;
use objc2::rc::Retained;
use objc2::runtime::NSObject;
use objc2::{
    AllocAnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel,
};
use objc2_app_kit::{
    NSControlStateValueOff, NSControlStateValueOn, NSImage, NSMenu, NSMenuItem, NSStatusBar,
    NSStatusItem, NSVariableStatusItemLength,
};
use objc2_foundation::{NSData, NSObjectProtocol, NSSize, NSString};

use crate::pure::{
    MacTrayAction, MacTrayIconState, MacTrayMenuSlot, MacTrayPresentation, MacTrayRenderedEntry,
    action_for_tag, plan_tray_menu_entries, tag_for_action, template_tray_icon_png,
};

type ActionCallback = Arc<dyn Fn(MacTrayAction) + Send + Sync>;

struct MacTrayTargetIvars {
    on_action: ActionCallback,
}

// SAFETY:
// - `NSObject` has no special subclassing requirements.
// - `MacTrayTarget` is `MainThreadOnly`, dropped only after `NSStatusItem` and `NSMenu` clear
//   references to it on the main thread.
define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = MacTrayTargetIvars]
    struct MacTrayTarget;

    unsafe impl NSObjectProtocol for MacTrayTarget {}

    impl MacTrayTarget {
        #[unsafe(method(onMenuItem:))]
        fn on_menu_item(&self, sender: &NSMenuItem) {
            if let Some(action) = action_for_tag(sender.tag()) {
                (self.ivars().on_action)(action);
            }
        }
    }
);

impl MacTrayTarget {
    fn new(mtm: MainThreadMarker, on_action: ActionCallback) -> Retained<Self> {
        let this = mtm.alloc().set_ivars(MacTrayTargetIvars { on_action });
        // SAFETY: `this` is a freshly allocated `MacTrayTarget` (`NSObject` subclass); `init`
        // returns an initialized instance on the main thread.
        unsafe { msg_send![super(this), init] }
    }
}

/// Configuration for [`MacTrayHandle`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacTrayConfig {
    pub title: String,
}

impl Default for MacTrayConfig {
    fn default() -> Self {
        Self {
            title: "Ocy".to_owned(),
        }
    }
}

/// Owning handle to an AppKit `NSStatusItem` and its `NSMenu`. Removes the status item on drop.
///
/// Must be created and used on the main AppKit/UI thread.
pub struct MacTrayHandle {
    mtm: MainThreadMarker,
    status_item: Retained<NSStatusItem>,
    menu: Retained<NSMenu>,
    target: Retained<MacTrayTarget>,
    items: Vec<(Option<MacTrayMenuSlot>, Retained<NSMenuItem>)>,
    current_icon_state: Option<MacTrayIconState>,
}

impl fmt::Debug for MacTrayHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MacTrayHandle")
            .field("current_icon_state", &self.current_icon_state)
            .field("menu_items", &self.items.len())
            .finish_non_exhaustive()
    }
}

impl MacTrayHandle {
    /// Creates the macOS menu-bar `NSStatusItem` and attaches its `NSMenu` on the main thread.
    pub fn spawn<F>(
        config: MacTrayConfig,
        initial_presentation: MacTrayPresentation,
        initial_icon_state: MacTrayIconState,
        on_action: F,
    ) -> Result<Self, PlatformError>
    where
        F: Fn(MacTrayAction) + Send + Sync + 'static,
    {
        let mtm = MainThreadMarker::new().ok_or_else(|| {
            PlatformError("NSStatusItem must be created on the main thread".to_owned())
        })?;

        let status_bar = NSStatusBar::systemStatusBar();
        let status_item = status_bar.statusItemWithLength(NSVariableStatusItemLength);
        let menu_title = NSString::from_str(&config.title);
        let menu = NSMenu::initWithTitle(mtm.alloc(), &menu_title);
        menu.setAutoenablesItems(false);

        let target = MacTrayTarget::new(mtm, Arc::new(on_action));
        let entries = plan_tray_menu_entries(&initial_presentation);
        let mut items = Vec::with_capacity(entries.len());
        let empty_key = NSString::from_str("");

        for entry in entries {
            match entry {
                MacTrayRenderedEntry::Separator => {
                    let sep = NSMenuItem::separatorItem(mtm);
                    menu.addItem(&sep);
                    items.push((None, sep));
                }
                MacTrayRenderedEntry::Item { slot, action, item } => {
                    let action_sel = action.map(|_| sel!(onMenuItem:));
                    let title = NSString::from_str(&item.text);
                    // SAFETY: `mtm.alloc()` allocates an uninitialized `NSMenuItem`; `title` and
                    // `empty_key` are valid `NSString`s and `action_sel` is either `None` or
                    // `onMenuItem:`, which `target` implements.
                    let ns_item = unsafe {
                        NSMenuItem::initWithTitle_action_keyEquivalent(
                            mtm.alloc(),
                            &title,
                            action_sel,
                            &empty_key,
                        )
                    };
                    if let Some(act) = action {
                        ns_item.setTag(tag_for_action(act));
                        // SAFETY: `target` stays retained on `Self` for the entire lifetime of
                        // `menu` and `status_item` (cleared in `Drop`), and implements `onMenuItem:`.
                        unsafe { ns_item.setTarget(Some(target.as_ref())) };
                    }
                    apply_menu_item_state(&ns_item, &item);
                    menu.addItem(&ns_item);
                    items.push((Some(slot), ns_item));
                }
            }
        }

        status_item.setMenu(Some(&menu));

        let mut handle = Self {
            mtm,
            status_item,
            menu,
            target,
            items,
            current_icon_state: None,
        };
        handle.set_tooltip(&initial_presentation.tooltip);
        handle.set_icon(initial_icon_state, false)?;
        Ok(handle)
    }

    /// Updates the status bar button hover tooltip.
    pub fn set_tooltip(&self, tooltip: &str) {
        if let Some(button) = self.status_item.button(self.mtm) {
            let tip = NSString::from_str(tooltip);
            button.setToolTip(Some(&tip));
        }
    }

    /// Updates the menu-bar icon using a template `NSImage` derived from the existing tray art.
    pub fn set_icon(
        &mut self,
        state: MacTrayIconState,
        _use_light_set: bool,
    ) -> Result<(), PlatformError> {
        if self.current_icon_state == Some(state) {
            return Ok(());
        }
        let png_bytes = template_tray_icon_png(state).map_err(PlatformError)?;
        let data = NSData::with_bytes(&png_bytes);
        let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
            return Err(PlatformError(
                "NSImage::initWithData returned nil".to_owned(),
            ));
        };
        // 32x32 PNG rendered at 16x16 pt gives a crisp 2x template icon on Retina menu bars.
        image.setSize(NSSize::new(16.0, 16.0));
        image.setTemplate(true);

        if let Some(button) = self.status_item.button(self.mtm) {
            button.setImage(Some(&image));
        }
        self.current_icon_state = Some(state);
        Ok(())
    }

    /// Updates menu item labels, visibility, enabled state, check marks, and tooltip in place.
    pub fn set_menu(&self, presentation: &MacTrayPresentation) {
        self.set_tooltip(&presentation.tooltip);
        let entries = plan_tray_menu_entries(presentation);
        for ((_, ns_item), entry) in self.items.iter().zip(entries) {
            if let MacTrayRenderedEntry::Item { item, .. } = entry {
                ns_item.setTitle(&NSString::from_str(&item.text));
                apply_menu_item_state(ns_item, &item);
            }
        }
    }
}

impl Drop for MacTrayHandle {
    fn drop(&mut self) {
        self.status_item.setMenu(None);
        for (_, ns_item) in &self.items {
            // SAFETY: Clearing target/action before dropping `self.target` prevents dangling
            // unretained target pointers in `NSMenuItem`.
            unsafe {
                ns_item.setTarget(None);
                ns_item.setAction(None);
            }
        }
        self.menu.removeAllItems();
        NSStatusBar::systemStatusBar().removeStatusItem(&self.status_item);
        let _ = &self.target;
    }
}

fn apply_menu_item_state(ns_item: &NSMenuItem, state: &crate::pure::MacTrayMenuItem) {
    ns_item.setHidden(!state.is_visible);
    ns_item.setEnabled(state.is_enabled);
    ns_item.setState(if state.is_checked {
        NSControlStateValueOn
    } else {
        NSControlStateValueOff
    });
}
