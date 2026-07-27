#!/usr/bin/env bash
# Scenario 1: CRUD HTTP + PSQL storage. See scenario.md for what this proves.
set -euo pipefail

SCENARIO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib.sh
source "$SCENARIO_DIR/../lib.sh"

scenario_build_cli

workdir="$(scenario_workdir)"
trap 'scenario_cleanup "$workdir"' EXIT

scenario_run "$workdir" "$SCENARIO_DIR/app.tr"

log_step "Checking generated files exist"
for f in \
    main/src/domains/task.rs \
    main/src/infrastructures/storage/psql/task.rs \
    main/src/applications/create_task_use_case.rs \
    main/src/applications/get_task_use_case.rs \
    main/src/applications/list_tasks_use_case.rs \
    main/src/adapters/http/http_adapter.rs \
    main/src/adapters/http/http_adapter/create_task_use_case_post.rs
do
    [ -f "$workdir/out/$f" ] || log_fail "expected generated file missing: $f"
done
log_pass "all expected files were generated"

scenario_build_generated "$workdir"
log_pass "generated CRUD project compiles"
