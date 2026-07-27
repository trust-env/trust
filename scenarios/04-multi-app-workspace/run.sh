#!/usr/bin/env bash
# Scenario 4: two named `app` blocks compiling into two independent
# binaries/crates inside one generated Cargo workspace. See scenario.md.
set -euo pipefail

SCENARIO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib.sh
source "$SCENARIO_DIR/../lib.sh"

scenario_build_cli

workdir="$(scenario_workdir)"
trap 'scenario_cleanup "$workdir"' EXIT

scenario_run "$workdir" "$SCENARIO_DIR/app.tr"

log_step "Checking the workspace root lists both member crates"
grep -q '\[workspace\]' "$workdir/out/Cargo.toml" || log_fail "out/Cargo.toml isn't a workspace manifest"
grep -qF '"http_api"' "$workdir/out/Cargo.toml" || log_fail "workspace doesn't list the http_api member"
grep -qF '"grpc_worker"' "$workdir/out/Cargo.toml" || log_fail "workspace doesn't list the grpc_worker member"
log_pass "workspace root Cargo.toml lists both members"

log_step "Checking each member has its own package Cargo.toml"
grep -qF 'name = "http_api"' "$workdir/out/http_api/Cargo.toml" || log_fail "http_api/Cargo.toml has wrong package name"
grep -qF 'name = "grpc_worker"' "$workdir/out/grpc_worker/Cargo.toml" || log_fail "grpc_worker/Cargo.toml has wrong package name"
log_pass "each member crate has its own [package] name"

log_step "Checking files shared by both apps were generated in both crates"
for member in http_api grpc_worker; do
    for f in \
        src/domains/task.rs \
        src/infrastructures/storage/psql/task.rs \
        src/applications/get_task_use_case.rs
    do
        [ -f "$workdir/out/$member/$f" ] || log_fail "expected shared file missing from $member: $f"
    done
done
log_pass "shared type/infra/use_case were duplicated into both independent crates"

log_step "Checking each app only contains what its own adapter needs"
[ -f "$workdir/out/http_api/src/adapters/http/http_api_adapter.rs" ] \
    || log_fail "http_api is missing its own HTTP adapter"
[ -f "$workdir/out/http_api/src/applications/create_task_use_case.rs" ] \
    || log_fail "http_api is missing its own CreateTaskUseCase"
[ ! -f "$workdir/out/http_api/src/adapters/grpc/grpc_worker_adapter.rs" ] \
    || log_fail "http_api leaked the grpc_worker's gRPC adapter"

[ -f "$workdir/out/grpc_worker/src/adapters/grpc/grpc_worker_adapter.rs" ] \
    || log_fail "grpc_worker is missing its own gRPC adapter"
[ ! -f "$workdir/out/grpc_worker/src/adapters/http/http_api_adapter.rs" ] \
    || log_fail "grpc_worker leaked the http_api's HTTP adapter"
[ ! -f "$workdir/out/grpc_worker/src/applications/create_task_use_case.rs" ] \
    || log_fail "grpc_worker leaked CreateTaskUseCase, which only http_api's adapter references"
log_pass "partitioning filtered each crate to only what its own app needs (not just duplicated everything)"

log_step "Checking each crate's main.rs spawns only its own app's adapter"
grep -q "HttpApiAdapter" "$workdir/out/http_api/src/main.rs" \
    || log_fail "http_api's main.rs doesn't spawn HttpApiAdapter"
grep -q "GrpcWorkerAdapter" "$workdir/out/grpc_worker/src/main.rs" \
    || log_fail "grpc_worker's main.rs doesn't spawn GrpcWorkerAdapter"
log_pass "each crate's main.rs runs only its own app"

scenario_build_generated "$workdir"
log_pass "both independent binaries compile from one workspace build"
