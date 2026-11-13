#!/usr/bin/env python3
"""Where does the time go? CPU flame graphs of Trust vs hand-written axum.

For each scenario, loads one contender exactly like bench.py does and samples
its call stacks with `perf` (99 % of the run, 1999 Hz, frame pointers), then
writes:

  dashboard/flame/<app>-<scenario>.svg     interactive flame graph
  dashboard/flame/diff-<scenario>.svg      differential: red = more time in Trust
  dashboard/profile.js                     CPU breakdown by component, Postgres
                                           wait events, for the dashboard

    python3 bench/profile.py                          # trust vs rust-axum, all scenarios
    python3 bench/profile.py --scenarios get --duration 15

perf runs in its own container sharing the app's PID namespace, with only
CAP_PERFMON / CAP_SYS_PTRACE: the host's perf_event_paranoid is left alone.
The profiled builds add frame pointers and line tables (both contenders get
the same treatment), so absolute numbers sit a little below bench.py's.
"""
from __future__ import annotations

import argparse
import collections
import json
import os
import re
import subprocess
import sys
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import bench  # noqa: E402  (reuses its Docker plumbing)

PERF_IMAGE = "trust-bench/perf"
FLAME = bench.BENCH / "dashboard" / "flame"
WORK = bench.BUILD / "profile"
PROF_ENV = ('ENV RUSTFLAGS="-C force-frame-pointers=yes" '
            "CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_PROFILE_RELEASE_STRIP=none")

# Leaf-frame classification: the component a CPU sample was spent *in*.
# First match wins; frames are matched against the function name perf reports.
COMPONENTS = [
    ("kernel", "Noyau (réseau, syscalls)", re.compile(r"_\[k\]$")),
    ("tracing", "Tracing / logs", re.compile(r"tracing|tower_http::trace|env_filter|EnvFilter")),
    ("alloc", "Allocation mémoire", re.compile(r"malloc|free|realloc|calloc|_int_|tcache|cfree|alloc::alloc")),
    ("json", "JSON (serde)", re.compile(r"serde|ryu|itoa")),
    ("sqlx", "Driver Postgres (sqlx)", re.compile(r"sqlx")),
    ("http", "HTTP (hyper)", re.compile(r"hyper|httparse|\bhttp::|h2::|bytes::")),
    ("router", "Routage (axum / tower)", re.compile(r"axum|tower|matchit")),
    ("runtime", "Runtime async (tokio)", re.compile(r"tokio|mio|futures|parking_lot|std::sync|core::task")),
    ("app", "Code de l'app", re.compile(r"^main::|rust_axum_bench|^main$")),
    ("libc", "libc (mémoire, chaînes)", re.compile(r"mem(cpy|move|set|cmp)|str(len|cmp)|libc")),
]
OTHER = ("other", "Autre")
# tracing's generic future wrapper: its inclusive time is the wrapped work, not tracing.
WRAPPER = re.compile(r"Instrumented<.*> as core::future::future::Future>::poll|drop_in_place<tracing::instrument::Instrumented")


def classify(leaf: str) -> str:
    if WRAPPER.search(leaf):
        return "runtime"  # inlined async machinery of the wrapped future, not tracing work
    for key, _, rx in COMPONENTS:
        if rx.search(leaf):
            return key
    return OTHER[0]


def breakdown(folded: Path) -> dict:
    """Self time by component, plus the share of samples with any tracing frame."""
    self_time: collections.Counter = collections.Counter()
    tracing_incl = total = 0
    top: collections.Counter = collections.Counter()
    for line in folded.read_text().splitlines():
        stack, _, n = line.rpartition(" ")
        if not stack:
            continue
        n = int(n)
        frames = stack.split(";")
        total += n
        self_time[classify(frames[-1])] += n
        top[frames[-1]] += n
        # Time inside tracing itself (span creation, enter/exit, filtering): a stack counts
        # once it reaches a tracing function other than the generic future wrapper,
        # whose inclusive time is just the work the span wraps (e.g. a whole SQL query).
        if any(COMPONENTS[1][2].search(f) and not WRAPPER.search(f) for f in frames):
            tracing_incl += n
    pct = lambda v: round(100 * v / total, 2) if total else 0
    return {
        "samples": total,
        "self_pct": {k: pct(v) for k, v in self_time.items()},
        "tracing_cost_pct": pct(tracing_incl),
        "top_functions": [{"name": f, "pct": pct(v)} for f, v in top.most_common(12)],
    }


