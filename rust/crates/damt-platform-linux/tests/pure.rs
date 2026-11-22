//! Pure-logic tests for `damt-platform-linux` (no display server or D-Bus daemon required).

#[cfg(target_os = "linux")]
mod linux_pure_tests {
    use std::collections::HashMap;
    use std::ffi::OsStr;
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use damt_platform::{AutostartService, FocusState, ScreenRect, SessionEvent};
    use damt_platform_linux::LinuxAutostart;
    use damt_platform_linux::pure::{
        AutostartDesktopError, DbusSignal, ExtIdleMapping, IdleEnv, IdleMath, IdleSourceKind,
        LinuxDisplayBackend, LinuxTrayIconState, LinuxWindowPolicy, MotionMapper,
        PortalColorScheme, RawX11Crtc, X11_BREAK_STATE_ATOM_NAMES, X11_BREAK_USER_TIME,
        X11_BREAK_WM_HINTS_INPUT, X11_WM_STATE_ABOVE_ATOM, X11_WM_STATE_SKIP_PAGER_ATOM,
        X11_WM_STATE_SKIP_TASKBAR_ATOM, X11_WM_WINDOW_TYPE_NOTIFICATION_ATOM, X11MonitorInfo,
        autostart_dir, autostart_file_path, build_background_portal_options, build_desktop_entry,
        decode_tray_icon_argb32, escape_desktop_exec_arg, ewmh_has_fullscreen,
        extract_portal_autostart_result, format_portal_autostart_state, intersect_screen_rect,
        is_flatpak_env, is_sandboxed_env, map_background_portal_response,
        map_background_portal_results, map_dbus_signal, merge_wm_state_atoms,
        net_wm_state_add_messages, parse_desktop_entry_enabled, parse_net_workarea,
        parse_portal_autostart_state, parse_raw_dbus_signal, parse_xft_dpi,
        portal_autostart_state_path, probe_idle_source, read_portal_autostart_state,
        resolve_logind_session_id, resolve_x11_monitors, rgba_to_argb32, select_active_monitor,
        select_display_backend, select_idle_source_order, write_portal_autostart_state,
        x11_restack_above_target,
    };
    use zbus::zvariant::{OwnedValue, Value};

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "damt-platform-linux-{name}-{}-{nanos}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    // --- Idle source selection order ---

    #[test]
    fn idle_selection_prefers_x11_when_display_set_and_not_wayland() {
        let env = IdleEnv {
            display: Some(":0".to_owned()),
            wayland_display: None,
            session_type: Some("x11".to_owned()),
            current_desktop: Some("XFCE".to_owned()),
        };
        assert_eq!(
            select_idle_source_order(&env),
            vec![IdleSourceKind::X11ScreenSaver]
        );
    }

    #[test]
    fn idle_selection_handles_x11_without_session_type() {
        let env = IdleEnv {
            display: Some(":1".to_owned()),
            wayland_display: None,
            session_type: None,
            current_desktop: None,
        };
        assert_eq!(
            select_idle_source_order(&env),
            vec![IdleSourceKind::X11ScreenSaver]
        );
    }

    #[test]
    fn idle_selection_on_gnome_wayland_uses_mutter_and_skips_xwayland() {
        // Wayland sessions commonly keep DISPLAY=:0 for XWayland; must NOT pick X11ScreenSaver.
        for desktop in ["GNOME", "ubuntu:GNOME", "pop:GNOME"] {
            let env = IdleEnv {
                display: Some(":0".to_owned()),
                wayland_display: Some("wayland-0".to_owned()),
                session_type: Some("wayland".to_owned()),
                current_desktop: Some(desktop.to_owned()),
            };
            assert_eq!(
                select_idle_source_order(&env),
                vec![
                    IdleSourceKind::GnomeMutterIdleMonitor,
                    IdleSourceKind::ExtIdleNotify,
                ]
            );
        }
    }

    #[test]
    fn idle_selection_on_kde_or_wlroots_wayland_uses_ext_idle_notify() {
        for desktop in ["KDE", "sway", "Hyprland"] {
            let env = IdleEnv {
                display: Some(":0".to_owned()),
                wayland_display: Some("wayland-0".to_owned()),
                session_type: Some("wayland".to_owned()),
                current_desktop: Some(desktop.to_owned()),
            };
            assert_eq!(
                select_idle_source_order(&env),
                vec![IdleSourceKind::ExtIdleNotify]
            );
        }
    }

