# The mabel app

A [Tauri](https://tauri.app) v2 app that is a wallet node with the wallet UI in
front of it, built for macOS and iOS. It is the same node and the same React app
as `mabel serve` plus a browser, packaged as one process the user can
double click.

## How the in-process node works

The rust side runs the node inside the app process. On start,
`app/src-tauri/src/node.rs` calls `mabel_node::NodeRuntime::start`, the same
entry point `mabel serve` uses, over a node home in the app's data directory.
That binds the Iroh endpoint, the sync server and the HTTP API, then the app opens its window on `http://127.0.0.1:<port>/wallet` and the
node serves both the JSON API and the UI bundle from there.

Three consequences worth knowing:

- The HTTP listener always takes an ephemeral port on `127.0.0.1`, so the app
  never collides with another copy of itself or with a `mabel serve` on
  the default port 9080.
- The API's loopback rules require `Host` to be `127.0.0.1:<port>` or
  `localhost:<port>`, and require an `Origin` that matches on mutating routes. A
  webview loading `http://127.0.0.1:<port>/wallet` sends both by itself, so the
  app needs no exception to those rules.
- The app registers no tauri command and grants the page no capability. The UI
  reaches the node over HTTP exactly as it does in a browser, so the page cannot
  reach tauri even though it is loaded in a webview.

The UI comes from the bundle `mabel-node` compiles in from `ui/dist`, so the app
has no frontend build of its own. `ui/dist` must exist before the app is
compiled, or the node answers `ui_not_built` on the UI paths. A debug build reads
`ui/dist` from that path at runtime; a release build embeds it.

`app/dist/index.html` is the only page the app ships. It is what the window opens
on when the node could not start, and it is what `build.frontendDist` in
`tauri.conf.json` points at.

## Where the node home lives

The app keeps one home under its own data directory, in a `node` subdirectory. It
does not share the CLI's default `~/.mabel`.

| Platform | Path |
|---|---|
| macOS | `~/Library/Application Support/dev.reamde.mabel/node` |
| iOS | `Library/Application Support/dev.reamde.mabel/node` inside the app's sandbox container |
| Linux (dev only) | `~/.local/share/dev.reamde.mabel/node` |

The layout is the node home layout of proposal 001 section 8, so the CLI can read
and write the same home while the app is closed:

```sh
mabel --home ~/Library/Application\ Support/dev.reamde.mabel/node identity list
```

The home is created on first run as a wallet with the n0 relays enabled. A
`node.key` that other users can read is refused, which is the same code 60 the
CLI returns.

## Running it in development

```sh
(cd ui && npm ci && npm run build)                      # writes ui/dist
cargo install tauri-cli --version 2.11.4 --locked
(cd app/src-tauri && cargo tauri dev)
```

macOS needs Xcode's command line tools. Linux needs webkit2gtk and gtk3
(`libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev
librsvg2-dev` on Debian and Ubuntu); the Linux build is for development only and
ships nowhere.

`app/src-tauri` is its own cargo workspace, so the root `cargo build --workspace`
and `cargo test --workspace` never build tauri, and this crate has its own
`Cargo.lock` and its own `target/`.

The node embedding is behind no feature and the webview glue is behind the
default `tauri-app` feature, so a machine with no webview toolkit can still check
and test everything that matters:

```sh
cd app/src-tauri
cargo test --no-default-features     # boots the node on a temp home, asks GET /api/node
cargo check                          # the whole app, webview included
cargo clippy --all-targets -- -D warnings
```

## What CI builds

Two workflows build this app. Every job builds `ui/dist` first, because the node
embeds it.

| Workflow | Job | When | Artifact |
|---|---|---|---|
| [release.yml](../.github/workflows/release.yml) | macOS app | every push to `main`, from the release tag | `mabel-app-<version>-macos.dmg` and `mabel-app-<version>-macos.zip`, attached to the GitHub release |
| [app.yml](../.github/workflows/app.yml) | Node embedding checks | every push to `main` | none: `cargo fmt --check`, `cargo clippy --no-default-features`, `cargo test --no-default-features` on ubuntu |
| [app.yml](../.github/workflows/app.yml) | macOS app | `workflow_dispatch` only | `mabel-app-<shortsha>-macos`: the same two files as a workflow artifact |
| [app.yml](../.github/workflows/app.yml) | iOS simulator app | every push to `main` | `mabel-app-<shortsha>-ios-simulator`: the simulator `.app` as a zip |

The release build is the canonical macOS build, so `app.yml` does not build the
app on a push: a second signed build would cost another 15 minutes of macOS
runner for a bundle nobody downloads. Dispatch the `app.yml` macOS job to build
the app off a branch before it lands. Both jobs call the same composite action,
[.github/actions/macos-app](../.github/actions/macos-app/action.yml), so what a
dispatch proves is what a release ships.

With no Apple secrets set both builds are unsigned, and Gatekeeper refuses an
unsigned download; the action logs a notice saying so and naming the secrets it
did not find. The signed builds are described in [Signing](#signing) below.

The build also checks that the bundle reports the version in the file names, by
reading `CFBundleShortVersionString` out of the built `Info.plist`. Tauri writes
that key from `version` in `tauri.conf.json`, and the release commit is what
writes `tauri.conf.json`.

The Xcode project is generated by `cargo tauri ios init` on every run and is not
committed. It holds absolute paths from the machine that generated it, and it is
derived from `tauri.conf.json`, so a committed copy would only be a second place
to keep the same facts.

The release workflow attaches the `.dmg` and the `.app` zip to the GitHub
release itself, so publishing a build takes no manual step. To publish what a
dispatched `app.yml` run built instead:

```sh
gh run download <run-id> --name mabel-app-<shortsha>-macos
gh release upload <tag> mabel-app-<version>-macos.dmg
```

## Signing

Signing is off until the secrets exist. Each workflow reads them, decides in a
check step whether it has all of them, and either signs or logs a notice naming
what was missing. The artifact names never change.

The same six secrets also sign the `mabel` CLI binary in the macOS tarball that
`release.yml` publishes. That is a plain `cargo build` with no bundler, so the
release workflow runs `codesign` and `notarytool` itself rather than through
tauri, and it staples nothing: a bare executable has nowhere to keep a
notarization ticket, so Gatekeeper looks that one up over the network. The
`.dmg` and the `.app` are what carry a ticket with them.

The bundle identifier to register with Apple is `dev.reamde.mabel`. It comes
from `identifier` in `tauri.conf.json` and must match the App ID on
developer.apple.com and the app record in App Store Connect exactly.

### The secrets

All seven are repository secrets under Settings, Secrets and variables, Actions.

| Secret | Where it comes from |
|---|---|
| `APPLE_CERTIFICATE` | A Developer ID Application certificate. Create it under developer.apple.com, Certificates, Identifiers and Profiles, then export it from Keychain Access as a `.p12` with its private key and base64 it: `base64 -i cert.p12 \| pbcopy` |
| `APPLE_CERTIFICATE_PASSWORD` | The password typed during that `.p12` export |
| `APPLE_SIGNING_IDENTITY` | The certificate's full name, as `security find-identity -v -p codesigning` prints it, for example `Developer ID Application: Jane Doe (A1B2C3D4E5)` |
| `APPLE_API_ISSUER` | App Store Connect, Users and Access, Integrations, App Store Connect API. The Issuer ID sits above the key table and is a UUID |
| `APPLE_API_KEY` | The Key ID of a key in that table, a ten-character string |
| `APPLE_API_KEY_CONTENT` | The whole `AuthKey_<keyid>.p8` file that App Store Connect offers once when the key is created, `BEGIN PRIVATE KEY` and `END PRIVATE KEY` lines included |
| `APPLE_TEAM_ID` | The ten-character Team ID from developer.apple.com, Membership details. Only the iOS job reads it |

Two things about the App Store Connect key decide whether any of this works.

- It has to be a **Team key**, from the Team Keys tab. An Individual key cannot
  reach notarytool or the provisioning endpoints at all.
- Creating and downloading certificates and profiles over the API needs the
  **Admin** role. The iOS path does that, so an Admin team key covers
  notarization, iOS cloud signing and the TestFlight upload with one key.

Do not add `APPLE_ID` or `APPLE_PASSWORD`. Tauri prefers those over the API key
when they are set, and then also demands `APPLE_TEAM_ID`, so adding them
silently moves notarization onto the app-specific-password route.

### What the secrets turn on

Nothing is partial. Both macOS builds want six of them, `APPLE_CERTIFICATE`,
`APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_API_ISSUER`,
`APPLE_API_KEY` and `APPLE_API_KEY_CONTENT`, and with any one of them missing
they build unsigned. There is no signed but un-notarized middle state on purpose:
Gatekeeper refuses that download exactly as it refuses an unsigned one.

With all six present, `cargo tauri build` does the app work itself. Tauri imports
the `.p12` into a keychain it creates and deletes on its own, signs the `.app`
with the identity under the hardened runtime, submits it to notarytool with the
API key, and staples the ticket. The composite action adds two steps around that:
it notarizes and staples the `.dmg`, which tauri signs but does not notarize, and
it runs `codesign --verify` and `stapler validate` over both so a run that
produced an unusable bundle fails rather than uploads.

The CLI binary has no bundler to do any of that, so `release.yml` creates a
keychain under `RUNNER_TEMP` with a throwaway password, imports the `.p12`,
signs the binary with `--options runtime --timestamp`, submits a zip of it to
notarytool, and deletes the keychain in a step that runs even when the build
failed.

`APPLE_API_KEY_CONTENT` exists because tauri has no variable for the key body. It
only reads `APPLE_API_KEY_PATH`, so the key is written to a file under the
runner's temp directory, outside the workspace where no artifact can pick it up,
and deleted at the end of the job.

### The iOS switch

The iOS distribution path is off even when every secret is set, because turning
it on changes state on the Apple account. Two repository variables, not secrets,
control it. A variable can be read in a job condition where a secret cannot, and
the two acts cost different things.

| Variable | Set it to `true` to |
|---|---|
| `IOS_DISTRIBUTION_READY` | Build a signed App Store archive and upload the `.ipa` as the `mabel-app-<shortsha>-ios-ipa` artifact |
| `IOS_TESTFLIGHT_UPLOAD` | Also send that `.ipa` to App Store Connect |

`IOS_DISTRIBUTION_READY` is the switch to leave off until the App ID and the app
record exist, because the signed build registers the identifier and may create a
distribution certificate on the account. `IOS_TESTFLIGHT_UPLOAD` is separate
because an upload consumes a build number and is visible to testers.

The signed build needs no certificate secret and no provisioning profile. Given
the API key, the tauri CLI archives without signing and then exports with
`xcodebuild -allowProvisioningUpdates`, so Xcode fetches or creates the
distribution certificate and the profile itself. `APPLE_TEAM_ID` is passed as
`APPLE_DEVELOPMENT_TEAM`, which is the name cargo-mobile2 reads, and the Xcode
project is regenerated with it set because the simulator project carries no team.

The build number is the workflow run number, appended to the version from
`tauri.conf.json`, because App Store Connect rejects a `CFBundleVersion` it has
already seen.

The upload uses `fastlane pilot`, which the macOS runner image already has, so it
installs nothing. `xcrun altool --upload-app` would be the dependency-free
choice, but Apple deprecated it for `--upload-package`, and that wants the
numeric Apple ID and the ASC public ID of an app record that does not exist yet.
pilot reads both from the API key.

With `IOS_DISTRIBUTION_READY` unset, the iOS job ends after the simulator
artifact, which is what it has always done. All of the distribution steps run
after that artifact is uploaded, so a signing failure cannot cost the build CI
has always produced.

### What `tauri.conf.json` sets

Only `bundle.macOS.hardenedRuntime`, set to `true`. Notarization requires the
hardened runtime, and stating it in the file rather than relying on tauri's
default keeps the requirement visible next to the identifier it applies to.

`signingIdentity` is deliberately absent. Tauri would read a value there on every
build, including builds with no certificate, and fail them; the environment
variable is what carries it, so the unsigned path stays intact.

No entitlements file, for two reasons worth writing down because both look like
they should need one. The app is distributed with a Developer ID and is not
sandboxed, so the `com.apple.security.network.*` entitlements do not apply to it:
those gate the App Sandbox, not the hardened runtime, and the hardened runtime
places no restriction on outbound connections or on the Iroh QUIC listener
accepting inbound ones. And the webview runs JavaScript in a separate
`com.apple.WebKit.WebContent` process that carries Apple's own entitlements, so
the app itself needs no JIT exception.

### What only a real run can show

None of this can be exercised from Linux, and it was not. There is no `codesign`,
no `notarytool` and no Apple account reachable from the machine that wrote it, so
what is checked here is that the workflows and the composite action parse, that
actionlint and yamllint pass, that every shell script in them passes shellcheck
with a stub for `plutil` and `ditto`, and that the variable names
match the tauri 2.11.4 and tauri-bundler 2.9.4 sources that read them. The first
run with secrets present is the first test of:

- whether the `.p12` in `APPLE_CERTIFICATE` imports and its identity matches
  `APPLE_SIGNING_IDENTITY`, which tauri compares by substring and fails on
- whether the API key's role and type let it notarize, which is where an
  Individual key fails
- how long Apple's notary service takes, against the 90 minute job timeout and
  the 45 minute `notarytool --wait` timeout
- whether `stapler validate` passes on the `.app` and the `.dmg`, which is the
  only real proof the notarization happened
- on the CLI side, whether the hand-written keychain import lets `codesign`
  find the identity without prompting, and whether notarytool accepts a zip
  holding one bare executable
- on the iOS side, whether the Admin key can register `dev.reamde.mabel` and
  issue a distribution certificate, and whether `fastlane pilot` accepts the
  generated key file

## Known limits

- Startup blocks until the node has bound both listeners, which is two socket
  binds and a directory read. There is no splash screen: the window appears when
  the node is ready.
- The window is one webview on one URL. Closing it quits the app, which stops the
  node and closes the Iroh endpoint.
- On iOS, syncing through the n0 relays works. Finding peers directly on the same
  network uses multicast, which Apple gates behind the
  `com.apple.developer.networking.multicast` entitlement that a request to Apple
  grants; without it the app still syncs through relays.
- Signing waits on the secrets in [Signing](#signing). Until they are set every
  build is unsigned and is for developers and testers rather than for users.

## Layout

| Path | What is in it |
|---|---|
| `src-tauri/src/node.rs` | starting and stopping the in-process wallet node, and its test |
| `src-tauri/src/app.rs` | the webview glue: data directory, window, shutdown on exit |
| `src-tauri/tauri.conf.json` | product name, bundle identifier, bundle targets, hardened runtime |
| `src-tauri/capabilities/default.json` | the window's own permissions, and nothing for the page |
| `src-tauri/Info.ios.plist` | iOS keys merged into the generated `Info.plist`: local networking and its usage string |
| `src-tauri/icons/` | placeholder icons, a blue ring on a dark square |
| `dist/index.html` | the page shown when the node could not start |
