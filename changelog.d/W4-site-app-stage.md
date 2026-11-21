# W4-site-app-stage

- Website "The real app": the tabs no longer jump when clicked. The section was vertically centred, so each scene's height re-centred the tab column. It is now top-aligned, the stage has a fixed size, and the note sits under the stage.
- One laptop + monitor stage, drawn in CSS, replaces the per-tab monitors. The screens crossfade per tab (instant with reduced motion):
  - short break: the card on the laptop, with a soft green frame on the monitor
  - long break: the laptop dims, with the long-break card
  - warning: the warning card
  - settings: the Settings window
- Four geometric tab icons that match the Settings timeline legend (short = hollow circle, long = filled rounded square, warning = round clock, settings = round slider knobs). Phones get a stable 2×2 tab grid.