    #[test]
    fn idle_selection_on_gnome_wayland_falls_back_from_mutter_to_ext_idle() {
        let env = IdleEnv {
            display: Some(":0".to_owned()),
            wayland_display: Some("wayland-0".to_owned()),
            session_type: Some("wayland".to_owned()),
            current_desktop: Some("GNOME".to_owned()),
        };
        assert_eq!(
            select_idle_source_order(&env),
            vec![
                IdleSourceKind::GnomeMutterIdleMonitor,
                IdleSourceKind::ExtIdleNotify,
            ]
        );
    }

    #[test]
    fn idle_selection_on_headless_ci_is_empty() {
        let env = IdleEnv::default();
        assert!(select_idle_source_order(&env).is_empty());
        assert_eq!(probe_idle_source(&env, |_| true), None);
    }

    #[test]
    fn idle_probe_falls_back_to_none_when_x11_unusable() {
        let env = IdleEnv {
            display: Some(":99".to_owned()),
            wayland_display: None,
            session_type: Some("x11".to_owned()),
            current_desktop: None,
        };
        assert_eq!(probe_idle_source(&env, |_| false), None);
        assert_eq!(
            probe_idle_source(&env, |kind| kind == IdleSourceKind::X11ScreenSaver),
            Some(IdleSourceKind::X11ScreenSaver)
        );
    }

    #[test]
    fn idle_math_converts_milliseconds() {
        assert_eq!(IdleMath::from_x11_millis(45_000), Duration::from_secs(45));
        assert_eq!(
            IdleMath::from_mutter_millis(u64::from(u32::MAX) + 500),
            Duration::from_millis(u64::from(u32::MAX) + 500)
        );
    }

    #[test]
    fn ext_idle_mapping_tracks_idle_since_timeout() {
        let start = std::time::Instant::now();
        let mut mapping = ExtIdleMapping::new();
        assert_eq!(mapping.idle_time(start), Duration::ZERO);

        let idled = start + Duration::from_secs(1);
        mapping.on_idled(idled, Duration::from_secs(1));
        assert_eq!(mapping.idle_time(idled), Duration::from_secs(1));
        assert_eq!(
            mapping.idle_time(start + Duration::from_secs(8)),
            Duration::from_secs(8)
        );

        mapping.on_resumed();
        assert_eq!(
            mapping.idle_time(idled + Duration::from_secs(9)),
            Duration::ZERO
        );
    }

    // --- D-Bus signal -> SessionEvent mapping via fakes ---

    #[test]
    fn dbus_signal_mapping_handles_fake_logind_stream() {
        let expected_session = "/org/freedesktop/login1/session/_32";
        let fake_stream = [
            (
                "org.freedesktop.login1.Manager",
                "PrepareForSleep",
                "/org/freedesktop/login1",
                Some(true),
            ),
            (
                "org.freedesktop.login1.Manager",
                "PrepareForSleep",
                "/org/freedesktop/login1",
                Some(false),
            ),
            (
                "org.freedesktop.login1.Session",
                "Lock",
                "/org/freedesktop/login1/session/_32",
                None,
            ),
            // Different user/TTY session must be ignored when expected_session is known.
            (
                "org.freedesktop.login1.Session",
                "Lock",
                "/org/freedesktop/login1/session/_99",
                None,
            ),
            (
                "org.freedesktop.login1.Session",
                "Unlock",
                "/org/freedesktop/login1/session/_32",
                None,
            ),
            (
                "org.freedesktop.login1.Manager",
                "PrepareForShutdown",
                "/org/freedesktop/login1",
                Some(true),
            ),
            (
                "org.freedesktop.login1.Manager",
                "PrepareForShutdown",
                "/org/freedesktop/login1",
                Some(false),
            ),
        ];

        let mapped: Vec<SessionEvent> = fake_stream
            .into_iter()
            .filter_map(|(iface, member, path, b)| {
                let sig = parse_raw_dbus_signal(iface, member, path, b)?;
                map_dbus_signal(&sig, Some(expected_session))
            })
            .collect();

        assert_eq!(
            mapped,
            vec![
                SessionEvent::Suspending,
                SessionEvent::Resumed,
                SessionEvent::Locked,
                SessionEvent::Unlocked,
                SessionEvent::LoggingOff,
            ]
        );
    }

