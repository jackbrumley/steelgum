# Steelgum release and updates

Steelgum follows the shared [App Standard](https://github.com/jackbrumley/jackbrumley.github.io/blob/main/APP_STANDARD.md)
and [Release and Update Standard](https://github.com/jackbrumley/jackbrumley.github.io/blob/main/RELEASE_STANDARD.md),
adapted to a native Rust desktop application. The updater, installers,
packaging recipes and project page live in this repo. The first stable
release and live site still need deployment and installation verification.
The shared checkout used here was `d551587` from 16 November 2026. GitHub
DNS prevented fetching its current remote head from this workspace.

## Site and identity

Use `https://brumley.info/steelgum/` as the initial public install page and
installer script location. Follow the shared install page design and app
catalogue. If the release repository is in the same GitHub account as the
portfolio site, `main:/docs` can supply the project Pages site. If the release
repository is in the Steelgum organisation, publish the `/steelgum/` paths
from the portfolio account using the verified release content. Project Pages
only inherits a custom domain from the same account. Confirm the canonical
release repository, Pages routing and exact installer URLs before shipping
a build that uses them. [GitHub Pages domain inheritance](https://docs.github.com/en/pages/configuring-a-custom-domain-for-your-github-pages-site/about-custom-domains-and-github-pages#using-a-custom-domain-across-multiple-repositories)

If the maintainer secures `steelgum.org`, it can become the public site.
Keep serving the original HTTPS installer URLs for installed clients when
the site moves. The first updater downloads only from those URLs.

The current packages and native window use `info.brumley.steelgum`. Confirm
that identity before the first release and preserve it afterward. The
`steelgum` config directory respects
`XDG_CONFIG_HOME` on Linux and `%USERPROFILE%\.config\steelgum` on Windows.
Keep project data in the project, separate from application settings and logs.

## Update design

1. Read the canonical GitHub repository's `releases/latest` API. Compare the
   stable release tag with the installed version. The website hosts the
   installers and user-facing copy. GitHub Releases owns version discovery and
   assets, as it does for the other apps.
2. Packaged installs check in the background and log failures. F6 runs a
   manual check and a native dialog shows the result and last check time.
   The app offers release notes and asks before installing any update.
3. Use the same installer for a fresh install and an update. Verify each
   package against its SHA-256 release-asset digest before modifying the
   installation. Preserve the installation variant, settings and projects.
4. Hand off to a separate updater process before exiting the desktop app.
   Relaunch on success. On failure, reopen the installed version if it remains
   available and report the failure with the location of `debug/update.log`.
   The updater waits for the running process to exit before changing files.
   One module resolves the application's update log location.

This updater applies to an installed Steelgum desktop application. Games in
source repositories choose their Steelgum dependency with Cargo and retain
their own release process. Updating the desktop tooling must not rewrite a
game's declared dependency or its authored project files.

## Linux packages

Build the x64 `.deb`, `.rpm` and AppImage from one staged desktop binary,
launcher and icon on Ubuntu 22.04. `scripts/release-assets.mjs` selects the
Cargo manifest for both the Linux and Windows builds. The selected binary
must be named `steelgum` and report `steelgum X.Y.Z` with `--version`. When the
desktop target changes, verify its linked and runtime-loaded libraries on the
supported distributions and run the installed application on Wayland and X11.

The default Linux installer uses `.deb` on Mint, Ubuntu and supported Debian
versions or `.rpm` on Fedora. `--system` requires a native package and
administrator authentication. `--appimage` installs without administrator
access. Older releases with only an AppImage still work with the default
installer. A successful system install removes the current user's AppImage
launcher, icon and executable so they cannot shadow the system package.

F6 updates use the same format as the installed application. For system
packages, the update downloads the verified GitHub Release asset and gives it
to the package manager. The uninstall script supports both formats and keeps
settings, projects and logs. Use `--appimage` or `--system` to remove just that
format when both are present.

## Before the first public release

- Choose the open-source licence, update the RPM `License` field, and verify
  the redistribution terms for packaged dependencies and shipped assets.
- Establish `dev`, `main` and the canonical release repository. The code
  currently uses the checkout's `jackbrumley/steelgum` remote. If the project
  moves to the organisation, update the release workflow, installers,
  updater and site together before tagging.
- Use plain `X.Y.Z` versions and `vX.Y.Z` tags. GitHub CI builds and verifies
  all four assets from a tagged prerelease. Linux jobs exercise AppImage
  installs, migration to `.deb` and `.rpm`, native reinstalls and removal.
  Promote only after those jobs and the release notes pass.
- Run the graphical release candidate on Linux and Windows and verify the F6
  dialogs, background check, restart, and Open Update Log. The general desktop
  session log, crash reporting and diagnostics UI remain first-release work.
- Test fresh install, re-install and update on Linux and Windows. Verify
  failure and relaunch behaviour, including a failed digest check and paths
  with spaces. Check the real site and updater URLs before promotion.

## Release procedure

Set up the canonical `main` and `dev` branches and enable GitHub Pages from
`main:/docs` before first promotion. Run the Release workflow manually from
GitHub to build test packages without creating a release.

From a clean `dev` checkout, use:

```sh
npm run release:prepare -- X.Y.Z
git push origin dev vX.Y.Z
npm run release:verify
mkdir -p release-artifacts
npm run --silent release:notes > release-artifacts/release-notes.md
```

Wait for the tag's release workflow to finish before verification. Fill in
the notes, keeping the generated Downloads block, and upload the notes to
the prerelease. Run `npm run release:promote` only with the maintainer's
authorisation. It requires the tag workflow to have passed, validates the
release assets and notes, advances `main`, and verifies the served installer
files before it makes the release visible to updaters.
