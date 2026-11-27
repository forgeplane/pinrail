#!/usr/bin/env bash
# Installs the tools that Pinrail's build and tests need for the current
# account, clones Pinrail into ~/pinrail and runs its setup. cloud-init runs
# it once at the first boot, with its output in ~/pinrail-vm-setup.log. Run
# `pinrail-vm-setup` again to retry after a failure: each step is skipped when
# its result is already there.
set -euxo pipefail

# shellcheck disable=SC2016 # written to ~/.bashrc unexpanded
tools_path='$HOME/.local/share/mise/shims:$HOME/.local/bin:$HOME/.cargo/bin'
export PATH="$HOME/.local/share/mise/shims:$HOME/.local/bin:$HOME/.cargo/bin:$PATH"

# Commands run over SSH read only ~/.bashrc, and Debian's ~/.bashrc stops
# early for them, so the tools are added to PATH at its top.
if ! grep -q 'pinrail-vm' ~/.bashrc; then
	{
		echo '# pinrail-vm: the build tools, for interactive shells and SSH commands'
		echo "export PATH=\"$tools_path:\$PATH\""
		cat ~/.bashrc
	} >~/.bashrc.new
	mv ~/.bashrc.new ~/.bashrc
fi

if ! command -v rustup >/dev/null; then
	curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs |
		sh -s -- -y --profile minimal --default-toolchain none --no-modify-path
fi
if ! command -v mise >/dev/null; then
	curl -fsSL https://mise.run | sh
fi

if [ ! -d ~/pinrail/.git ]; then
	git clone https://github.com/forgeplane/pinrail ~/pinrail
fi
cd ~/pinrail
mise trust
mise install
# the Rust version that rust-toolchain.toml pins
rustup toolchain install
mise run setup
# WebKit for the end-to-end tests, and the system libraries both browsers
# need, as CI installs them on Linux
(cd e2e && npx playwright install webkit && npx playwright install-deps chromium webkit)

touch ~/.pinrail-vm-ready
echo 'The VM is ready.'