    #[test]
    fn dbus_signal_mapping_handles_screensaver_alternative_and_wildcard_session() {
        let active = parse_raw_dbus_signal(
            "org.freedesktop.ScreenSaver",
            "ActiveChanged",
            "/org/freedesktop/ScreenSaver",
            Some(true),
        );
        assert_eq!(
            active,
            Some(DbusSignal::ScreenSaverActiveChanged { active: true })
        );
        assert_eq!(
            active.as_ref().and_then(|s| map_dbus_signal(s, None)),
            Some(SessionEvent::Locked)
        );

        let inactive = parse_raw_dbus_signal(
            "org.freedesktop.ScreenSaver",
            "ActiveChanged",
            "/org/freedesktop/ScreenSaver",
            Some(false),
        );
        assert_eq!(
            inactive.as_ref().and_then(|s| map_dbus_signal(s, None)),
            Some(SessionEvent::Unlocked)
        );

        let lock_any = parse_raw_dbus_signal(
            "org.freedesktop.login1.Session",
            "Lock",
            "/org/freedesktop/login1/session/c1",
            None,
        );
        assert_eq!(
            lock_any.as_ref().and_then(|s| map_dbus_signal(s, None)),
            Some(SessionEvent::Locked)
        );
    }

    #[test]
    fn dbus_signal_parser_ignores_unknown_or_missing_bool() {
        assert_eq!(
            parse_raw_dbus_signal(
                "org.freedesktop.login1.Manager",
                "PrepareForSleep",
                "/org/freedesktop/login1",
                None
            ),
            None
        );
        assert_eq!(
            parse_raw_dbus_signal(
                "org.freedesktop.DBus",
                "NameOwnerChanged",
                "/org/freedesktop/DBus",
                None
            ),
            None
        );
    }

    #[test]
    fn resolve_logind_session_id_trims_and_rejects_empty() {
        assert_eq!(resolve_logind_session_id(Some(" 42 \n")), Some("42"));
        assert_eq!(resolve_logind_session_id(Some("   ")), None);
        assert_eq!(resolve_logind_session_id(None), None);
    }

    // --- XDG autostart .desktop paths, formatting, parsing ---

    #[test]
    fn autostart_paths_follow_xdg_config_home_and_home_fallback() {
        assert_eq!(
            autostart_dir(Some(OsStr::new("/xdg")), Some(OsStr::new("/home/u"))),
            Some(PathBuf::from("/xdg/autostart"))
        );
        assert_eq!(
            autostart_dir(Some(OsStr::new("")), Some(OsStr::new("/home/u"))),
            Some(PathBuf::from("/home/u/.config/autostart"))
        );
        assert_eq!(autostart_dir(None, None), None);
        assert_eq!(
            autostart_file_path(
                Some(OsStr::new("/xdg")),
                Some(OsStr::new("/home/u")),
                "xyz.damt.ocy"
            ),
            Some(PathBuf::from("/xdg/autostart/xyz.damt.ocy.desktop"))
        );
    }

    #[test]
    fn escape_desktop_exec_arg_quotes_spaces_and_escapes_percent() {
        assert_eq!(
            escape_desktop_exec_arg("/usr/bin/Ocy").as_deref(),
            Ok("/usr/bin/Ocy")
        );
        assert_eq!(
            escape_desktop_exec_arg("/opt/My Apps/Ocy%1").as_deref(),
            Ok("\"/opt/My Apps/Ocy%%1\"")
        );
        assert_eq!(
            escape_desktop_exec_arg("   "),
            Err(AutostartDesktopError::EmptyPath)
        );
        assert_eq!(
            escape_desktop_exec_arg("/tmp/bad\npath"),
            Err(AutostartDesktopError::ControlCharacter)
        );
    }

