#!/usr/bin/env bash
# Creates a Windows 11 on ARM virtual machine in UTM on an Apple-silicon Mac.
# Windows installs without prompts, then a setup script in the VM installs
# the tools that Pinrail's build and tests need and clones the repository.
#
# Usage: scripts/windows-vm/create.sh [options] <windows-11-arm64.iso>
#
# Download the ISO from https://www.microsoft.com/software-download/windows11arm64
# first. The password of the VM's account is read from PINRAIL_VM_PASSWORD,
# or asked for.
set -euo pipefail

name="Pinrail Windows"
user="dev"
memory=8192
cpus=4
disk=128

usage() {
	cat <<EOF
Usage: $0 [options] <windows-11-arm64.iso>

Options:
  --name NAME      The VM's name in UTM (default: $name)
  --user NAME      The Windows account's name (default: $user)
  --memory MIB     Memory in MiB (default: $memory)
  --cpus N         CPU cores (default: $cpus)
  --disk GIB       Disk size in GiB, allocated as Windows uses it (default: $disk)
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
	-*) echo "Unknown option: $1" >&2; usage >&2; exit 2 ;;
	*) break ;;
	esac
done

if [ $# -ne 1 ]; then
	usage >&2
	exit 2
fi
iso="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
[ -f "$iso" ] || { echo "No such file: $1" >&2; exit 1; }
[ -d /Applications/UTM.app ] || { echo "UTM is not installed in /Applications." >&2; exit 1; }

here="$(cd "$(dirname "$0")" && pwd)"
cache="$HOME/Library/Caches/pinrail-windows-vm"
mkdir -p "$cache"

if osascript -e "tell application \"UTM\" to exists virtual machine named \"$name\"" | grep -q true; then
	echo "UTM already has a VM named \"$name\". Delete it or pass --name." >&2
	exit 1
fi
# UTM is restarted to add the install discs, which would stop running VMs.
if osascript -e 'tell application "UTM" to get status of every virtual machine' | grep -qvE '^(stopped(, stopped)*)?$'; then
	echo "Stop the VMs that are running in UTM first." >&2
	exit 1
fi

free_gib=$(df -g "$HOME" | awk 'NR == 2 { print $4 }')
if [ "$free_gib" -lt 80 ]; then
	echo "Warning: only ${free_gib}G is free. Windows and a Pinrail build need about 80G." >&2
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

# UTM's guest tools: the network, display and clipboard drivers.
tools_iso="$cache/utm-guest-tools.iso"
if [ ! -f "$tools_iso" ]; then
	echo "Downloading the UTM guest tools"
	curl -fL --progress-bar -o "$tools_iso.part" https://getutm.app/downloads/utm-guest-tools-latest.iso
	mv "$tools_iso.part" "$tools_iso"
fi

work="$(mktemp -d)"
mount="$work/tools"
cleanup() {
	hdiutil detach -quiet "$mount" 2>/dev/null || true
	rm -rf "$work"
}
trap cleanup EXIT

stage="$work/disc"
mkdir -p "$stage" "$mount"
hdiutil attach -quiet -nobrowse -readonly -mountpoint "$mount" "$tools_iso"
cp "$mount"/utm-guest-tools-*.exe "$stage/"
hdiutil detach -quiet "$mount"

# The answer file, with the account's name and password escaped for XML.
VM_USER="$user" VM_PASSWORD="$password" perl -pe '
	sub esc { my $s = shift; $s =~ s/&/&amp;/g; $s =~ s/</&lt;/g; $s =~ s/>/&gt;/g; $s =~ s/"/&quot;/g; $s =~ s/\x27/&apos;/g; $s }
	s/__USER__/esc($ENV{VM_USER})/ge; s/__PASSWORD__/esc($ENV{VM_PASSWORD})/ge;
' "$here/autounattend.xml" >"$stage/autounattend.xml"
cp "$here/pinrail-vm-setup.ps1" "$stage/"

# The public key that may sign in to the VM over SSH.
ssh_key="${PINRAIL_VM_SSH_KEY:-}"
if [ -z "$ssh_key" ]; then
	for candidate in "$HOME"/.ssh/id_ed25519.pub "$HOME"/.ssh/id_ecdsa.pub "$HOME"/.ssh/id_rsa.pub; do
		if [ -f "$candidate" ]; then ssh_key="$candidate"; break; fi
	done
fi
if [ -n "$ssh_key" ]; then
	cp "$ssh_key" "$stage/authorized_keys"
else
	echo "Warning: no SSH public key found in ~/.ssh, so the VM will have no SSH server." >&2
	echo "Set PINRAIL_VM_SSH_KEY to a public key file to choose one." >&2
fi

setup_iso="$work/setup.iso"
hdiutil makehybrid -quiet -iso -joliet -default-volume-name PINRAIL_VM -o "$setup_iso" "$stage"

echo "Creating the VM \"$name\" in UTM"
osascript - "$name" "$memory" "$cpus" "$((disk * 1024))" >/dev/null <<'APPLESCRIPT'
on run argv
	set {vmName, vmMemory, vmCpus, diskMib} to argv
	tell application "UTM"
		make new virtual machine with properties {backend:qemu, configuration:{name:vmName, architecture:"aarch64", memory:(vmMemory as integer), cpu cores:(vmCpus as integer), hypervisor:true, uefi:true, drives:{{interface:NVMe, guest size:(diskMib as integer)}}, network interfaces:{{hardware:"virtio-net-pci", mode:shared}}, displays:{{hardware:"virtio-ramfb", dynamic resolution:true}}}}
	end tell
end run
APPLESCRIPT

bundle="$HOME/Library/Containers/com.utmapp.UTM/Data/Documents/$name.utm"
[ -f "$bundle/config.plist" ] || { echo "UTM did not create $bundle." >&2; exit 1; }

# UTM's scripting cannot put a disc in a CD drive, so the discs are added to
# the VM's configuration while UTM is closed. The Windows ISO is an APFS
# clone, which takes no extra space.
osascript -e 'tell application "UTM" to quit'
while pgrep -xq UTM; do sleep 1; done
cp -c "$iso" "$bundle/Data/windows.iso" 2>/dev/null || cp "$iso" "$bundle/Data/windows.iso"
cp "$setup_iso" "$bundle/Data/setup.iso"
config="$bundle/config.plist"
# The discs come after the VM's disk in the boot order. While the disk is
# empty, the VM boots the Windows disc. After Setup's first restart, it boots
# the disk, so the disc's key prompt does not start Setup over.
for disc in windows setup; do
	plutil -insert Drive -json "{\"Identifier\":\"$(uuidgen)\",\"ImageName\":\"$disc.iso\",\"ImageType\":\"CD\",\"Interface\":\"USB\",\"InterfaceVersion\":1,\"ReadOnly\":true}" -append "$config"
done
# The TPM and a clock in local time, as UTM's Windows template sets them.
plutil -replace QEMU.TPMDevice -bool true "$config"
plutil -replace QEMU.RTCLocalTime -bool true "$config"
open -ga UTM
sleep 5

echo "Starting the VM"
osascript - "$name" <<'APPLESCRIPT'
on run argv
	tell application "UTM"
		set vm to virtual machine named (item 1 of argv)
		-- start waits for the VM, which can take longer than an Apple event.
		with timeout of 120 seconds
			try
				start vm
			end try
		end timeout
	end tell
end run
APPLESCRIPT

# The Windows disc waits a few seconds for a key press before it boots, and
# the firmware can take a minute to load it, so a key is pressed every second
# until Setup starts writing to the disk. Later presses would reach Setup's
# Cancel button.
disk_image="$(find "$bundle/Data" -name '*.qcow2' | head -1)"
for _ in $(seq 1 120); do
	[ "$(du -k "$disk_image" | cut -f1)" -gt 4096 ] && break
	osascript -e "tell application \"UTM\" to input keystroke (virtual machine named \"$name\") text \" \"" 2>/dev/null || true
	sleep 1
done

cat <<EOF

The VM is installing Windows. Expect it to take about an hour in total:

1. Windows installs and restarts a few times, which takes about 20 minutes.
2. Windows signs in as "$user" and opens the "Pinrail VM setup" window. It
   installs the guest tools, Git, the Visual Studio build tools, Rust, mise
   and the project's tools, clones Pinrail into C:\\dev\\pinrail and runs
   "mise run setup".

The setup log is in C:\\ProgramData\\PinrailVM\\setup.log in the VM. When the
window says the VM is ready, shut the VM down and delete its two CD drives in
UTM's settings for the VM. The Windows ISO in the VM is a clone of
$iso,
so it takes space of its own only once that file is deleted. Then clone the
VM in UTM to keep a clean copy.

To sign in over SSH once the setup has finished:

  ssh $user@\$(/Applications/UTM.app/Contents/MacOS/utmctl ip-address "$name" | grep -m1 -E '^[0-9.]+$')
EOF
