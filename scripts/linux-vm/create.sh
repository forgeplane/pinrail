#!/usr/bin/env bash
# Creates a Debian 13 virtual machine in UTM on an Apple-silicon Mac, from
# Debian's ARM64 cloud image. At the first boot, cloud-init installs a light
# desktop and the libraries the app needs, then a setup script installs the
# tools that Pinrail's build and tests need and clones the repository.
#
# Usage: scripts/linux-vm/create.sh [options]
#
# The password of the VM's account is read from PINRAIL_VM_PASSWORD, or asked
# for. The VM accepts the first of ~/.ssh/id_ed25519.pub, id_ecdsa.pub and
# id_rsa.pub over SSH, or the public key file PINRAIL_VM_SSH_KEY names.
set -euo pipefail

name="Pinrail Linux"
user="dev"
memory=6144
cpus=4
disk=64

release="trixie"
image="debian-13-generic-arm64.tar.xz"
images_url="https://cloud.debian.org/images/cloud/$release/latest"

usage() {
	cat <<EOF
Usage: $0 [options]

Options:
  --name NAME      The VM's name in UTM (default: $name)
  --user NAME      The Linux account's name (default: $user)
  --memory MIB     Memory in MiB (default: $memory)
  --cpus N         CPU cores (default: $cpus)
  --disk GIB       Disk size in GiB, allocated as the VM uses it (default: $disk)
EOF
}

while [ $# -gt 0 ]; do
	case "$1" in
	--name) name="$2"; shift 2 ;;
	--user) user="$2"; shift 2 ;;
	--memory) memory="$2"; shift 2 ;;
	--cpus) cpus="$2"; shift 2 ;;
	--disk) disk="$2"; shift 2 ;;
	-h | --help) usage; exit 0 ;;
	*) echo "Unknown argument: $1" >&2; usage >&2; exit 2 ;;
	esac
done

[ -d /Applications/UTM.app ] || { echo "UTM is not installed in /Applications." >&2; exit 1; }

here="$(cd "$(dirname "$0")" && pwd)"
cache="$HOME/Library/Caches/pinrail-linux-vm"
mkdir -p "$cache"

if osascript -e "tell application \"UTM\" to exists virtual machine named \"$name\"" | grep -q true; then
	echo "UTM already has a VM named \"$name\". Delete it or pass --name." >&2
	exit 1
fi
# UTM is restarted to add the disks, which would stop running VMs.
if osascript -e 'tell application "UTM" to get status of every virtual machine' | grep -qvE '^(stopped(, stopped)*)?$'; then
	echo "Stop the VMs that are running in UTM first." >&2
	exit 1
fi

free_gib=$(df -g "$HOME" | awk 'NR == 2 { print $4 }')
if [ "$free_gib" -lt 40 ]; then
	echo "Warning: only ${free_gib}G is free. Debian and a Pinrail build need about 40G." >&2
fi

password="${PINRAIL_VM_PASSWORD:-}"
if [ -z "$password" ]; then
	read -rsp "Password for the VM's account \"$user\": " password
	echo
	read -rsp "Repeat the password: " again
	echo
	[ "$password" = "$again" ] || { echo "The passwords differ." >&2; exit 1; }
	[ -n "$password" ] || { echo "The password is empty." >&2; exit 1; }
fi

ssh_key="${PINRAIL_VM_SSH_KEY:-}"
if [ -z "$ssh_key" ]; then
	for candidate in "$HOME"/.ssh/id_ed25519.pub "$HOME"/.ssh/id_ecdsa.pub "$HOME"/.ssh/id_rsa.pub; do
		if [ -f "$candidate" ]; then ssh_key="$candidate"; break; fi
	done
fi
[ -n "$ssh_key" ] || echo "Warning: no SSH public key found in ~/.ssh, so the VM accepts no SSH sign-in." >&2

# Debian's cloud image, checked against the published SHA-512 sums. A newer
# image replaces the cached one.
sums="$(curl -fsSL "$images_url/SHA512SUMS")"
expected="$(echo "$sums" | awk -v f="$image" '$2 == f { print $1 }')"
[ -n "$expected" ] || { echo "$image is not in $images_url/SHA512SUMS." >&2; exit 1; }
if [ ! -f "$cache/$image" ] || [ "$(shasum -a 512 "$cache/$image" | cut -d' ' -f1)" != "$expected" ]; then
	echo "Downloading $image"
	curl -fL --progress-bar -o "$cache/$image.part" "$images_url/$image"
	[ "$(shasum -a 512 "$cache/$image.part" | cut -d' ' -f1)" = "$expected" ] ||
		{ echo "The downloaded image does not match its SHA-512 sum." >&2; exit 1; }
	mv "$cache/$image.part" "$cache/$image"
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# The seed disc that cloud-init reads at the first boot. The placeholders are
# replaced with values quoted for YAML's single-quoted strings.
seed="$work/seed"
mkdir -p "$seed"
ssh_keys=""
if [ -n "$ssh_key" ]; then
	ssh_keys="    ssh_authorized_keys:
      - '$(sed "s/'/''/g" "$ssh_key")'"
