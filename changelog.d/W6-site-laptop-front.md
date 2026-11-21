# W6-site-laptop-front

- Website "The real app": the laptop is now a front-view cut-out (transparent, no desk or background) on the page
  background, with a soft contact shadow. The app screens are still composited onto its display, with the glass edge
  and the per-tab crossfade. The W5 photo assets were removed.
- Ships as AVIF/WebP with alpha at 800 and 1600 px (at most 12 KB). The tabs still never move.
- Also fixes a stray `}` in global.css left by W5.
