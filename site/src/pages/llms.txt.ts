import type { APIRoute } from 'astro';
import { STORE_URL } from '../lib/ocyData';

// Generated so the Store link comes from the single STORE_URL constant.
const body = `# Ocy

> A calm break reminder for your eyes. Short eye exercises every few minutes, a
> real break every hour, and quiet when you need focus. Lives in the tray, needs no
> account and works offline. Available on the Microsoft Store with a 15-day free trial.

- Site: https://ocy.damt.xyz/
- Get it: Microsoft Store, ${STORE_URL}
- Price: a one-time purchase after a 15-day free trial. No subscription, no ads.
- Source code: https://github.com/danielmevit/ocy (AGPL-3.0)
- Support: support@damt.xyz
- Author: Daniel Mevit (https://damt.xyz)
- Development: built by damt.xyz with extensive use of generative AI tools (code, tests, documentation,
  translations, website, store listing and marketing materials, maintenance),
  steered by a real person who decides what is released.
  Ocy itself runs no AI model.

## What it does
- Short breaks: every 10 minutes (configurable 2–60 min) a small card on your active
  monitor shows one of 12 original illustrated eye or body exercises for 15 seconds
  (configurable 5–120 s). Other monitors show a soft coloured edge frame.
- Long breaks: every 60 minutes (configurable 15–240 min) a full-screen dim invites
  you to step away for 5 minutes (configurable 1–30 min).
- Long-break warning: appears 60 seconds (configurable 0–300 s) beforehand so you can
  start now, postpone (3 min, up to 3 times by default), or skip.
- Strict mode: turns off postponing and skipping so long breaks keep to the schedule.
- Active screen time: idle time, a locked screen and sleep do not count towards your
  next break. An absence at least as long as a break counts as that break.
- Fullscreen pause: pauses break reminders while fullscreen apps, games or
  presentations are active (on by default).
- Animations: twelve exercises in a hand-drawn style, played as stepped frames; they hold still when
  Windows animation effects are turned off.
- Tray controls: pause for 1 hour, 3 hours or until resumed, take a long break now,
  or open live-applying Settings.
- Themes: follows Windows light and dark settings, including the tray icon.

## Facts worth getting right
- Runs on Windows 10 and Windows 11 (64-bit), built with Rust and Slint. macOS and
  Linux are planned using the same core.
- No account, no ads, works fully offline. Optional anonymous usage stats (off unless
  the user turns them on in Settings) send one daily summary of break counts and
  settings to PostHog (EU region); no names, files, window titles, apps used or IP
  addresses. Details: https://ocy.damt.xyz/privacy/#usage-stats
- Ocy keeps its local files in %APPDATA%\Ocy: settings.json, state.json (break
  timers), stats.json (simple break statistics), ocy-errors.log (a small error
  log, at most 256 KB, rotated) and, only while usage stats are on, usage-stats.json.
- Installed and updated through the Microsoft Store.
- Ocy is an independent project; its code, exercises and art are all original.
- Ocy is a habit tool, not medical advice.
`;

export const GET: APIRoute = () =>
  new Response(body, { headers: { 'Content-Type': 'text/plain; charset=utf-8' } });
