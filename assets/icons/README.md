# App and tray icons

Original artwork of the Ocy mascot, approved by the owner on 2026-11-17/18.
- `app/ocy.ico`: the multi-size Windows icon used for the exe, the taskbar and the windows. Owner decision 2026-11-18:
  **16/20/24/32 = the brand sprout** (`brand/png/ocy-sprout-light-*`; the face is illegible that small) and **40–256 = the
  mascot face**. The previous all-face icon is kept as design/icons/app/ocy-face-all-sizes.ico (outside the repo).
  `app/*.svg` are the master and the simplified small-size sources; `app/png/` holds the renders.
- `tray/light/` and `tray/dark/`: `tray-<active|paused|onbreak|attention>.svg` + `png/` (16/20/24/32), only the
  sprout. Light taskbar: saturated fills with deep outlines (paused stem dark). Dark taskbar: the owner-chosen INVERTED set
  (light outline, deep fill). `attention` (clay) is shown only during the long-break warning. The app picks the set
  by the Windows taskbar theme (SystemUsesLightTheme), falling back to the app theme.

Source generator: `D:\Vibe Coding\Ocy\design\icons\gen_icons.py` (outside the repo).
- `brand/ocy-sprout-<light|dark>.svg` + `brand/png/` (16–256): **the Ocy brand mark** (owner 2026-11-18): the "on break"
  sprout form (open leaves) WITHOUT the + sign, re-centred. Used for small app-icon sizes, the website header logo and
  the favicon. `-light` = for light backgrounds, `-dark` = the inverted set for dark backgrounds.