    #[test]
    fn build_and_parse_desktop_entry_respects_hidden_and_gnome_flag() {
        let enabled_entry = build_desktop_entry(
            "Ocy",
            "Calm break reminders for your eyes",
            "xyz.damt.ocy",
            "/usr/local/bin/Ocy",
            false,
        )
        .expect("valid desktop entry");
        assert!(enabled_entry.contains("Exec=/usr/local/bin/Ocy --autostart"));
        assert!(enabled_entry.contains("Icon=xyz.damt.ocy"));
        assert!(parse_desktop_entry_enabled(&enabled_entry));

        let hidden_entry = build_desktop_entry(
            "Ocy",
            "Calm break reminders for your eyes",
            "xyz.damt.ocy",
            "/usr/local/bin/Ocy",
            true,
        )
        .expect("valid hidden desktop entry");
        assert!(hidden_entry.contains("Hidden=true"));
        assert!(!parse_desktop_entry_enabled(&hidden_entry));

        let gnome_disabled = "\
[Desktop Entry]
Type=Application
Name=Ocy
Exec=/usr/bin/Ocy --autostart
X-GNOME-Autostart-enabled=false
";
        assert!(!parse_desktop_entry_enabled(gnome_disabled));

        // Hidden=true in a non-[Desktop Entry] group must not disable [Desktop Entry].
        let hidden_in_action = "\
[Desktop Entry]
Type=Application
Name=Ocy
Exec=/usr/bin/Ocy --autostart

[Desktop Action Extra]
Hidden=true
";
        assert!(parse_desktop_entry_enabled(hidden_in_action));
    }

