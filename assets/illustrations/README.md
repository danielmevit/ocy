# Exercise illustrations

Final assets used by the app: original hand-drawn mascot artwork (v1f style), Ocy green palette, light and dark
sticker treatment. Eyes are single irregular solid black dots only (owner: no white, no outlines).
- `ocy-v1f/svg/<exercise>-<light|dark>-f<N>.svg`: frames with viewBox 0 0 64 64. Plain shapes only.
- `ocy-v1f/manifest.json`: frame count, stepped sequence (1-based), step duration and the frame shown when animations
  are off.

The source generator and the design studies live outside the repo, in `D:\Vibe Coding\Ocy\design\` (the owner's
design folder). The app's XAML resources are generated from these files by `tools/illustrations/`. Never edit the
generated XAML by hand.
