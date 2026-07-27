#!/usr/bin/env bash
# Scenario 2: gRPC adapter + a load balancer proxying to genuinely separate
# processes. See scenario.md for what this proves.
set -euo pipefail

SCENARIO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib.sh
source "$SCENARIO_DIR/../lib.sh"

LB_ADDR="127.0.0.1:19100"
BACKEND_PORTS=(51000 51001 51002)
BACKEND_NAMES=(Backend-0 Backend-1 Backend-2)

scenario_build_cli

workdir="$(scenario_workdir)"
pg_container=""
main_pid=""
backend_pids=()
cleanup() {
    [ -n "$main_pid" ] && kill "$main_pid" 2>/dev/null || true
    for pid in "${backend_pids[@]:-}"; do
        [ -n "$pid" ] && kill "$pid" 2>/dev/null || true
    done
    [ -n "$pg_container" ] && scenario_stop_postgres "$pg_container"
    scenario_cleanup "$workdir"
}
trap cleanup EXIT

scenario_run "$workdir" "$SCENARIO_DIR/app.tr"

log_step "Checking generated files exist"
for f in \
    main/src/domains/user.rs \
    main/src/infrastructures/storage/psql/user.rs \
    main/src/applications/create_user_use_case.rs \
    main/src/applications/get_user_use_case.rs \
    main/src/adapters/http/load_balancer.rs \
    main/src/adapters/grpc/internal_grpc_api.rs \
    main/proto/internal_grpc_api.proto \
    main/build.rs \
    public_http_backend/src/adapters/http/public_http_api.rs
do
    [ -f "$workdir/out/$f" ] || log_fail "expected generated file missing: $f"
done
log_pass "all expected files were generated (gRPC service + .proto + load balancer + the separate backend app)"

log_step "Checking main.rs never constructs the backend adapter locally"
grep -q 'RoundRobinProxy::new(vec!\["127.0.0.1:51000".to_string(), "127.0.0.1:51001".to_string(), "127.0.0.1:51002".to_string()\])' \
    "$workdir/out/main/src/main.rs" \
    || log_fail "main.rs doesn't proxy to the 3 declared external target addresses"
grep -q "PublicHttpApi::new(" "$workdir/out/main/src/main.rs" \
    && log_fail "main.rs constructs PublicHttpApi locally — the load balancer should be adapter-agnostic"
log_pass "the load balancer's binary only proxies to external addresses, never constructs a backend"

scenario_build_generated "$workdir"
log_pass "both independent binaries (the load balancer/gRPC app and the backend app) compile from one workspace build"

log_step "Starting a throwaway Postgres container with one database per backend"
read -r pg_host pg_port pg_container <<< "$(scenario_start_postgres)"
log_pass "Postgres listening at $pg_host:$pg_port"

log_step "Seeding each backend's own database with a distinguishable row"
docker exec "$pg_container" createdb -U postgres maindb >/dev/null
docker exec "$pg_container" psql -U postgres -d maindb -c \
    "CREATE TABLE users (id BIGINT PRIMARY KEY, name TEXT NOT NULL, email TEXT NOT NULL);" >/dev/null
for i in 0 1 2; do
    db="backend${i}"
    name="${BACKEND_NAMES[$i]}"
    docker exec "$pg_container" createdb -U postgres "$db" >/dev/null
    docker exec "$pg_container" psql -U postgres -d "$db" -c \
        "CREATE TABLE users (id BIGINT PRIMARY KEY, name TEXT NOT NULL, email TEXT NOT NULL);
         INSERT INTO users VALUES (1, '${name}', '${name}@example.com');" >/dev/null
done
log_pass "seeded 3 separate databases (backend0/1/2), each with user id=1 under a different name"

log_step "Starting 3 real OS processes of the SAME public_http_backend binary — one per 'server'"
for i in 0 1 2; do
    port="${BACKEND_PORTS[$i]}"
    db="backend${i}"
    USER_INFRA_DB_URL="postgres://postgres:postgres@${pg_host}:${pg_port}/${db}" PORT="$port" \
        "$SCENARIO_LIB_REPO_ROOT/target/scenario-builds/debug/public_http_backend" \
        > "$workdir/backend${i}.log" 2>&1 &
    backend_pids+=("$!")
    scenario_wait_for_port "127.0.0.1" "$port" "public_http_backend (instance $i)"
done
log_pass "all 3 backend processes are running (distinct PIDs: ${backend_pids[*]}) and listening"

log_step "Starting the load balancer + gRPC binary"
USER_INFRA_DB_URL="postgres://postgres:postgres@${pg_host}:${pg_port}/maindb" \
    "$SCENARIO_LIB_REPO_ROOT/target/scenario-builds/debug/main" \
    > "$workdir/main.log" 2>&1 &
main_pid=$!
scenario_wait_for_port "127.0.0.1" "19100" "main (load balancer)"
log_pass "load balancer is listening on $LB_ADDR, proxying to 3 separate processes"

log_step "Calling the load balancer repeatedly and checking all 3 backend processes actually respond"
declare -A seen
for _ in $(seq 1 6); do
    response="$(curl -sf "http://${LB_ADDR}/users/1")" || {
        cat "$workdir"/backend*.log "$workdir/main.log"
        log_fail "GET /users/1 through the load balancer failed"
    }
    for i in 0 1 2; do
        name="${BACKEND_NAMES[$i]}"
        if grep -qF "\"name\":\"${name}\"" <<< "$response"; then
            seen[$i]=1
        fi
    done
done
for i in 0 1 2; do
    [ "${seen[$i]:-0}" = "1" ] || {
        cat "$workdir"/backend*.log "$workdir/main.log"
        log_fail "never saw a response from backend instance $i (${BACKEND_NAMES[$i]}) across 6 requests"
    }
done
log_pass "round-robin genuinely reached all 3 separate processes — this is real multi-process load balancing, not in-process fan-out"
