//! Session events: screen lock/unlock via distributed notifications, sleep/wake via
//! `NSWorkspace` notifications.

use std::fmt;
use std::ptr::NonNull;
use std::sync::Arc;

use block2::RcBlock;
use damt_platform::{SessionEvent, SessionEventSource};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{AllocAnyThread, DefinedClass, define_class, msg_send, sel};
use objc2_app_kit::{
    NSWorkspace, NSWorkspaceDidWakeNotification, NSWorkspaceWillSleepNotification,
};
use objc2_foundation::{
    NSDistributedNotificationCenter, NSNotification, NSNotificationCenter, NSNotificationName,
    NSString,
};

use crate::pure::{SCREEN_IS_LOCKED_NOTIFICATION, SCREEN_IS_UNLOCKED_NOTIFICATION};

type Sink = Arc<dyn Fn(SessionEvent) + Send + Sync>;

/// Ivars of [`LockObserver`]: the shared event sink.
struct LockObserverIvars {
    sink: Sink,
}

// SAFETY:
// - The superclass `NSObject` has no subclassing requirements.
// - `LockObserver` does not implement `Drop`; it is released through `Retained`
//   after `MacSessionEvents` removed it from the notification center.
// - The ivars are only read while the object is registered, and the center keeps
//   delivering to it for exactly that long.
define_class!(
    #[unsafe(super(NSObject))]
    #[ivars = LockObserverIvars]
    struct LockObserver;

    unsafe impl NSObjectProtocol for LockObserver {}

    impl LockObserver {
        #[unsafe(method(onLocked:))]
        fn on_locked(&self, _notification: Option<&NSNotification>) {
            (self.ivars().sink)(SessionEvent::Locked);
        }

        #[unsafe(method(onUnlocked:))]
        fn on_unlocked(&self, _notification: Option<&NSNotification>) {
            (self.ivars().sink)(SessionEvent::Unlocked);
        }
    }
);

impl LockObserver {
    fn new(sink: Sink) -> Retained<Self> {
        let this = Self::alloc().set_ivars(LockObserverIvars { sink });
        // SAFETY: `this` is a valid, just-allocated `LockObserver`; `init` is
        // `NSObject`'s designated initializer and returns a fully-formed instance.
        unsafe { msg_send![super(this), init] }
    }
}

/// The registered observer objects (the distributed-notification observer plus the
/// block-observer tokens of the workspace notification center).
///
/// SAFETY (Send + Sync): the ObjC objects are opaque handles here. They are created
/// in `start` and removed in `Drop`, both of which take `&mut self`, so no Rust code
/// touches them through a shared reference; the notification centers access the
/// objects on the main thread with the ObjC runtime's own synchronization while they
/// are registered. Moving or sharing these pointers across threads is therefore
/// sound, and `MacSessionEvents` (through `MacPlatform`) satisfies the `Send + Sync`
/// that `Arc<dyn PlatformServices>` requires.
struct ObserverState {
    lock_observer: Option<Retained<LockObserver>>,
    workspace_tokens: Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
}

unsafe impl Send for ObserverState {}
unsafe impl Sync for ObserverState {}

/// macOS [`SessionEventSource`]:
///
/// - lock/unlock: the distributed notifications `com.apple.screenIsLocked` /
///   `com.apple.screenIsUnlocked` (posted by the screen-lock session),
/// - sleep/wake: `NSWorkspaceWillSleepNotification` / `NSWorkspaceDidWakeNotification`
///   on the workspace notification center.
///
/// `start` only registers the observers; delivery happens on the running main run
/// loop (the host app's event loop), which is also where the callbacks fire. Call
/// `start` from the main thread, like the rest of AppKit.
pub struct MacSessionEvents {
    started: bool,
    observers: ObserverState,
}

impl fmt::Debug for MacSessionEvents {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MacSessionEvents")
            .field("started", &self.started)
            .finish_non_exhaustive()
    }
}

impl Default for MacSessionEvents {
    fn default() -> Self {
        Self::new()
    }
}

impl MacSessionEvents {
    /// Creates a session event source; register with [`SessionEventSource::start`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            started: false,
            observers: ObserverState {
                lock_observer: None,
                workspace_tokens: Vec::new(),
            },
        }
    }
}

impl SessionEventSource for MacSessionEvents {
    fn is_supported(&self) -> bool {
        true
    }

    fn start(&mut self, sink: Box<dyn Fn(SessionEvent) + Send + Sync>) {
        if self.started {
            return;
        }
        self.started = true;
        let sink: Sink = Arc::from(sink);

        // Lock/unlock: selector-based observer on the distributed notification center.
        let observer = LockObserver::new(Arc::clone(&sink));
        let center = NSDistributedNotificationCenter::defaultCenter();
        let locked = NSString::from_str(SCREEN_IS_LOCKED_NOTIFICATION);
        let unlocked = NSString::from_str(SCREEN_IS_UNLOCKED_NOTIFICATION);
        // SAFETY: `observer` is a live `LockObserver` object; `onLocked:` and
        // `onUnlocked:` are valid selectors it implements; both names are valid
        // `NSNotificationName` values and the object filter is `nil`.
        unsafe {
            center.addObserver_selector_name_object(
                &observer,
                sel!(onLocked:),
                Some(&locked),
                None,
            );
            center.addObserver_selector_name_object(
                &observer,
                sel!(onUnlocked:),
                Some(&unlocked),
                None,
            );
        }
        self.observers.lock_observer = Some(observer);

        // Sleep/wake: block-based observers on the workspace notification center.
        let workspace_center: Retained<NSNotificationCenter> =
            NSWorkspace::sharedWorkspace().notificationCenter();
        // SAFETY: both statics are constant, always-initialized notification names
        // provided by AppKit.
        let will_sleep: &'static NSNotificationName = unsafe { NSWorkspaceWillSleepNotification };
        let did_wake: &'static NSNotificationName = unsafe { NSWorkspaceDidWakeNotification };
        for (name, event) in [
            (will_sleep, SessionEvent::Suspending),
            (did_wake, SessionEvent::Resumed),
        ] {
            let sink = Arc::clone(&sink);
            let block = RcBlock::new(move |_note: NonNull<NSNotification>| sink(event));
            // SAFETY: the block is a valid `Fn(NonNull<NSNotification>)` whose closure
            // is `Send` + `Sync` (an `Arc` around the sink); the queue is `nil`, so it
            // runs on the posting thread (the main thread), and the name is a valid
            // static notification name.
            let token = unsafe {
                workspace_center.addObserverForName_object_queue_usingBlock(
                    Some(name),
                    None,
                    None,
                    &block,
                )
            };
            self.observers.workspace_tokens.push(token);
        }
    }
}

impl Drop for MacSessionEvents {
    fn drop(&mut self) {
        if let Some(observer) = self.observers.lock_observer.take() {
            let center = NSDistributedNotificationCenter::defaultCenter();
            // SAFETY: `observer` was registered exactly by `start` and is still
            // alive; `nil` name and object remove every entry for this observer.
            unsafe { center.removeObserver_name_object(&observer, None, None) };
        }
        if !self.observers.workspace_tokens.is_empty() {
            let center = NSWorkspace::sharedWorkspace().notificationCenter();
            for token in self.observers.workspace_tokens.drain(..) {
                // SAFETY: `token` is the opaque observer object that
                // `addObserverForName:object:queue:usingBlock:` returned and it has
                // not been removed yet.
                let observer: &AnyObject = token.as_ref();
                unsafe { center.removeObserver(observer) };
            }
        }
    }
}
