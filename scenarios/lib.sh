#!/usr/bin/env bash
# Shared helpers for scenarios/*/run.sh. Each run.sh sources this file, then
# calls scenario_run followed by scenario_build (and, for scenarios that need
# extra CLI flags, scenario_run is given them directly).
set -euo pipefail

SCENARIO_LIB_REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

log_step() { echo -e "\n>>> $1"; }
log_pass() { echo -e "${GREEN}PASS${NC}: $1"; }
log_fail() { echo -e "${RED}FAIL${NC}: $1"; exit 1; }

# Builds the trust CLI binary itself (debug profile, matches what the repo's
# own tests build against).
scenario_build_cli() {
    log_step "Building the trust CLI"
    (cd "$SCENARIO_LIB_REPO_ROOT" && cargo build --quiet --bin trust)
}

# Creates a scratch working directory with mod/ symlinked in (the local
# checkouts generated crates' private git dependencies are patched to), and
# prints its path. Every scenario's generated output lands at <workdir>/out.
scenario_workdir() {
    local workdir
    workdir="$(mktemp -d)"
    ln -s "$SCENARIO_LIB_REPO_ROOT/mod" "$workdir/mod"
    echo "$workdir"
}

# Runs the compiled trust binary against a .tr file (given as an absolute
# path) from inside $workdir, forwarding any extra CLI flags (e.g.
# --plugin=..., --ai-base-url=...). Leaves generated output in
# $workdir/out. The CLI's own stdout (a large raw token/AST dump) is noise
# for a pass/fail script, so it's captured to a log and only shown if the
# run actually fails.
scenario_run() {
    local workdir="$1"
    local tr_file="$2"
    shift 2
    local log="$workdir/trust-cli.log"

    log_step "Running trust on $(basename "$tr_file")"
    if ! (cd "$workdir" && "$SCENARIO_LIB_REPO_ROOT/target/debug/trust" "$tr_file" "$@") > "$log" 2>&1; then
        cat "$log"
        log_fail "trust CLI failed on $(basename "$tr_file")"
    fi
}

