# Vendored tmux

Ryve ships its own tmux binary so Hand and Head sessions behave identically on
every install, regardless of whether the user has tmux installed, which version,
or how it is configured.

## Pinned version

The pinned tmux version is recorded in `vendor/tmux/VERSION`. The current
version is **3.5a**.

## Building the vendored binary

Run the build script from the repo root:

```sh
./scripts/build-vendored-tmux.sh
```

This downloads the pinned tmux release from GitHub, compiles it from source,
and places the binary at `vendor/tmux/bin/tmux`. The `bin/` directory is
git-ignored.

### Prerequisites

**macOS:**

```sh
brew install autoconf automake libevent pkg-config
```

**Linux (Debian/Ubuntu):**

```sh
sudo apt-get install build-essential autoconf automake libevent-dev libncurses-dev bison pkg-config
```

## How to bump the version

1. Edit `vendor/tmux/VERSION` to the new version tag (e.g. `3.6`). The tag
   must match a GitHub release at `https://github.com/tmux/tmux/releases`.
2. Run `./scripts/build-vendored-tmux.sh` to verify the new version builds.
3. Run `cargo test -p ryve` to confirm the `PINNED_TMUX_VERSION` constant
   compiles correctly.
4. Commit `vendor/tmux/VERSION` with a message like
   `chore: bump vendored tmux to 3.6`.

## Runtime resolution

The Rust function `bundled_tmux::bundled_tmux_path()` resolves the tmux binary
at runtime:

1. **Installed layout:** `<exe_dir>/bin/tmux` — used in the macOS `.app` bundle
   and Linux tarball.
2. **Development layout:** `<repo>/vendor/tmux/bin/tmux` — used during
   `cargo run` after running the build script.

The function returns `None` if neither path exists on disk.

## Packaging

When building a distributable artifact, the packaging step must copy
`vendor/tmux/bin/tmux` (or a freshly-built binary) into the correct location:

| Format          | tmux path                          |
|-----------------|------------------------------------|
| macOS `.app`    | `Ryve.app/Contents/MacOS/bin/tmux` |
| Linux tarball   | `ryve/bin/tmux`                    |

## Non-goals

- **Windows:** tmux is not shipped on Windows.
- **User-swappable:** Users cannot substitute a custom tmux build.