    #[test]
    fn linux_autostart_roundtrip_and_respects_external_hidden_true() {
        let dir = temp_dir("autostart");
        let desktop_path = dir.join("xyz.damt.ocy.desktop");
        let svc =
            LinuxAutostart::with_paths("Ocy", "xyz.damt.ocy", desktop_path.clone(), "/tmp/Ocy");

        assert!(svc.is_supported());
        assert!(!svc.is_enabled());

        svc.set_enabled(true).expect("enable autostart");
        assert!(desktop_path.is_file());
        assert!(svc.is_enabled());

        // Simulate a desktop environment toggling Hidden=true in the user file.
        std::fs::write(
            &desktop_path,
            "[Desktop Entry]\nType=Application\nName=Ocy\nExec=/tmp/Ocy --autostart\nHidden=true\n",
        )
        .expect("write hidden file");
        assert!(!svc.is_enabled());

        // Re-enabling clears Hidden=true.
        svc.set_enabled(true).expect("re-enable autostart");
        assert!(svc.is_enabled());

        svc.set_enabled(false).expect("disable autostart");
        assert!(!svc.is_enabled());
        assert!(!desktop_path.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn flatpak_detection_checks_injected_env_and_info_path() {
        let dir = temp_dir("flatpak-detect");
        let missing = dir.join("missing-flatpak-info");
        let present = dir.join("flatpak-info");
        std::fs::write(&present, "[Application]\nname=xyz.damt.ocy\n")
            .expect("write mock flatpak-info");

        assert!(!is_flatpak_env(None, &missing));
        assert!(!is_flatpak_env(Some(OsStr::new("")), &missing));
        assert!(!is_flatpak_env(Some(OsStr::new("   ")), &missing));
        assert!(is_flatpak_env(Some(OsStr::new("xyz.damt.ocy")), &missing));
        assert!(is_flatpak_env(None, &present));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn background_portal_option_map_builds_expected_keys_and_values() {
        let enable_opts = build_background_portal_options(
            true,
            "ocy_autostart_1_1",
            "Ocy",
            "Start Ocy automatically",
        );
        assert_eq!(
            enable_opts.get("handle_token"),
            Some(&Value::from("ocy_autostart_1_1"))
        );
        assert_eq!(
            enable_opts.get("reason"),
            Some(&Value::from("Start Ocy automatically"))
        );
        assert_eq!(enable_opts.get("autostart"), Some(&Value::from(true)));
        assert_eq!(
            enable_opts.get("dbus-activatable"),
            Some(&Value::from(false))
        );
        assert_eq!(
            enable_opts.get("commandline"),
            Some(&Value::from(vec!["Ocy".to_owned()]))
        );

        let disable_opts = build_background_portal_options(
            false,
            "ocy_autostart_1_2",
            "Ocy",
            "Stop Ocy autostart",
        );
        assert_eq!(disable_opts.get("autostart"), Some(&Value::from(false)));
        assert_eq!(
            disable_opts.get("dbus-activatable"),
            Some(&Value::from(false))
        );
        assert!(!disable_opts.contains_key("commandline"));
    }

    #[test]
    fn background_portal_response_mapping_handles_success_denial_cancel_and_errors() {
        assert!(map_background_portal_response(true, 0, Some(true)).is_ok());
        assert!(map_background_portal_response(true, 0, None).is_ok());
        assert!(map_background_portal_response(false, 0, Some(false)).is_ok());

        let denied = map_background_portal_response(true, 0, Some(false))
            .expect_err("autostart=false while enabling must fail");
        assert!(denied.0.contains("denied"));

        let cancelled =
            map_background_portal_response(true, 1, None).expect_err("response code 1 must fail");
        assert!(cancelled.0.contains("cancelled"));

        let failed =
            map_background_portal_response(false, 2, None).expect_err("response code 2 must fail");
        assert!(failed.0.contains("2"));

        let mut results: HashMap<String, OwnedValue> = HashMap::new();
        results.insert(
            "autostart".to_owned(),
            OwnedValue::try_from(Value::from(false)).expect("owned bool"),
        );
        assert_eq!(extract_portal_autostart_result(&results), Some(false));
        assert!(map_background_portal_results(true, 0, &results).is_err());
        assert!(map_background_portal_results(false, 0, &results).is_ok());
    }

    #[test]
    fn portal_autostart_state_path_and_roundtrip_work() {
        assert_eq!(
            portal_autostart_state_path(
                Some(OsStr::new("/xdg")),
                Some(OsStr::new("/home/u")),
                "Ocy"
            ),
            Some(PathBuf::from("/xdg/ocy/autostart-portal.state"))
        );
        assert_eq!(
            portal_autostart_state_path(Some(OsStr::new("")), Some(OsStr::new("/home/u")), "Ocy"),
            Some(PathBuf::from("/home/u/.config/ocy/autostart-portal.state"))
        );
        assert_eq!(portal_autostart_state_path(None, None, "Ocy"), None);
        assert_eq!(
            portal_autostart_state_path(Some(OsStr::new("/xdg")), None, "   "),
            None
        );

        assert_eq!(format_portal_autostart_state(true), "true\n");
        assert_eq!(format_portal_autostart_state(false), "false\n");
        assert_eq!(parse_portal_autostart_state("true\n"), Some(true));
        assert_eq!(parse_portal_autostart_state("false\n"), Some(false));
        assert_eq!(parse_portal_autostart_state("garbage"), None);

        let dir = temp_dir("portal-state");
        let state_file = dir.join("ocy").join("autostart-portal.state");

        // Missing file defaults to not enabled.
        assert!(!read_portal_autostart_state(&state_file));

        write_portal_autostart_state(&state_file, true).expect("write true state");
        assert!(read_portal_autostart_state(&state_file));

        write_portal_autostart_state(&state_file, false).expect("write false state");
        assert!(!read_portal_autostart_state(&state_file));

        // Corrupt content safely falls back to false.
        std::fs::write(&state_file, "unexpected").expect("write corrupt state");
        assert!(!read_portal_autostart_state(&state_file));

        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- Portal / EWMH / RandR / Tray helpers ---

    #[test]
    fn portal_color_scheme_and_motion_map_correctly() {
        assert_eq!(PortalColorScheme::to_is_light(0), None);
        assert_eq!(PortalColorScheme::to_is_light(1), Some(false));
        assert_eq!(PortalColorScheme::to_is_light(2), Some(true));
        assert_eq!(PortalColorScheme::to_is_light(99), None);

        assert!(!MotionMapper::prefers_reduced(None));
        assert!(!MotionMapper::prefers_reduced(Some(true)));
        assert!(MotionMapper::prefers_reduced(Some(false)));
    }

    #[test]
    fn ewmh_and_monitor_selection_work() {
        assert_eq!(
            ewmh_has_fullscreen(&[10, 20, 30], 20),
            FocusState::FullscreenApp
        );
        assert_eq!(ewmh_has_fullscreen(&[10, 30], 20), FocusState::Normal);
        assert_eq!(ewmh_has_fullscreen(&[0], 0), FocusState::Normal);

        let left = ScreenRect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let right = ScreenRect {
            x: 1920,
            y: 0,
            width: 2560,
            height: 1440,
        };
        let monitors = [left, right];

        // Focused window on right monitor
        assert_eq!(
            select_active_monitor(
                &monitors,
                Some(ScreenRect {
                    x: 2000,
                    y: 100,
                    width: 800,
                    height: 600
                }),
                Some((10, 10))
            ),
            Some(right)
        );
        // No focused window -> pointer fallback on right monitor
        assert_eq!(
            select_active_monitor(&monitors, None, Some((2100, 200))),
            Some(right)
        );
        // Neither -> primary monitor
        assert_eq!(select_active_monitor(&monitors, None, None), Some(left));
    }

    #[test]
    fn tray_icons_decode_to_32x32_argb32_and_sandbox_detection_works() {
        assert_eq!(rgba_to_argb32(&[10, 20, 30, 255]), vec![255, 10, 20, 30]);

        for use_light in [false, true] {
            for state in [
                LinuxTrayIconState::Active,
                LinuxTrayIconState::Paused,
                LinuxTrayIconState::OnBreak,
                LinuxTrayIconState::Attention,
            ] {
                let (w, h, data) =
                    decode_tray_icon_argb32(state, use_light).expect("valid tray PNG");
                assert_eq!((w, h), (32, 32));
                assert_eq!(data.len(), 32 * 32 * 4);
            }
        }

        assert!(!is_sandboxed_env(None, false, None));
        assert!(is_sandboxed_env(Some("xyz.damt.ocy"), false, None));
        assert!(is_sandboxed_env(None, true, None));
        assert!(is_sandboxed_env(
            None,
            false,
            Some("0::/user.slice/user-1000.slice/snap.ocy.ocy.scope")
        ));
    }

    // --- Display backend decision, X11 EWMH atoms, RandR monitors ---

    #[test]
    fn display_backend_and_window_policy_distinguish_x11_wayland_and_headless() {
        let x11_env = IdleEnv {
            display: Some(":0".to_owned()),
            wayland_display: None,
            session_type: Some("x11".to_owned()),
            current_desktop: Some("XFCE".to_owned()),
        };
        assert_eq!(select_display_backend(&x11_env), LinuxDisplayBackend::X11);

        // Even with DISPLAY=:0 for XWayland, Wayland session must select Wayland.
        let wayland_env = IdleEnv {
            display: Some(":0".to_owned()),
            wayland_display: Some("wayland-0".to_owned()),
            session_type: Some("wayland".to_owned()),
            current_desktop: Some("GNOME".to_owned()),
        };
        assert_eq!(
            select_display_backend(&wayland_env),
            LinuxDisplayBackend::Wayland
        );

        let headless_env = IdleEnv::default();
        assert_eq!(
            select_display_backend(&headless_env),
            LinuxDisplayBackend::Headless
        );

        let x11_composited = LinuxWindowPolicy::for_backend(LinuxDisplayBackend::X11, true);
        assert!(x11_composited.can_position_windows);
        assert!(x11_composited.supports_always_on_top);
        assert!(x11_composited.uses_x11_ewmh_hints);
        assert!(!x11_composited.uses_wayland_fullscreen_dim);
        assert!(x11_composited.uses_edge_frames);

        let x11_uncomposited = LinuxWindowPolicy::for_backend(LinuxDisplayBackend::X11, false);
        assert!(!x11_uncomposited.uses_edge_frames);

        let wayland = LinuxWindowPolicy::for_backend(LinuxDisplayBackend::Wayland, true);
        assert!(!wayland.can_position_windows);
        assert!(!wayland.supports_always_on_top);
        assert!(!wayland.uses_x11_ewmh_hints);
        assert!(wayland.uses_wayland_fullscreen_dim);
        assert!(!wayland.uses_edge_frames);

        let headless = LinuxWindowPolicy::for_backend(LinuxDisplayBackend::Headless, false);
        assert!(!headless.can_position_windows);
        assert!(!headless.uses_x11_ewmh_hints);
        assert!(!headless.uses_wayland_fullscreen_dim);
    }

    #[test]
    fn x11_ewmh_atom_lists_and_client_messages_are_packed_correctly() {
        assert_eq!(
            X11_WM_WINDOW_TYPE_NOTIFICATION_ATOM,
            "_NET_WM_WINDOW_TYPE_NOTIFICATION"
        );
        assert_eq!(
            X11_BREAK_STATE_ATOM_NAMES,
            [
                X11_WM_STATE_ABOVE_ATOM,
                X11_WM_STATE_SKIP_TASKBAR_ATOM,
                X11_WM_STATE_SKIP_PAGER_ATOM,
            ]
        );

        // Merges required atoms while keeping existing atoms and stripping 0 / duplicates.
        assert_eq!(
            merge_wm_state_atoms(&[99, 10, 0], &[10, 20, 30, 0]),
            vec![99, 10, 20, 30]
        );
        assert_eq!(merge_wm_state_atoms(&[], &[11, 12, 13]), vec![11, 12, 13]);

        // Packs up to two atoms per _NET_WM_STATE ClientMessage with action=1 (_NET_WM_STATE_ADD)
        // and source=1 (normal application).
        assert_eq!(
            net_wm_state_add_messages(&[101, 102, 103]),
            vec![[1, 101, 102, 1, 0], [1, 103, 0, 1, 0]]
        );
        assert!(net_wm_state_add_messages(&[0, 0]).is_empty());
    }

    #[test]
    fn x11_dpi_workarea_and_monitor_resolution_work() {
        let rm = "*customization:\t-color\nXft.antialias:\t1\nXft.dpi:\t144\nXft.hinting:\t1\n";
        assert_eq!(parse_xft_dpi(rm), Some(144));
        assert_eq!(parse_xft_dpi("Xft.dpi: 120.4\n"), Some(120));
        assert_eq!(parse_xft_dpi("Xft.dpi: 0\n"), None);
        assert_eq!(parse_xft_dpi("Xft.dpi: 9999\n"), None);
        assert_eq!(parse_xft_dpi("Xcursor.size: 24\n"), None);

        let values = [0, 24, 1920, 1056, 0, 0, 3840, 1080];
        assert_eq!(
            parse_net_workarea(&values, 0),
            Some(ScreenRect {
                x: 0,
                y: 24,
                width: 1920,
                height: 1056,
            })
        );
        assert_eq!(
            parse_net_workarea(&values, 1),
            Some(ScreenRect {
                x: 0,
                y: 0,
                width: 3840,
                height: 1080,
            })
        );
        assert_eq!(parse_net_workarea(&values, 2), None);

        let a = ScreenRect {
            x: 1920,
            y: 0,
            width: 2560,
            height: 1440,
        };
        let b = ScreenRect {
            x: 0,
            y: 32,
            width: 4480,
            height: 1368,
        };
        assert_eq!(
            intersect_screen_rect(a, b),
            Some(ScreenRect {
                x: 1920,
                y: 32,
                width: 2560,
                height: 1368,
            })
        );
        assert_eq!(
            intersect_screen_rect(
                a,
                ScreenRect {
                    x: 0,
                    y: 0,
                    width: 1920,
                    height: 1080,
                }
            ),
            None
        );

        let crtcs = [
            RawX11Crtc {
                id: 1,
                rect: ScreenRect {
                    x: 0,
                    y: 0,
                    width: 1920,
                    height: 1080,
                },
                outputs: vec![10],
            },
            RawX11Crtc {
                id: 2,
                rect: ScreenRect {
                    x: 1920,
                    y: 0,
                    width: 2560,
                    height: 1440,
                },
                outputs: vec![20],
            },
        ];
        let resolved = resolve_x11_monitors(&crtcs, Some(20), Some(b), 144);
        assert_eq!(
            resolved,
            vec![
                X11MonitorInfo {
                    id: 1,
                    rect: crtcs[0].rect,
                    work_area: ScreenRect {
                        x: 0,
                        y: 32,
                        width: 1920,
                        height: 1048,
                    },
                    dpi: 144,
                    primary: false,
                },
                X11MonitorInfo {
                    id: 2,
                    rect: crtcs[1].rect,
                    work_area: ScreenRect {
                        x: 1920,
                        y: 32,
                        width: 2560,
                        height: 1368,
                    },
                    dpi: 144,
                    primary: true,
                },
            ]
        );

        // Missing primary output falls back to first monitor.
        let fallback = resolve_x11_monitors(&crtcs, None, None, 0);
        assert!(fallback[0].primary);
        assert!(!fallback[1].primary);
        assert_eq!(fallback[0].dpi, 96);
    }

    #[test]
    fn x11_non_focus_hint_constants_and_restack_target_match_spec() {
        const { assert!(!X11_BREAK_WM_HINTS_INPUT) };
        assert_eq!(X11_BREAK_USER_TIME, 0);

        assert_eq!(x11_restack_above_target(None, 0x600001), None);
        assert_eq!(x11_restack_above_target(Some(0), 0x600001), None);
        assert_eq!(x11_restack_above_target(Some(0x600001), 0x600001), None);
        assert_eq!(
            x11_restack_above_target(Some(0x600008), 0x600003),
            Some(0x600008)
        );
    }
}
