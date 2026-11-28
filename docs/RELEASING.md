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
  it for an inert binary). For release builds that must not contain build-machine paths:
  - Build from a checkout whose path has **no spaces** (e.g. `~/.cache/ocy-deploy/release-*`;
    keep the checkout outside the working tree you edit). Space-free paths keep `--remap-path-prefix`
    flags and the Windows resource/linker tooling unambiguous.
  - Remap every location the compiler can bake into the binary. Every `FROM` must be non-empty:
    an empty `FROM` makes rustc prefix every *relative* path with the `TO`, and rustc applies the
    *last* matching rule, so the most specific prefixes come last:
    ```sh
    REPO="$HOME/.cache/ocy-deploy/release-ocy"   # no spaces in the path
    cd "$REPO/rust"
    sysroot="$(rustc --print sysroot)"
    sep=$'\037'; flags=""
    add_remap() {
      [ -n "$1" ] || return 0
      if [ -n "$flags" ]; then flags+="$sep"; fi
      flags+="--remap-path-prefix=$1=$2"
    }
    add_remap "${RUSTUP_HOME:-$HOME/.rustup}" rustup   # rustup downloads/toolchains
    add_remap "$REPO" ocy                             # the workspace
    add_remap "${CARGO_HOME:-$HOME/.cargo}" cargo      # the Cargo registry
    add_remap "${CARGO_TARGET_DIR:-$REPO/rust/target}" target
    add_remap "$sysroot" rustup                        # pinned toolchain's std sources
    export CARGO_ENCODED_RUSTFLAGS="$flags"
    OCY_POSTHOG_KEY=<key> cargo xwin build --release -p ocy-app --features store --target x86_64-pc-windows-msvc
    ```
  - Gate the result the same way CI does: `python3 rust/tools/path_leak_check.py
    <target-dir>/x86_64-pc-windows-msvc/release/Ocy.exe` exits non-zero if the binary still
    contains `runneradmin`, `/home/`, `/Users/` or `C:\Users`.
- **Rotating the key.** Reset the project API key in PostHog (project settings) → update the
  `OCY_POSTHOG_KEY` repository secret → the next release picks it up. Already-released builds
  keep the old key baked in, so their events stop being accepted once PostHog revokes it;
  users get the new key with their next Ocy update.
