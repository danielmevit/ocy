# App and tray icons

Original artwork of the Ocy mascot, approved by the owner on 2026-11-17/18.
- `app/ocy.ico`: the multi-size Windows icon (16–256, PNG entries) used for the exe, the taskbar and the windows.
  `app/*.svg` are the master and the simplified small-size sources; `app/png/` holds the renders.
- `tray/light/` and `tray/dark/`: `tray-<active|paused|onbreak|attention>.svg` + `png/` (16/20/24/32), only the
  sprout. Light taskbar: saturated fills with deep outlines (paused stem dark). Dark taskbar: the owner-chosen INVERTED set
  (light outline, deep fill). `attention` (clay) is shown only during the long-break warning. The app picks the set
  by the Windows taskbar theme (SystemUsesLightTheme), falling back to the app theme.

Source generator: `D:\Vibe Coding\Ocy\design\icons\gen_icons.py` (outside the repo).
