# Releasing e1

> Status: proposed design; there is no public release pipeline yet.
> Last updated: 2026-09-07

This document defines the release contract for the standalone macOS app. The
views embedded in Ginka remain Rust library crates and are not distributed on
their own.

## 1. Distribution decision

The first supported distribution is a universal macOS app in a notarized disk
image attached to a GitHub Release:

```text
e1-v0.1.0-macos-universal.dmg
SHA256SUMS
```

The disk image contains `e1.app` and an Applications-folder shortcut. It runs
on both Apple silicon and Intel. A package installer is unnecessary because the
app has one executable and keeps its mutable data under `~/.e1`; the reader can
install it by dragging it to Applications.

The Mac App Store, Homebrew cask and automatic updates are later distribution
channels, not conditions of the first release. The initial app checks for no
updates and a new version is installed by replacing the app. When automatic
updates are justified, use Sparkle with an Ed25519-signed appcast and add a
signed ZIP artifact; do not make the Developer ID certificate or HTTPS the
update authenticity boundary.

## 2. Trust model

Direct macOS distribution has three separate trust mechanisms:

1. A `Developer ID Application` certificate signs the app and disk image.
2. Apple's notary service scans the signed disk image and issues a ticket. The
   ticket is stapled to the disk image so installation also works when the
   notary service cannot be reached.
3. GitHub serves the artifacts over HTTPS. `SHA256SUMS` lets a reader detect an
   incomplete or substituted download when they obtain the expected digest
   through a trusted path.

Let's Encrypt issues TLS server certificates. It cannot issue an Apple
Developer ID code-signing certificate, submit an app to Apple's notary service,
or make Gatekeeper identify the publisher. It is useful only if e1 later owns a
download website; GitHub Releases already supplies HTTPS.

Unsigned builds remain useful for contributors, but they are development
artifacts. A public build without Developer ID and notarization makes readers
bypass Gatekeeper and is not a supported release.

## 3. One-time Apple setup

Before the first preview release:

- Enrol the publisher in the Apple Developer Program.
- Register and then freeze the bundle identifier. The proposed identifier is
  `com.bokuweb.e1`; changing it later creates a different application identity
  and can also affect Keychain behaviour.
- Create a `Developer ID Application` certificate. Export the identity and its
  private key as a password-protected PKCS#12 file for CI. A `Developer ID
  Installer` certificate is not needed while e1 ships as a DMG rather than a
  signed installer package.
- Create a dedicated App Store Connect API key for notarization and record its
  key ID, issuer ID and private `.p8` key.
- Create a protected GitHub Actions environment named `release`. Limit it to
  version tags and require a maintainer's approval before its secrets become
  available.

The release environment holds these secrets:

| Secret | Contents |
| --- | --- |
| `APPLE_CERTIFICATE_P12` | Base64-encoded PKCS#12 signing identity |
| `APPLE_CERTIFICATE_PASSWORD` | Password for that PKCS#12 file |
| `APPLE_SIGNING_IDENTITY` | Full `Developer ID Application: ... (TEAMID)` name |
| `APPLE_NOTARY_KEY_P8` | Contents of the App Store Connect API private key |
| `APPLE_NOTARY_KEY_ID` | API key ID |
| `APPLE_NOTARY_ISSUER_ID` | API issuer ID |
| `APPLE_TEAM_ID` | Apple developer team ID, used for validation and diagnostics |

The certificate and notary key are different credentials. Keep both out of the
repository and action artifacts. Import the certificate into an ephemeral
keychain on the hosted runner, unlock that keychain only for the signing step,
and delete it in an `always()` cleanup step.

## 4. Bundle contract

Packaging is an explicit repository script rather than hidden in an IDE. It
constructs this bundle:

```text
e1.app/
└── Contents/
    ├── Info.plist
    ├── MacOS/e1
    └── Resources/AppIcon.icns