# Every private git dependency the generator can emit, and the local checkout
# each one should be patched to — mirrors tests/support/mod.rs's
# LOCAL_GIT_DEPENDENCIES so these scripts never need network/SSH access.
# `out_dir` is the generated *workspace* root: each named `app` is its own
# member crate with its own Cargo.toml, so every member manifest is scanned
# for the git URL, but Cargo only honors `[patch]` at the workspace root, so
# the resulting block always goes into the root Cargo.toml.
scenario_patch_local_dependencies() {
    local out_dir="$1"
    local root_cargo_toml="$out_dir/Cargo.toml"

    declare -A deps=(
        ["ssh://git@github.com/BH-World/common.git"]="common"
        ["ssh://git@github.com/BH-World/auth.git"]="auth"
        ["ssh://git@github.com/BH-World/user_permission.git"]="user_permission"
    )

    for url in "${!deps[@]}"; do
        local name="${deps[$url]}"
        if grep -qlF "$url" "$out_dir"/*/Cargo.toml 2>/dev/null; then
            {
                echo ""
                echo "[patch.\"$url\"]"
                echo "$name = { path = \"$SCENARIO_LIB_REPO_ROOT/mod/$name\" }"
            } >> "$root_cargo_toml"
        fi
    done
}

# Patches local deps, then runs a real offline `cargo build` against the
# generated project, sharing one CARGO_TARGET_DIR across scenarios so common
# dependencies (axum, tokio, sqlx, ...) only compile once.
scenario_build_generated() {
    local workdir="$1"
    local out_dir="$workdir/out"

    scenario_patch_local_dependencies "$out_dir"

    log_step "cargo build --offline on the generated project"
    CARGO_TARGET_DIR="$SCENARIO_LIB_REPO_ROOT/target/scenario-builds" \
        cargo build --offline --quiet --manifest-path "$out_dir/Cargo.toml"
}

scenario_cleanup() {
    local workdir="$1"
    rm -rf "$workdir"
}

# Builds the note_plugin example crate (examples/plugins/note_plugin) as a
# real cdylib and prints the path to the resulting .so — mirrors
# tests/e2e_plugin.rs's build_note_plugin() so scenarios exercise the exact
# same dlopen-based --plugin mechanism the test suite already trusts.
scenario_build_note_plugin() {
    local manifest="$SCENARIO_LIB_REPO_ROOT/examples/plugins/note_plugin/Cargo.toml"
    local target_dir="$SCENARIO_LIB_REPO_ROOT/target/scenario-plugin-build"

    log_step "Building the note_plugin example (.so)" >&2
    CARGO_TARGET_DIR="$target_dir" cargo build --offline --quiet --manifest-path "$manifest" >&2

    echo "$target_dir/debug/libnote_plugin.so"
}

# Starts a one-shot mock Anthropic server standing in for the real API
# (mirrors tests/support/mod.rs's mock_anthropic_returning): replies to
# exactly one POST with a fixed `{"content":[{"text": response_text}]}`
# body, so an `ai {}` use case's body becomes literally `response_text`.
# Self-terminates after 20s even if nothing ever connects. Prints the
# server's base URL and its PID (space-separated) on stdout.
scenario_start_mock_anthropic() {
    local response_text="$1"
    local port_file
    port_file="$(mktemp)"

    timeout 20s python3 - "$response_text" "$port_file" > /dev/null 2>&1 <<'PYEOF' &
import http.server
import json
import sys

response_text = sys.argv[1]
port_file = sys.argv[2]


class Handler(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        length = int(self.headers.get("Content-Length", 0))
        self.rfile.read(length)
        body = json.dumps({"content": [{"text": response_text}]}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *args):
        pass


server = http.server.HTTPServer(("127.0.0.1", 0), Handler)
with open(port_file, "w") as f:
    f.write(str(server.server_port))
server.handle_request()
PYEOF
    local pid=$!

    local port=""
    for _ in $(seq 1 50); do
        if [ -s "$port_file" ]; then
            port="$(cat "$port_file")"
            break
        fi
        sleep 0.1
    done
    rm -f "$port_file"
    [ -n "$port" ] || { kill "$pid" 2>/dev/null || true; log_fail "mock Anthropic server never bound a port"; }

    echo "http://127.0.0.1:$port $pid"
}

scenario_stop_mock_anthropic() {
    local pid="$1"
    kill "$pid" 2>/dev/null || true
}

# Starts a throwaway Postgres container (`postgres:16` — cached locally by
# every scenario/e2e test that touches psql storage, so this never needs
# network access) on a docker-assigned free host port, waits for it to
# accept connections, and prints "host port container_id" on stdout. Used by
# scenario 05, which (unlike every other scenario) actually runs its
# generated binaries rather than just compiling them, so it needs a real
# database behind the psql-backed half of the cross-binary call.
scenario_start_postgres() {
    local container_id
    container_id="$(docker run -d --rm -p 0:5432 -e POSTGRES_PASSWORD=postgres postgres:16)"
    local port
    port="$(docker port "$container_id" 5432/tcp | head -1 | sed -E 's/.*:([0-9]+)$/\1/')"

    # The official postgres image starts a *temporary* server to run
    # initdb/init scripts, stops it, then starts the real one — logging
    # "database system is ready to accept connections" both times. Waiting
    # for only `pg_isready` (or the first log line) can catch that
    # temporary server moments before it's torn down, so wait for the
    # message's SECOND occurrence instead.
    for _ in $(seq 1 100); do
        local ready_count
        ready_count="$(docker logs "$container_id" 2>&1 | grep -c "database system is ready to accept connections" || true)"
        if [ "${ready_count:-0}" -ge 2 ] && docker exec "$container_id" pg_isready -U postgres >/dev/null 2>&1; then
            echo "127.0.0.1 $port $container_id"
            return 0
        fi
        sleep 0.2
    done
    docker stop "$container_id" >/dev/null 2>&1 || true
    log_fail "throwaway Postgres container never became ready"
}

scenario_stop_postgres() {
    local container_id="$1"
    docker stop "$container_id" >/dev/null 2>&1 || true
}

# Polls `host:port` (via bash's own `/dev/tcp` pseudo-device — no `nc`
# dependency) until something accepts a connection, or fails the scenario
# after ~10s. Used to know a background-spawned compiled binary has
# actually started listening before scenario 05 curls it.
scenario_wait_for_port() {
    local host="$1"
    local port="$2"
    local label="$3"

    for _ in $(seq 1 50); do
        if (exec 3<>"/dev/tcp/$host/$port") 2>/dev/null; then
            exec 3>&- 3<&- 2>/dev/null || true
            return 0
        fi
        sleep 0.2
    done
    log_fail "$label never started listening on $host:$port"
}
