# Code signing policy: MaxApps Hub (desktop)

This policy describes how the Windows program of MaxApps Hub is built, signed and released.

- **Source code:** https://github.com/maxiusofmaximus/MaxAppsHub (directory `desktop/`)
- **License:** GNU General Public License v3.0 (`LICENSE` in the repository root)
- **Signing service:** SignPath Foundation, which holds the certificate. Released programs carry the
  publisher name the Foundation issues.

## Roles

- **Author and maintainer:** maxiusofmaximus. Writes and reviews every change, and publishes the releases.
- **Approver:** maxiusofmaximus. Approves each signing request in SignPath, with multi-factor
  authentication, only after checking the tagged source and the build log of that release.

The project has a single maintainer today. No other person can approve a signing request.

## What gets signed

Only the program built from a version tag (`desktop-v<version>`) by the GitHub Actions workflow
`.github/workflows/desktop-release.yml`. Nothing built on a developer machine is signed. Each signing
request is approved by hand, one release at a time.

## Release process

1. The maintainer creates the tag `desktop-v<version>` on a commit of the public repository.
2. GitHub Actions builds the program from that commit with `cargo build --release --locked`.
3. The unsigned program goes to SignPath, which waits for the approver.
4. The approver checks the tag and the build log, then approves the request.
5. The signed program is signed again with the project's update key (Ed25519). The file `<program>.sig`
   is published next to it.
6. The GitHub release gets the program and its `.sig`. Pre-releases are marked as such.

The Hub installs an update of itself only when its `.sig` verifies against the public key built into
the program. A release without a valid signature is refused.

## Privacy and network use

The program has no telemetry and sends no personal data. It makes requests only to the GitHub API
(`api.github.com`) and to GitHub download addresses, to read release lists and download installers.
Its settings and its activity log stay in `%LOCALAPPDATA%\MaxAppsHub` on the user's computer.

## Vulnerabilities and problems

Report security problems through a private GitHub security advisory on the repository, or by
opening an issue that does not contain exploit details. A release that must be withdrawn is marked
as a pre-release or deleted from the GitHub releases page.