fi
VM_USER="$user" VM_PASSWORD="$password" VM_SSH_KEYS="$ssh_keys" \
	VM_SETUP="$(base64 <"$here/pinrail-vm-setup.sh" | tr -d '\n')" perl -pe '
	sub quote { my $s = shift; $s =~ s/\x27/\x27\x27/g; $s }
	s/__USER__/$ENV{VM_USER}/g; s/__PASSWORD__/quote($ENV{VM_PASSWORD})/ge;
	s/__SSH_KEYS__/$ENV{VM_SSH_KEYS}/g; s/__SETUP_B64__/$ENV{VM_SETUP}/g;
' "$here/user-data" >"$seed/user-data"
printf 'instance-id: %s\nlocal-hostname: pinrail-linux\n' "$(uuidgen)" >"$seed/meta-data"
hdiutil makehybrid -quiet -iso -joliet -default-volume-name cidata -o "$work/seed.iso" "$seed"

echo "Creating the VM \"$name\" in UTM"
osascript - "$name" "$memory" "$cpus" >/dev/null <<'APPLESCRIPT'
on run argv
	set {vmName, vmMemory, vmCpus} to argv
	tell application "UTM"
		make new virtual machine with properties {backend:qemu, configuration:{name:vmName, architecture:"aarch64", memory:(vmMemory as integer), cpu cores:(vmCpus as integer), hypervisor:true, uefi:true, drives:{}, network interfaces:{{hardware:"virtio-net-pci", mode:shared}}, displays:{{hardware:"virtio-ramfb", dynamic resolution:true}}}}
	end tell
end run
APPLESCRIPT

bundle="$HOME/Library/Containers/com.utmapp.UTM/Data/Documents/$name.utm"
[ -f "$bundle/config.plist" ] || { echo "UTM did not create $bundle." >&2; exit 1; }

# UTM's scripting cannot import a raw disk or put a disc in a CD drive, so
# both are added to the VM's configuration while UTM is closed. The disk is a
# sparse file: it takes space only as the VM writes to it.
# UTM 5 can refuse to quit when asked, reporting "User cancelled", so it
# is stopped with a signal then. No VM is running, as checked above.
osascript -e 'tell application "UTM" to quit' 2>/dev/null || pkill -x UTM
while pgrep -xq UTM; do sleep 1; done
echo "Unpacking the disk"
tar -xJf "$cache/$image" -C "$bundle/Data" disk.raw
mv "$bundle/Data/disk.raw" "$bundle/Data/disk.img"
truncate -s "${disk}G" "$bundle/Data/disk.img"
cp "$work/seed.iso" "$bundle/Data/seed.iso"
config="$bundle/config.plist"
# The seed disc is a read-only VirtIO disk: Linux detects a USB CD too late
# in the boot, after cloud-init has looked for its configuration.
plutil -insert Drive.0 -json "{\"Identifier\":\"$(uuidgen)\",\"ImageName\":\"seed.iso\",\"ImageType\":\"Disk\",\"Interface\":\"VirtIO\",\"InterfaceVersion\":1,\"ReadOnly\":true}" "$config"
plutil -insert Drive.0 -json "{\"Identifier\":\"$(uuidgen)\",\"ImageName\":\"disk.img\",\"ImageType\":\"Disk\",\"Interface\":\"VirtIO\",\"InterfaceVersion\":1,\"ReadOnly\":false}" "$config"
open -ga UTM
sleep 5

echo "Starting the VM"
osascript - "$name" <<'APPLESCRIPT'
on run argv
	tell application "UTM"
		-- start waits for the VM, which can take longer than an Apple event.
		with timeout of 120 seconds
			try
				start (virtual machine named (item 1 of argv))
			end try
		end timeout
	end tell
end run
APPLESCRIPT

cat <<EOF

The VM is setting itself up, which takes about 30 minutes. cloud-init
installs the desktop and the libraries, then the setup script installs Rust,
mise and the project's tools, clones Pinrail into ~/pinrail and runs
"mise run setup". The desktop appears part way through.

To follow the setup from the Mac:

  ssh $user@\$(/Applications/UTM.app/Contents/MacOS/utmctl ip-address "$name" | grep -m1 -E '^[0-9.]+$') tail -f pinrail-vm-setup.log

The setup has finished when ~/.pinrail-vm-ready exists in the VM. Once it
has, shut the VM down and delete its CD drive in UTM's settings for the VM.
EOF
