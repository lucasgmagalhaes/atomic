# Atomic - common developer commands.
#
# Windows note: `quickjs-sys`'s C build needs a Developer Command Prompt
# environment (cc-rs's own MSVC autodetection doesn't find the toolchain
# on this machine's split VS/SDK-drive layout - see CLAUDE.md). Every
# cargo invocation below runs through scripts/with-vcvars.bat, which
# sources vcvars64.bat first, when running on Windows. Set ATOMIC_VCVARS
# in your environment if your vcvars64.bat lives somewhere else (that
# script's own default matches this dev machine). On Linux/macOS this is
# skipped entirely - plain `cargo` needs no such setup there.

ifeq ($(OS),Windows_NT)
  CARGO := cmd /c scripts\with-vcvars.bat
else
  CARGO := cargo
endif

# Keep Cargo's default incremental artifacts for the edit/build loop. When
# developers have sccache installed, use it transparently as an additional
# compiler cache (especially useful after switching branches or cleaning).
# The check is deliberately optional: a fresh clone works with just Rust.
SCCACHE_PATH := $(shell command -v sccache 2>/dev/null)
ifneq ($(strip $(SCCACHE_PATH)),)
  RUSTC_WRAPPER ?= $(SCCACHE_PATH)
  export RUSTC_WRAPPER
  SCCACHE_STATUS := enabled ($(SCCACHE_PATH))
else
  SCCACHE_STATUS := not installed (Cargo incremental cache remains enabled)
endif

# Prefer nextest when available: unlike `cargo test`, it schedules individual
# tests across binaries. Keep a Cargo fallback so a fresh checkout remains
# usable before the optional developer tool is installed.
NEXTEST_AVAILABLE := $(shell $(CARGO) nextest --version >/dev/null 2>&1 && echo yes)
ifeq ($(NEXTEST_AVAILABLE),yes)
  TEST := $(CARGO) nextest run
  TEST_RUNNER := cargo-nextest
  PROFILE_TEST := $(TEST) -p profile
else
  TEST := $(CARGO) test
  TEST_RUNNER := cargo test
  PROFILE_TEST := $(CARGO) test -p profile -- --test-threads=1
endif

.PHONY: help build build-release test test-verbose unit runtime integration workspace check run run-release \
        fmt fmt-check lint cache-info cache-stats clean update doc graphify auto-loop auto-stop

help:
	@echo "Atomic - make targets:"
	@echo "  build          cargo build --workspace (debug)"
	@echo "  build-release  cargo build --workspace --release (prod)"
	@echo "  run            run the atomic app (debug)"
	@echo "  run-release    run the atomic app (release)"
	@echo "  test           cargo test --workspace"
	@echo "  test-verbose   cargo test --workspace -- --nocapture"
	@echo "  unit           fast deterministic tests (uses $(TEST_RUNNER) when installed)"
	@echo "  runtime        complete js-runtime suite; skips the Xvfb-unstable clipboard round-trip"
	@echo "  integration    OS, network, GPU and profile-worker tests (uses $(TEST_RUNNER) when installed)"
	@echo "  workspace      complete test gate (same coverage as test)"
	@echo "  check          cargo check --workspace (fast type-check, no codegen)"
	@echo "  fmt            cargo fmt --all"
	@echo "  fmt-check      cargo fmt --all -- --check (CI-style, no writes)"
	@echo "  lint           cargo clippy --workspace --all-targets"
	@echo "  cache-info     show local test-runner and compiler-cache status"
	@echo "  cache-stats    show sccache statistics when installed"
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
	$(CARGO) run -p atomic

run-release:
	$(CARGO) run -p atomic --release

test:
	$(TEST) --workspace

test-verbose:
	$(CARGO) test --workspace -- --nocapture

# Fast feedback loop: packages and js-runtime targets that stay inside the
# process. Tests that touch the clipboard/keychain, spawn profile-worker,
# bind sockets, or initialize a GPU live under `integration` below. nextest
# parallelizes these independent test binaries when available.
unit:
	$(TEST) --workspace --exclude atomic --exclude automation --exclude js-runtime --exclude net --exclude platform-apis --exclude profile --exclude render --exclude security --exclude webgl
	$(TEST) -p js-runtime --test blob_test --test computed_style_test --test console_test --test css_style_test --test cssom_stylesheet_test --test event_subclasses_test --test layout_measurement_test --test navigation_test --test notifications_test --test permissions_policy_test --test script_limits_test --test timers_test --test trusted_types_test --test web_audio_test

# Full browser-runtime coverage stays separate from the short unit loop. Use
# Cargo directly because its test-name skip syntax is portable and the one
# real clipboard round-trip is known to hang under a headless Xvfb display.
runtime:
	$(CARGO) test -p js-runtime --tests -- --skip write_text_then_read_text_round_trips_through_the_real_os_clipboard

# Full-environment tests are deliberately separate: they exercise real OS
# capabilities and process boundaries, and consequently cost far more than
# a unit-loop run. Keep this as a required CI/merge gate alongside `unit`.
integration:
	$(TEST) -p atomic -p automation -p net -p platform-apis -p render -p security -p webgl
	$(PROFILE_TEST)
	$(TEST) -p js-runtime --test clipboard_test --test cors_test --test csp_test --test document_cookie_test --test fetch_async_test --test fetch_test --test indexed_db_bindings_test --test local_storage_test --test mixed_content_test --test referrer_test

# `test` is the canonical all-tests command. This explicit name makes the
# intended CI gate clear while preserving backwards compatibility.
workspace: test

check:
	$(CARGO) check --workspace

fmt:
	$(CARGO) fmt --all

fmt-check:
	$(CARGO) fmt --all -- --check

lint:
	$(CARGO) clippy --workspace --all-targets

cache-info:
	@echo "Test runner: $(TEST_RUNNER)"
	@echo "Compiler cache: $(SCCACHE_STATUS)"
	@if [ -z "$(SCCACHE_PATH)" ]; then \
		echo "Tip: install sccache to reuse compiler outputs across clean builds and branch switches."; \
	fi

cache-stats:
	@if [ -n "$(SCCACHE_PATH)" ]; then \
		sccache --show-stats; \
	else \
		echo "sccache is not installed; Cargo incremental artifacts are still used in target/."; \
	fi

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
