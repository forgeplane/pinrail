#!/usr/bin/env bash
# Works on the Windows VM's desktop from the Mac: what SSH starts runs in a
# session of its own, with no desktop, so a window it opens is never seen.
# A scheduled task that runs only while the user is logged on runs in their
# desktop session instead, as a program they started would.
#
# Usage:
#   scripts/windows-vm/desktop.sh run <program> [arguments]
#       starts the program on the desktop and returns at once
#   scripts/windows-vm/desktop.sh shot [file.png]
#       saves a screenshot of the desktop (default: vm-desktop.png)
#
# The VM's account must be logged on at its console. PINRAIL_VM_NAME,
# PINRAIL_VM_USER and PINRAIL_VM_HOST are as for sync.sh.
set -euo pipefail

name="${PINRAIL_VM_NAME:-Pinrail Windows}"
user="${PINRAIL_VM_USER:-dev}"
host="${PINRAIL_VM_HOST:-}"
if [ -z "$host" ]; then
	host="$(/Applications/UTM.app/Contents/MacOS/utmctl ip-address "$name" 2>/dev/null | grep -m1 -E '^[0-9.]+$' || true)"
	[ -n "$host" ] || { echo "UTM knows no address for \"$name\": is it running?" >&2; exit 1; }
fi
vm() { ssh -o BatchMode=yes "$user@$host" "$@" < /dev/null; }

# Runs a PowerShell script, given on standard input, on the desktop, and
# waits for it to finish when $1 is "wait".
on_desktop() {
	local wait="$1" script
	script="$(cat)"
	printf '%s\n' "$script" | ssh -o BatchMode=yes "$user@$host" 'cat > "$USERPROFILE/pinrail-desktop.ps1"'
	vm 'rm -f "$USERPROFILE/pinrail-desktop.done"
schtasks //Create //TN PinrailDesktop //SC ONCE //ST 00:00 //IT //F //TR "powershell -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File \"%USERPROFILE%\pinrail-desktop.ps1\"" > /dev/null 2>&1 &&
schtasks //Run //TN PinrailDesktop > /dev/null'
	if [ "$wait" = wait ]; then
		for _ in $(seq 60); do
			vm 'test -f "$USERPROFILE/pinrail-desktop.done"' && return 0
			sleep 1
		done
		echo "the desktop did not answer: is $user logged on at the VM's console?" >&2
		return 1
	fi
}

# One argument in double quotes, as a Windows program reads it back
# (CommandLineToArgvW): a quote inside is \", and the backslashes before
# a quote, or before the closing one, are doubled.
windows_quoted() {
	local arg="$1" out="" slashes="" c i
	for ((i = 0; i < ${#arg}; i++)); do
		c="${arg:i:1}"
		case "$c" in
			\\) slashes+="\\" ;;
			\") out+="$slashes$slashes\\\""; slashes="" ;;
			*) out+="$slashes$c"; slashes="" ;;
		esac
	done
	printf '"%s"' "$out$slashes$slashes"
}

case "${1:-}" in
	run)
		shift
		[ $# -gt 0 ] || { echo "usage: $0 run <program> [arguments]" >&2; exit 2; }
		# Start-Process joins its arguments with spaces and quotes none,
		# so one with a space or a quote goes in double quotes, inside
		# PowerShell's single ones; the rest as they are, as a program
		# such as cmd takes no switch in quotes
		args=""
		shift_program="$1"
		shift
		for arg in "$@"; do
			case "$arg" in
				"" | *[[:space:]]* | *\"*) arg="$(windows_quoted "$arg")" ;;
			esac
			args+="'${arg//\'/\'\'}',"
		done
		on_desktop nowait <<EOF
Start-Process -FilePath '${shift_program//\'/\'\'}' $([ -n "$args" ] && echo "-ArgumentList ${args%,}")
EOF
		;;
	shot)
		out="${2:-vm-desktop.png}"
		on_desktop wait <<'EOF'
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type -Namespace Pinrail -Name Dpi -MemberDefinition '[DllImport("user32.dll")] public static extern bool SetProcessDPIAware();'
[Pinrail.Dpi]::SetProcessDPIAware() | Out-Null
$bounds = [System.Windows.Forms.SystemInformation]::VirtualScreen
$image = New-Object System.Drawing.Bitmap $bounds.Width, $bounds.Height
$graphics = [System.Drawing.Graphics]::FromImage($image)
$graphics.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
$image.Save("$env:USERPROFILE\pinrail-desktop.png", [System.Drawing.Imaging.ImageFormat]::Png)
New-Item -ItemType File -Force "$env:USERPROFILE\pinrail-desktop.done" | Out-Null
EOF
		scp -q -o BatchMode=yes "$user@$host:pinrail-desktop.png" "$out"
		echo "$out"
		;;
	*)
		echo "usage: $0 run <program> [arguments] | shot [file.png]" >&2
		exit 2
		;;
esac
