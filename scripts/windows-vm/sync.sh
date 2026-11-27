#!/usr/bin/env bash
# Copies this checkout's working tree to the Windows VM's clone over SSH, so
# that changes made on the Mac can be built and tested there. Only files
# Git would track are copied: what .gitignore leaves out, such as target/
# and node_modules/, is neither copied nor deleted, and the VM's own .git is
# kept. Files deleted here are deleted there. Files are compared by content,
# and a changed file gets the time of the sync, not the Mac's: an older time
# would let Cargo take it for built already.
#
# Usage: scripts/windows-vm/sync.sh [--dry-run]
#
# PINRAIL_VM_NAME names the VM in UTM (default: Pinrail Windows),
# PINRAIL_VM_USER its account (default: dev) and PINRAIL_VM_HOST its
# address, which is otherwise asked of UTM. The VM needs rsync in Git Bash:
# MSYS2's rsync.exe and the DLLs it needs, in Git's usr\bin.
set -euo pipefail

name="${PINRAIL_VM_NAME:-Pinrail Windows}"
user="${PINRAIL_VM_USER:-dev}"
host="${PINRAIL_VM_HOST:-}"
if [ -z "$host" ]; then
	host="$(/Applications/UTM.app/Contents/MacOS/utmctl ip-address "$name" 2>/dev/null | grep -m1 -E '^[0-9.]+$' || true)"
	[ -n "$host" ] || { echo "UTM knows no address for \"$name\": is it running?" >&2; exit 1; }
fi

root="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
rsync -rl --checksum --delete --itemize-changes "$@" \
	--exclude=/.git \
	--filter=':- .gitignore' \
	"$root/" "$user@$host:/c/dev/pinrail/"
