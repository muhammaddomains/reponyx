#!/usr/bin/env bash
# check-no-private-key.sh — proves the GitHub App private key is
# absent from the desktop app.
#
# Fails (exit 1) when:
#   1. any private-key marker appears in the desktop sources, or
#   2. the desktop crate depends on the Auth Broker crate
#      (the only component allowed to hold the key), or
#   3. a private-key marker appears in a built desktop binary, or
#   4. a committed key file (.pem/.key) exists under apps/desktop.
#
# The binary check runs only when a built desktop binary exists.
# On machines where the Tauri build does not work (this Linux VM
# lacks the WebViewGTK system libs), the binary part is SKIPPED
# with an explicit note — the sources + dependency checks still
# run and still fail the build if they find anything. Run the full
# script on a machine where `cargo tauri build` works before
# release (Phase 10).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fail=0

MARKERS=(
  "BEGIN RSA PRIVATE KEY"
  "BEGIN PRIVATE KEY"
  "BEGIN EC PRIVATE KEY"
  "BEGIN OPENSSH PRIVATE KEY"
)

echo "== desktop sources: private-key markers =="
for marker in "${MARKERS[@]}"; do
  if grep -rn -F --include='*.rs' --include='*.ts' --include='*.tsx' \
      --include='*.json' --include='*.toml' --include='*.md' \
      "$marker" "$REPO_ROOT/apps/desktop" 2>/dev/null; then
    echo "FAIL: private-key marker '$marker' found in desktop sources" >&2
    fail=1
  fi
done

echo "== desktop sources: broker key references =="
# The desktop must never read the broker's private key, in any form.
for needle in "REPONYX_BROKER_PRIVATE_KEY" "private_key_pem" "from_rsa_pem"; do
  if grep -rn -F --include='*.rs' --include='*.ts' --include='*.tsx' \
      "$needle" "$REPO_ROOT/apps/desktop" 2>/dev/null; then
    echo "FAIL: desktop source references broker key material ('$needle')" >&2
    fail=1
  fi
done

echo "== desktop crate: must not depend on the Auth Broker =="
if grep -q "reponyx-auth-broker" "$REPO_ROOT/apps/desktop/src-tauri/Cargo.toml"; then
  echo "FAIL: desktop Cargo.toml mentions reponyx-auth-broker" >&2
  fail=1
fi
if (cd "$REPO_ROOT" && cargo tree -p reponyx-desktop --edges normal 2>/dev/null) \
    | grep -q "reponyx-auth-broker"; then
  echo "FAIL: reponyx-auth-broker appears in the desktop dependency tree" >&2
  fail=1
fi

echo "== committed key files under apps/desktop =="
if git -C "$REPO_ROOT" ls-files apps/desktop | grep -iE '\.(pem|key)$'; then
  echo "FAIL: committed key file under apps/desktop" >&2
  fail=1
fi

echo "== desktop binary: private-key markers =="
binary_found=0
for candidate in \
    "$REPO_ROOT/apps/desktop/src-tauri/target/release/reponyx-desktop" \
    "$REPO_ROOT/target/release/reponyx-desktop"; do
  if [ -f "$candidate" ]; then
    binary_found=1
    echo "scanning $candidate"
    for marker in "${MARKERS[@]}"; do
      if grep -a -F -q "$marker" "$candidate"; then
        echo "FAIL: private-key marker '$marker' found in $candidate" >&2
        fail=1
      fi
    done
  fi
done
if [ "$binary_found" -eq 0 ]; then
  echo "SKIP: no built desktop binary found (Tauri build needs WebViewGTK libs;"
  echo "      run this script on a machine where 'cargo tauri build' works for the full check)."
fi

if [ "$fail" -ne 0 ]; then
  echo "check-no-private-key: FAILED" >&2
  exit 1
fi
echo "check-no-private-key: PASSED"
