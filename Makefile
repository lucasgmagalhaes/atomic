# Nimble - common developer commands.
#
# Windows note: `quickjs-sys`'s C build needs a Developer Command Prompt
# environment (cc-rs's own MSVC autodetection doesn't find the toolchain
# on this machine's split VS/SDK-drive layout - see CLAUDE.md). Every
# cargo invocation below runs through scripts/with-vcvars.bat, which
# sources vcvars64.bat first, when running on Windows. Set NIMBLE_VCVARS
# in your environment if your vcvars64.bat lives somewhere else (that
# script's own default matches this dev machine). On Linux/macOS this is
# skipped entirely - plain `cargo` needs no such setup there.

ifeq ($(OS),Windows_NT)
  CARGO := cmd /c scripts\with-vcvars.bat
else
  CARGO := cargo
endif

.PHONY: help build build-release test test-verbose check run run-release \
        fmt fmt-check lint clean update doc graphify auto-loop auto-stop

help:
	@echo "Nimble - make targets:"
	@echo "  build          cargo build --workspace (debug)"
	@echo "  build-release  cargo build --workspace --release (prod)"
	@echo "  run            run the shell app (debug)"
	@echo "  run-release    run the shell app (release)"
	@echo "  test           cargo test --workspace"
	@echo "  test-verbose   cargo test --workspace -- --nocapture"
	@echo "  check          cargo check --workspace (fast type-check, no codegen)"
	@echo "  fmt            cargo fmt --all"
	@echo "  fmt-check      cargo fmt --all -- --check (CI-style, no writes)"
	@echo "  lint           cargo clippy --workspace --all-targets"
	@echo "  update         cargo update (bump dependency lockfile)"
	@echo "  doc            cargo doc --workspace --no-deps --open"
	@echo "  graphify       refresh the local graphify knowledge graph"
	@echo "  clean          cargo clean"
	@echo "  auto-loop      run the autonomous coding loop in the foreground (see .opencode/)"
	@echo "  auto-stop      signal the autonomous loop to stop after its current step"

build:
	$(CARGO) build --workspace

build-release:
	$(CARGO) build --workspace --release

run:
	$(CARGO) run -p shell

run-release:
	$(CARGO) run -p shell --release

test:
	$(CARGO) test --workspace

test-verbose:
	$(CARGO) test --workspace -- --nocapture

check:
	$(CARGO) check --workspace

fmt:
	$(CARGO) fmt --all

fmt-check:
	$(CARGO) fmt --all -- --check

lint:
	$(CARGO) clippy --workspace --all-targets

update:
	$(CARGO) update

doc:
	$(CARGO) doc --workspace --no-deps --open

graphify:
	graphify update .

clean:
	$(CARGO) clean

auto-loop:
	@if exist .opencode\STOP del .opencode\STOP
	powershell -NoProfile -ExecutionPolicy Bypass -File .opencode/autonomous-loop.ps1

auto-stop:
	@echo stop > .opencode/STOP
	@echo "STOP flag set - loop will exit after its current step"
