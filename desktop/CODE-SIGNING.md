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
2. GitHub Actions builds the program from that commit with `cargo build --release --locked`, puts
   it through SignPath when that is set up, and creates a **draft** release with the program. Drafts
   are invisible to the Hub.
3. The maintainer checks the tag and the build log, then signs the program **on the maintainer's
   computer** with the update key (Ed25519), as described below. The key never leaves that computer
   and is never sent to GitHub.
4. The maintainer uploads `<program>.sig` to the draft and publishes it. Pre-releases are marked as such.

The Hub installs an update of itself only when its `.sig` verifies against the public key built into
the program. A release without a valid signature is refused.

### Signing on the maintainer's computer

Run in Git Bash. `KEY` is the private key kept outside the repository; `TAG` is the release tag.

```sh
KEY="/g/Mi unidad/MaxAppsHub-firma/maxapps-hub-update-signing-key.pem"
TAG=desktop-v1.0.2
VER=${TAG#desktop-v}
DIR=$(mktemp -d)
gh release download "$TAG" -p "MaxAppsHub-$VER-desktop.exe" -D "$DIR"
EXE="$DIR/MaxAppsHub-$VER-desktop.exe"
openssl pkeyutl -sign -inkey "$KEY" -rawin -in "$EXE" -out "$DIR/sig.bin"
od -An -tx1 "$DIR/sig.bin" | tr -d ' \n' > "$EXE.sig"
openssl pkey -in "$KEY" -pubout -out "$DIR/pub.pem"
openssl pkeyutl -verify -pubin -inkey "$DIR/pub.pem" -rawin -in "$EXE" -sigfile "$DIR/sig.bin"
gh release upload "$TAG" "$EXE.sig"
gh release edit "$TAG" --draft=false
rm -rf "$DIR"
```

The key is protected by a passphrase kept by the maintainer outside the computer's files. openssl asks
for it in the terminal each time it is used. The `verify` line must print `Signature Verified Successfully`
before the upload. The program is the
one the draft holds, so the signature matches the file the Hub downloads. Before publishing, compare
the program's SHA-256 with the build log of the workflow run.

## Privacy and network use

The program has no telemetry and sends no personal data. It makes requests only to the GitHub API
(`api.github.com`) and to GitHub download addresses, to read release lists and download installers.
Its settings and its activity log stay in `%LOCALAPPDATA%\MaxAppsHub` on the user's computer.

## Vulnerabilities and problems

Report security problems through a private GitHub security advisory on the repository, or by
opening an issue that does not contain exploit details. A release that must be withdrawn is marked
as a pre-release or deleted from the GitHub releases page.
