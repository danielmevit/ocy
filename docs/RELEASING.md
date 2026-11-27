# Releasing Ocy

How release artifacts are built and what they carry. CI-only today; the owner triggers releases.

## Usage stats key (`OCY_POSTHOG_KEY`)

Anonymous, opt-in usage stats are compiled into **release builds only**, through the
PostHog project API key read at build time by `option_env!("OCY_POSTHOG_KEY")` in
`rust/apps/ocy/ocy-app/src/usage_stats.rs`. `build_key()` there treats an absent or empty
key as `None`, which disables the whole feature: no thread, no network, no Settings toggle.

- **Where the key lives.** In the GitHub **repository secret** `OCY_POSTHOG_KEY`
  (repo Settings → Secrets and variables → Actions). The owner adds it; it may not exist yet.
  The four release-artifact CI jobs pass it as a step-level `env` on exactly their build steps:
  - `rust.yml` → `windows-release` → "Build release binary"
  - `linux-smoke.yml` → `tarball` → "Build release binary" (the smoke job's build does not get it)
  - `macos-package.yml` → "Build macOS app" (`build_app.sh` inherits the environment)
  - `linux-flatpak.yml` → a step before "Build Flatpak bundle" that, only when the secret is
    non-empty, writes the key into the `ocy` module's `build-options.env` of the runner's
    workspace copy of `packaging/linux/xyz.damt.ocy.yml` (flatpak-builder does not pass host
    env into the sandbox). That edit never touches the committed manifest.
- **Never committed.** The key exists only in the repository secret and in the CI build
  environments. It is never echoed, never placed in artifacts or logs, and never written to
  this repository. Test, smoke, and PR builds never see it (forks get no secrets anyway).
- **Local Store builds.** Set the same environment variable before building:
  `OCY_POSTHOG_KEY=<key> cargo build -p ocy-app --release --features store` (or build without
  it for an inert binary). For release builds that must not contain build-machine paths, remap
  the Cargo registry, workspace, and rustup sources too:
  `OCY_POSTHOG_KEY=<key> CARGO_ENCODED_RUSTFLAGS="$(printf '%s\037%s\037%s\037%s' "--remap-path-prefix=$CARGO_HOME=cargo" "--remap-path-prefix=/mnt/d=ocy" "--remap-path-prefix=$CARGO_TARGET_DIR=target" "--remap-path-prefix=$HOME/.rustup=rustup")" cargo xwin build --release -p ocy-app --features store --target x86_64-pc-windows-msvc`
- **Rotating the key.** Reset the project API key in PostHog (project settings) → update the
  `OCY_POSTHOG_KEY` repository secret → the next release picks it up. Already-released builds
  keep the old key baked in, so their events stop being accepted once PostHog revokes it;
  users get the new key with their next Ocy update.
