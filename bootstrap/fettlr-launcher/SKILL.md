---
name: fettlr-launcher
description: Install and launch Fettlr, the manager for Claude Code's skills, plugins, marketplaces, and MCP servers.
---

# Fettlr launcher

Get [Fettlr](https://github.com/thomasguillot/fettlr) running on this Mac: launch it if it is
already installed, otherwise download the latest release, verify it, install it, and launch it.
Fettlr is not signed with an Apple Developer ID, so this skill is explicit about that and verifies
the download against Fettlr's own release key instead.

Follow the steps below in order. If any step fails, stop, tell the user plainly what went wrong,
and point them to the manual download at https://github.com/thomasguillot/fettlr/releases. Do not
retry automatically and do not improvise a workaround.

Needs `curl`, `tar`, `jq`, and `minisign` on the `PATH`. `curl` and `tar` ship with macOS; `jq` and
`minisign` usually don't. If either is missing, install it (`brew install jq`, `brew install
minisign`) before continuing, or ask the user.

## 1. Check for an existing install

```
test -d /Applications/Fettlr.app
```

If that succeeds, Fettlr is already installed. Launch it and confirm it started, the same way as
step 3:

```
open -a Fettlr
pgrep -x fettlr-app
```

If a process ID comes back, stop here, done. If not, say so plainly and suggest opening it from the
Applications folder or Spotlight instead of retrying blindly.

## 2. Tell the user, then download, verify, and install

Before running anything, tell the user plainly: Fettlr is not signed with an Apple Developer ID.
Instead, the download is verified against Fettlr's own release key (a minisign signature) before it
is installed, and the install is refused if verification fails.

Then run the whole script below as a single command. It must be one Bash call: shell variables do
not survive between separate calls. It checks the architecture (Fettlr currently ships Apple
silicon builds only), reads the latest release's `latest.json`, downloads the app archive, verifies
its signature against the hard-coded public key, and only then installs it. Any failure stops the
script and cleans up after itself.

```bash
set -euo pipefail
TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

ARCH=$(uname -m)
if [ "$ARCH" != "arm64" ]; then
  echo "Fettlr currently ships Apple silicon builds only; this Mac is $ARCH." >&2
  exit 1
fi

PUBKEY="RWTpUXRNkl/BCe9qPYC5NASYX0ovODw/yawqbD7DppfUvDmzhwdqwxhN"

curl -fsSL -o "$TMP_DIR/latest.json" https://github.com/thomasguillot/fettlr/releases/latest/download/latest.json
APP_URL=$(jq -r '.platforms["darwin-aarch64"].url // empty' "$TMP_DIR/latest.json")
SIG=$(jq -r '.platforms["darwin-aarch64"].signature // empty' "$TMP_DIR/latest.json")
if [ -z "$APP_URL" ] || [ -z "$SIG" ]; then
  echo "No Apple silicon build is listed in the latest release." >&2
  exit 1
fi

curl -fsSL -o "$TMP_DIR/Fettlr.app.tar.gz" "$APP_URL"
printf %s "$SIG" | base64 -d > "$TMP_DIR/app.minisig"

if ! minisign -Vm "$TMP_DIR/Fettlr.app.tar.gz" -x "$TMP_DIR/app.minisig" -P "$PUBKEY"; then
  echo "Signature verification FAILED. Nothing was installed." >&2
  exit 1
fi

mkdir "$TMP_DIR/extract"
tar -xzf "$TMP_DIR/Fettlr.app.tar.gz" -C "$TMP_DIR/extract"

rm -rf /Applications/Fettlr.app
ditto "$TMP_DIR/extract/Fettlr.app" /Applications/Fettlr.app
echo "Installed Fettlr."
```

If the script fails for any reason, report which step failed, and in particular a verification
failure plainly: the download did not match Fettlr's release key, so nothing was installed. Then
point the user to the manual download and stop. If the script failed during the final copy, remove
any partial bundle with `rm -rf /Applications/Fettlr.app`, otherwise step 1's `test -d` would treat
the broken bundle as a valid install on every future run. Only continue if the script printed
"Installed Fettlr."

## 3. Launch and confirm

```
open -a Fettlr
```

Give it a moment to start, then confirm it actually launched:

```
pgrep -x fettlr-app
```

If a process ID comes back, Fettlr is running. If not, say so plainly and suggest opening it from
the Applications folder or Spotlight instead of retrying blindly.

## Not this skill's job

Fettlr updates itself once installed. This skill only gets it installed and open; it never checks
for or applies updates to an existing install.