def build_prof_image(app: dict, ctx: Path) -> str:
    """The bench image, rebuilt with frame pointers and line tables."""
    WORK.mkdir(parents=True, exist_ok=True)
    src = (bench.BENCH / "apps" / app["dir"] / "Dockerfile").read_text()
    dockerfile = WORK / f"{app['dir']}.Dockerfile"
    dockerfile.write_text(src.replace("WORKDIR /src\n", f"WORKDIR /src\n{PROF_ENV}\n", 1))
    tag = f"trust-bench/{app['dir']}:prof"
    bench.log(f"docker build {tag}")
    bench.sh("docker", "build", "-q", "-f", str(dockerfile), "-t", tag, str(ctx))
    return tag


class PgWaits(threading.Thread):
    """Samples what Postgres backends are waiting on while a load runs."""

    SQL = ("SELECT coalesce(wait_event_type || ':' || wait_event, 'CPU') FROM pg_stat_activity "
           "WHERE datname = 'bench' AND state = 'active' AND pid <> pg_backend_pid()")

    def __init__(self):
        super().__init__(daemon=True)
        self.counts: collections.Counter = collections.Counter()
        self.stop_flag = threading.Event()

    def run(self):
        while not self.stop_flag.is_set():
            out = subprocess.run(["docker", "exec", bench.PG, "psql", "-At", "-U", "bench", "-d", "bench",
                                  "-c", self.SQL], capture_output=True, text=True).stdout
            self.counts.update(l for l in out.splitlines() if l)
            time.sleep(0.05)

    def result(self) -> dict:
        total = sum(self.counts.values())
        return {k: round(100 * v / total, 1) for k, v in self.counts.most_common(8)} if total else {}


