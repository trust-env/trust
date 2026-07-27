#!/usr/bin/env bash
# Scenario 5: a use case in one app (`HttpApi`) calls, over a real gRPC
# connection, an adapter running in a *different* app (`GrpcWorker`) — the
# structural proof that two of this compiler's generated binaries can
# actually communicate with each other. See scenario.md for what this proves.
set -euo pipefail

SCENARIO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib.sh
source "$SCENARIO_DIR/../lib.sh"

GRPC_ADDR="127.0.0.1:15051"
HTTP_ADDR="127.0.0.1:18080"

scenario_build_cli

workdir="$(scenario_workdir)"
pg_container=""
grpc_worker_pid=""
http_api_pid=""
cleanup() {
    [ -n "$http_api_pid" ] && kill "$http_api_pid" 2>/dev/null || true
    [ -n "$grpc_worker_pid" ] && kill "$grpc_worker_pid" 2>/dev/null || true
    [ -n "$pg_container" ] && scenario_stop_postgres "$pg_container"
    scenario_cleanup "$workdir"
}
trap cleanup EXIT

scenario_run "$workdir" "$SCENARIO_DIR/app.tr"

log_step "Checking generated files exist"
for f in \
    grpc_worker/src/adapters/grpc/grpc_worker_adapter.rs \
    grpc_worker/src/infrastructures/storage/psql/task.rs \
    http_api/src/infrastructures/storage/grpc_client/task.rs \
    http_api/proto/task_client.proto
do
    [ -f "$workdir/out/$f" ] || log_fail "expected generated file missing: $f"
done
log_pass "all expected files were generated (server + client sides of the cross-binary call)"

log_step "Checking http_api's main.rs connects the grpc_client infra, not a psql one"
grep -q 'GrpcTaskClient::connect("127.0.0.1:15051")' "$workdir/out/http_api/src/main.rs" \
    || log_fail "http_api's main.rs doesn't connect RemoteTaskInfra to the GrpcWorker address"
log_pass "http_api's main.rs wires the grpc_client infra to the correct static address"

scenario_build_generated "$workdir"
log_pass "both independent binaries compile from one workspace build"

log_step "Starting a throwaway Postgres container for GrpcWorker's storage"
read -r pg_host pg_port pg_container <<< "$(scenario_start_postgres)"
log_pass "Postgres listening at $pg_host:$pg_port"

log_step "Creating the tasks table and seeding one known row"
docker exec "$pg_container" psql -U postgres -c \
    "CREATE TABLE tasks (id BIGINT PRIMARY KEY, title TEXT NOT NULL);" >/dev/null
docker exec "$pg_container" psql -U postgres -c \
    "INSERT INTO tasks (id, title) VALUES (1, 'Buy milk');" >/dev/null
log_pass "seeded task id=1 title='Buy milk'"

log_step "Starting the grpc_worker binary"
TASK_INFRA_DB_URL="postgres://postgres:postgres@${pg_host}:${pg_port}/postgres" \
    "$SCENARIO_LIB_REPO_ROOT/target/scenario-builds/debug/grpc_worker" \
    > "$workdir/grpc_worker.log" 2>&1 &
grpc_worker_pid=$!
scenario_wait_for_port "127.0.0.1" "15051" "grpc_worker"
log_pass "grpc_worker is listening on $GRPC_ADDR"

log_step "Starting the http_api binary"
"$SCENARIO_LIB_REPO_ROOT/target/scenario-builds/debug/http_api" \
    > "$workdir/http_api.log" 2>&1 &
http_api_pid=$!
scenario_wait_for_port "127.0.0.1" "18080" "http_api"
log_pass "http_api is listening on $HTTP_ADDR"

log_step "Calling http_api's HTTP endpoint, which must round-trip through gRPC to Postgres"
response="$(curl -sf "http://${HTTP_ADDR}/tasks/1")" || {
    cat "$workdir/grpc_worker.log" "$workdir/http_api.log"
    log_fail "GET /tasks/1 failed"
}
echo "response: $response"
grep -qF '"title":"Buy milk"' <<< "$response" || {
    cat "$workdir/grpc_worker.log" "$workdir/http_api.log"
    log_fail "response didn't contain the row seeded directly into GrpcWorker's own Postgres"
}
log_pass "http_api's response came from GrpcWorker's storage over a real gRPC call — cross-binary communication works"

log_step "Checking a nonexistent id correctly 404s (the 'not found' branch, not just the happy path)"
status="$(curl -s -o /dev/null -w '%{http_code}' "http://${HTTP_ADDR}/tasks/999")"
[ "$status" = "404" ] || {
    cat "$workdir/grpc_worker.log" "$workdir/http_api.log"
    log_fail "expected 404 for a nonexistent task id, got $status"
}
log_pass "a missing row on GrpcWorker's side correctly surfaces as 404 through the remote gRPC call"
