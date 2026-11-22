Ocy for Linux (x86_64 tarball)
================================

Run
---
Extract this archive anywhere and run `./Ocy`:

  ./Ocy

Needs
-----
Ocy loads its X11, Wayland, and font libraries at runtime (dlopen), so the host
must provide them. Desktop distributions ship all of these by default; a plain
server or minimal container may not.

  Debian/Ubuntu:
    sudo apt-get install libxkbcommon0 libxkbcommon-x11-0 libxcb1 \
      libx11-6 libx11-xcb1 libxcursor1 libxi6 libwayland-client0 libfontconfig1

  Fedora:
    sudo dnf install libxkbcommon libxkbcommon-x11 libxcb \
      libX11 libX11-xcb libXcursor libXi libwayland-client fontconfig

  Arch:
    sudo pacman -S --needed libxkbcommon libxcb libx11 libxcursor libxi \
      wayland fontconfig

(Arch ships libxkbcommon-x11.so.0 in `libxkbcommon` and libX11-xcb.so.1 in
`libx11`; Fedora and Debian split them into the packages listed above.)

Display server support
----------------------
Ocy runs on both **X11** and **Wayland**:
- **X11:** Full support (active-monitor break placement, non-activating always-on-top
  break cards, edge frames on secondary monitors when a compositor is running,
  XScreenSaver idle detection, and EWMH fullscreen deferral).
- **Wayland:** Reduced protocol support. Wayland does not allow clients to inspect
  other windows or force windows above other apps, so fullscreen deferral,
  secondary-monitor edge frames, and always-on-top break cards are unavailable on
  GNOME and KDE Wayland.

System tray requirements
------------------------
Ocy registers a StatusNotifierItem (SNI) tray icon and menu:
- **KDE Plasma / XFCE / desktops with an SNI host:** works natively.
- **GNOME:** stock GNOME does not include a tray host; install the
  AppIndicator and KStatusNotifierItem Support extension:
  https://extensions.gnome.org/extension/615/appindicator-support/

If no tray host is available, launching `Ocy` a second time signals the running
instance to open Settings.

Desktop entry & icon (optional)
-------------------------------
`xyz.damt.ocy.desktop` and the 256x256 icon (`xyz.damt.ocy.png`) are included if
you want an application menu entry:

  mkdir -p ~/.local/share/applications ~/.local/share/icons/hicolor/256x256/apps
  cp xyz.damt.ocy.png ~/.local/share/icons/hicolor/256x256/apps/xyz.damt.ocy.png
  cp xyz.damt.ocy.desktop ~/.local/share/applications/xyz.damt.ocy.desktop

Update `Exec=` in `~/.local/share/applications/xyz.damt.ocy.desktop` to the full
path of `Ocy` if it is not on your `PATH`.

Autostart
---------
- Enable **Start with system** inside Ocy Settings to write
  `~/.config/autostart/xyz.damt.ocy.desktop` automatically, **or**
- Copy `xyz.damt.ocy.desktop` to `~/.config/autostart/xyz.damt.ocy.desktop` with
  `Exec=/path/to/Ocy --autostart`.

Data & configuration
--------------------
Settings, break state, statistics, and logs are stored in:
  $XDG_CONFIG_HOME/ocy  (default: ~/.config/ocy)

License
-------
Ocy is licensed under AGPL-3.0-only (see `LICENSE.txt` and `THIRD-PARTY-NOTICES.txt`).