def profile_point(app: dict, scenario: dict, c: int, args) -> dict:
    name = f"{app['id']}-{scenario['id']}"
    out = WORK / "out"
    out.mkdir(parents=True, exist_ok=True)
    total = args.warmup + args.duration + 2
    load = subprocess.Popen(
        ["docker", "run", "--rm", "--network", bench.NETWORK, "--cpuset-cpus", args.load_cpus, bench.OHA_IMAGE,
         "--no-tui", "--output-format", "json", "-z", f"{total}s", "-c", str(c), *scenario["args"]],
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    time.sleep(args.warmup)
    waits = PgWaits()
    waits.start()
    # perf container: same PID namespace as the app (its PID 1), its own CPU so
    # sampling overhead lands outside the app's cpuset.
    script = (f"perf-bin record -q -F 1999 -g -p 1 -o /out/{name}.data -- sleep {args.duration} >/dev/null 2>&1; "
              f"perf-bin script --no-inline -i /out/{name}.data 2>/dev/null | /fg/stackcollapse-perf.pl --kernel > /out/{name}.folded; "
              f"chown -R {os.getuid()}:{os.getgid()} /out")
    # perf resolves symbols by path in its *own* filesystem: give it the app's
    # binary at the same path (/app), and CAP_SYSLOG for kernel symbols.
    binary = out / f"{app['id']}.bin"
    if not binary.exists():
        bench.sh("docker", "cp", f"{bench.APP}:/app", str(binary))
    bench.sh("docker", "run", "--rm", f"--pid=container:{bench.APP}", "--cap-add", "PERFMON",
             "--cap-add", "SYS_PTRACE", "--cap-add", "SYSLOG", "--cpuset-cpus", args.perf_cpus,
             "-v", f"{out}:/out", "-v", f"{binary}:/app:ro", PERF_IMAGE, "sh", "-c", script)
    waits.stop_flag.set()
    waits.join()
    raw = load.communicate()[0]
    rps = json.loads(raw[raw.index("{"):])["summary"]["requestsPerSec"]
    folded = out / f"{name}.folded"
    info = breakdown(folded)
    info["rps"] = round(rps, 1)
    if scenario["id"] == "create":
        info["pg_waits_pct"] = waits.result()
    FLAME.mkdir(parents=True, exist_ok=True)
    title = f"{app['name']} · {scenario['label']} · {c} connexions"
    bench.sh("docker", "run", "--rm", "--user", f"{os.getuid()}:{os.getgid()}",
                 "-v", f"{out}:/out", "-v", f"{FLAME}:/flame", PERF_IMAGE, "sh", "-c",
             f"/fg/flamegraph.pl --title '{title}' --width 1400 --colors rust /out/{name}.folded > /flame/{name}.svg")
    bench.log(f"  {name}: {info['samples']} samples, {rps:,.0f} req/s, "
              f"tracing cost {info["tracing_cost_pct"]} %")
    return info


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--apps", default="trust,rust-axum", help="exactly two, the first is the one diffed")
    ap.add_argument("--scenarios", default="ping,get,list,create")
    ap.add_argument("--connections", type=int, default=64)
    ap.add_argument("--duration", type=float, default=10)
    ap.add_argument("--warmup", type=float, default=3)
    ap.add_argument("--app-cpus", default="1,3")
    ap.add_argument("--pg-cpus", default="6,8,10")
    ap.add_argument("--load-cpus", default="12-19")
    ap.add_argument("--perf-cpus", default="20,21")
    ap.add_argument("--memory", default="1g")
    ap.add_argument("--db-pool", type=int, default=32)
    args = ap.parse_args()

    ids = args.apps.split(",")
    apps = [next(a for a in bench.APPS if a["id"] == i) for i in ids]
    scenarios = [s for s in bench.SCENARIOS if s["id"] in args.scenarios.split(",")]

    bench.log("docker build trust-bench/perf")
    bench.sh("docker", "build", "-q", "-t", PERF_IMAGE, str(bench.BENCH / "profiling"))
    images = {}
    for app in apps:
        ctx = bench.prepare_trust() if app["dir"] == "trust" else bench.BENCH / "apps" / app["dir"]
        images[app["id"]] = build_prof_image(app, ctx)

    report = {"meta": {"date": time.strftime("%Y-%m-%dT%H:%M:%S%z"), "connections": args.connections,
                       "duration_s": args.duration, "frequency_hz": 1999, "apps": ids,
                       "components": [{"id": k, "label": l} for k, l, _ in COMPONENTS] + [
                           {"id": OTHER[0], "label": OTHER[1]}],
                       "scenarios": [{k: s[k] for k in ("id", "label")} for s in scenarios]},
              "profiles": {}}
    bench.start_postgres(args.pg_cpus)
    try:
        for app in apps:
            bench.reset_db()
            bench.start_app(app, images[app["id"]], args.app_cpus, args.memory, args.db_pool)
            bench.check_contract()
            bench.log(f"{app['id']}: profiling")
            for scenario in scenarios:
                bench.wait_quiet(2.0, 300)
                report["profiles"][f"{app['id']}-{scenario['id']}"] = profile_point(app, scenario, args.connections, args)
            bench.sh("docker", "rm", "-f", bench.APP, check=False)
    finally:
        bench.sh("docker", "rm", "-f", bench.APP, bench.PG, check=False)

    a, b = ids
    out = WORK / "out"
    for scenario in scenarios:
        s = scenario["id"]
        # -n normalises sample counts, so the diff shows shares, not throughput.
        bench.sh("docker", "run", "--rm", "--user", f"{os.getuid()}:{os.getgid()}",
                 "-v", f"{out}:/out", "-v", f"{FLAME}:/flame", PERF_IMAGE, "sh", "-c",
                 f"/fg/difffolded.pl -n /out/{b}-{s}.folded /out/{a}-{s}.folded | /fg/flamegraph.pl "
                 f"--title 'Différentiel {scenario['label']} : rouge = plus de temps dans {a}' --width 1400 "
                 f"> /flame/diff-{s}.svg")
    payload = json.dumps(report, indent=2)
    (bench.RESULTS / "profile.json").write_text(payload)
    (bench.BENCH / "dashboard" / "profile.js").write_text(f"window.BENCH_PROFILE = {payload};\n")
    bench.log("wrote dashboard/flame/*.svg, results/profile.json and dashboard/profile.js")
    return 0


if __name__ == "__main__":
    sys.exit(main())