```

The SVG used inside the UI is not an application icon. An `.icns` with all
required representations must be designed and checked into `assets/macos/`
before the first release.

`Info.plist` is generated from a checked-in template and contains at least:

- `CFBundleIdentifier = com.bokuweb.e1`
- `CFBundleExecutable = e1`
- `CFBundleName` and `CFBundleDisplayName = e1`
- `CFBundlePackageType = APPL`
- `CFBundleShortVersionString = <Cargo package version>`
- `CFBundleVersion = <monotonically increasing CI build number>`
- `LSMinimumSystemVersion = 11.0`
- `CFBundleIconFile = AppIcon`
- `NSHighResolutionCapable = true`

The repository's pinned Rust toolchain and `Cargo.lock` are part of the build
input. CI builds with `--locked` and sets `MACOSX_DEPLOYMENT_TARGET=11.0`
explicitly for both slices. The current arm64 binary already declares macOS
11.0; making it explicit prevents the Intel slice or a future runner image from
silently choosing another floor.

The first release has no entitlements. It needs network and Keychain access but
is not App-Sandboxed, and it has no JIT, debugger, camera, microphone, location
or Apple Events requirement. Add an entitlement only when a concrete feature
needs it, with a signed-bundle smoke test. Hardened Runtime is enabled by
`codesign --options runtime`, with a secure timestamp.

## 5. Version and tag contract

The workspace package version is the source of truth. Stable releases use
three-integer SemVer versions initially; for example, Cargo version `0.1.0`
maps to tag `v0.1.0` and short app version `0.1.0`. A release job refuses a tag
that does not exactly match the Cargo version.

Every release starts as a normal release-preparation pull request that:

1. updates every workspace package version together;
2. updates `Cargo.lock`;
3. moves user-visible changes into a versioned changelog section;
4. passes the ordinary pull-request CI; and
5. records any compatibility or migration note.

After merge, a maintainer creates and pushes the annotated `vX.Y.Z` tag. Tags
are immutable: a failed release is rerun for the same commit, while changed
code receives a new version. The workflow creates a draft GitHub Release; a
maintainer publishes it only after the installation checks pass.

Prerelease version mapping needs a separate decision before the first beta:
Apple's bundle version fields are more restrictive than Cargo SemVer. Do not
invent a mapping inside the workflow.

## 6. Automated release job

Add `.github/workflows/release.yml`, triggered only by `v*` tag pushes and also
available as a dry-run `workflow_dispatch` that cannot access release secrets.
The signed path runs on a GitHub-hosted macOS runner in the protected `release`
environment with `contents: write` and no broader repository permissions.

The job is deliberately linear after compilation because each artifact is the
input to the next trust step:

```text
validate tag/version and clean source
        │
        ├─ cargo test --workspace --locked
        ├─ cargo clippy --workspace --all-targets --locked -- -D warnings
        └─ build release slices (arm64 + x86_64)
                            │
                         lipo -create
                            │
                    construct e1.app
                            │
           sign app (Developer ID + runtime + timestamp)
                            │
              verify signature and both architectures
                            │
                    construct and sign DMG
                            │
               notarytool submit --wait
                            │
                  staple and validate ticket
                            │
          mount DMG; Gatekeeper and launch smoke tests
                            │
           checksum; create draft GitHub Release
```

Use `cargo build --release --locked` for `aarch64-apple-darwin` and
`x86_64-apple-darwin`, then `lipo -create` the two `e1` binaries. Do not build
one slice on a developer machine and the other in CI: both must come from the
tagged tree, pinned toolchain and one controlled job.

Sign nested code first and the app bundle last. e1 initially contains only one
Mach-O, so this is just the executable followed by the bundle; the rule matters
when helpers or frameworks arrive. Avoid `codesign --deep` as a signing
strategy. Verify with `codesign --verify --deep --strict --verbose=2` and inspect
the identity, Team ID, hardened-runtime flag, entitlements and architectures.

Submit the signed DMG with `xcrun notarytool submit --wait` using the API key.
On either success or failure, retain the submission ID and download the notary
log; success with warnings is still actionable. On success, run `xcrun stapler
staple` and `xcrun stapler validate` on the DMG. Mount the final DMG read-only,
run `spctl --assess --type execute` against its app, copy the app to a temporary
directory and launch it once with `E1_DEMO=1`. This smoke test must use the
signed bundle rather than `cargo run`.

Generate `SHA256SUMS` only after stapling, because stapling changes the DMG.
Upload only the final DMG, checksums and release notes. Intermediate unsigned
apps, private keys, temporary keychains and notarization upload archives must
never become workflow artifacts.

## 7. Checks before publishing the draft

CI proves the mechanical contract. A maintainer completes the release by
checking the draft on a second Mac account or machine:

- download the asset from the draft release rather than using the runner copy;
- verify its SHA-256 digest, mount it and drag `e1.app` to Applications;
- confirm Gatekeeper names the expected developer and opens without a bypass;
- sign in through GitHub's device flow, relaunch, and confirm the token is found
  in Keychain;
- open the inbox and one pull request, open a browser link, then sign out;
- repeat a launch without network access to cover the stapled ticket and cached
  startup; and
- on one Intel Mac for the first release, confirm the Intel slice actually runs.

Publish the existing draft after those checks. If notarization or installation
fails, leave the draft unpublished and preserve the notary log in the workflow
log after inspecting it for secrets.

## 8. Failure and rotation rules

- A signing or notarization failure publishes nothing. There is no unsigned
  fallback release.
- A partially created draft is safe to delete; a published tag or release is
  never silently replaced.
- If the Developer ID private key may have leaked, revoke it with Apple, remove
  the GitHub secret, inspect issued/notarized builds, create a new certificate
  and release a newly versioned build.
- If only the notary API key leaks, revoke and replace that key. It cannot sign
  the app, but it still grants access to the team's notarization API.
- Renew Apple Developer Program membership and rotate credentials before they
  expire. Existing validly signed apps can continue to run after a normal
  certificate expiry, but new releases need a current certificate.

## 9. Implementation order

1. Decide the licence (roadmap Q3), publisher identity and final bundle ID.
2. Create the application icon and `Info.plist` template.
3. Add a local packaging script with ad-hoc signing and a DMG smoke test; keep
   all output under `target/dist/`.
4. Add a CI dry run that builds and verifies the universal unsigned/ad-hoc
   artifact without release secrets.
5. Provision Apple and GitHub environment credentials.
6. Add signed tag releases, notarization and the manual publish gate.
7. After the first stable release, decide whether demand justifies Sparkle and
   a Homebrew cask.
