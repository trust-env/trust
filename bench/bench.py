#!/usr/bin/env python3
"""Trust performance benchmark.

Builds the same small API (ping, get by id, paged list, create) in Trust and
in hand-written stacks, runs each one in Docker on pinned CPUs against one
Postgres, loads it with oha, and writes a JSON report the dashboard reads.

    python3 bench/bench.py                      # everything, default settings
    python3 bench/bench.py --apps trust,rust-axum --duration 5 --connections 64
    python3 bench/bench.py --no-cache           # also time cold image builds

Only the standard library and Docker are needed. See bench/README.md.
"""
from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import threading
import time
import urllib.request
from pathlib import Path

BENCH = Path(__file__).resolve().parent
REPO = BENCH.parent
BUILD = BENCH / ".build"
RESULTS = BENCH / "results"

NETWORK = "trust-bench"
PG = "trust-bench-pg"
APP = "trust-bench-app"
LOAD = "trust-bench-load"
# Every bench container runs under this cgroup, so the CPU time of everything
# else on the bench CPUs can be told apart exactly.
SLICE = "trustbench.slice"
PG_IMAGE = "postgres:17-alpine"
OHA_IMAGE = "ghcr.io/hatoo/oha:1.16.0"
HOST_PORT = 18080

# id, label, family, image dir, extra env, hand-written source files.
APPS = [
    {"id": "trust", "name": "Trust", "family": "Rust (generated)", "dir": "trust",
     "env": {}, "sources": ["app.tr", "ping.rs"]},
    {"id": "rust-axum", "name": "Rust · axum", "family": "Rust", "dir": "rust-axum",
     "env": {}, "sources": ["src/main.rs"]},
    {"id": "go", "name": "Go · net/http", "family": "Go", "dir": "go",
     "env": {}, "sources": ["main.go"]},
    {"id": "fastify", "name": "Node · Fastify", "family": "Node.js", "dir": "fastify",
     "env": {}, "workers": True, "sources": ["server.js"]},
    {"id": "nestjs-express", "name": "NestJS · Express", "family": "Node.js", "dir": "nestjs",
     "env": {"PLATFORM": "express"}, "workers": True, "sources": ["src/main.ts"]},
    {"id": "nestjs-fastify", "name": "NestJS · Fastify", "family": "Node.js", "dir": "nestjs",
     "env": {"PLATFORM": "fastify"}, "workers": True, "sources": ["src/main.ts"]},
    {"id": "fastapi", "name": "Python · FastAPI", "family": "Python", "dir": "fastapi",
     "env": {}, "workers": True, "sources": ["main.py"]},
]

SEED_ROWS = 10_000
# Cores' worth of foreign CPU time on the app's CPUs above which a run is flagged.
NOISY_INTERFERENCE = 0.1
# Same on the app's hyperthread siblings: an active desktop (shell, browser,
# terminal) already puts about 0.2 core there, a background build far more.
NOISY_SIBLING = 0.35
# Same, summed over the Postgres and load-generator CPUs (they have headroom).
NOISY_REST = 0.6
# Scheduling priority of the bench containers (see boost); --nice overrides it.
NICE = -10
CREATE_BODY = '{"name":"Ada Lovelace","email":"ada@example.com","age":36}'
# oha's --rand-regex-url draws a fresh URL per request (ids 1..9999).
SCENARIOS = [
    {"id": "ping", "label": "GET /ping", "desc": "Framework overhead: routing + JSON, no I/O",
     "args": ["http://app:8080/ping"]},
    {"id": "get", "label": "GET /users/:id", "desc": "Primary-key read, random id among 10k rows",
     "args": ["--rand-regex-url", "http://app:8080/users/[1-9][0-9]{0,3}"]},
    {"id": "list", "label": "GET /users?limit=20", "desc": "Paged read of 20 rows at a random offset",
     "args": ["--rand-regex-url", r"http://app:8080/users\?limit=20&offset=[0-9]{1,3}"]},
    {"id": "create", "label": "POST /users", "desc": "JSON body validation + INSERT … RETURNING",
     "args": ["-m", "POST", "-H", "content-type: application/json", "-d", CREATE_BODY,
              "http://app:8080/users"]},
]


def log(msg: str) -> None:
    print(f"[bench {time.strftime('%H:%M:%S')}] {msg}", flush=True)


def sh(*cmd: str, check: bool = True, capture: bool = True, **kw) -> str:
    proc = subprocess.run(cmd, check=False, text=True,
                          stdout=subprocess.PIPE if capture else None,
                          stderr=subprocess.STDOUT if capture else None, **kw)
    if check and proc.returncode != 0:
        raise RuntimeError(f"{' '.join(cmd)} failed ({proc.returncode}):\n{proc.stdout or ''}")
    return proc.stdout or ""


def cpu_list(spec: str) -> list[int]:
    out = []
    for part in spec.split(","):
        a, _, b = part.partition("-")
        out.extend(range(int(a), int(b or a) + 1))
    return out


# ---------------------------------------------------------------- building

def prepare_trust() -> Path:
    """Compile the trust CLI, generate the bench app, vendor private crates."""
    log("building the trust compiler (release)")
    sh("cargo", "build", "--release", "--quiet", cwd=REPO, capture=False)
    ctx = BUILD / "trust"
    if ctx.exists():
        shutil.rmtree(ctx)
    ctx.mkdir(parents=True)
    log("generating bench/apps/trust/app.tr")
    sh(str(REPO / "target/release/trust"), str(BENCH / "apps/trust/app.tr"),
       f"--out={ctx / 'out'}", "--vendor")
    shutil.copy(BENCH / "apps/trust/Dockerfile", ctx / "Dockerfile")
    return ctx


def build_image(app: dict, ctx: Path, no_cache: bool) -> dict:
    tag = f"trust-bench/{app['dir']}"
    log(f"docker build {tag}{' (no cache)' if no_cache else ''}")
    t0 = time.monotonic()
    sh("docker", "build", "-q", *(["--no-cache"] if no_cache else []), "-t", tag, str(ctx))
    seconds = time.monotonic() - t0
    size = int(sh("docker", "image", "inspect", "--format", "{{.Size}}", tag).strip())
    return {"image": tag, "build_s": round(seconds, 1), "build_cold": no_cache,
            "image_mb": round(size / 1e6, 1)}


def count_loc(paths: list[Path]) -> int:
    comment = re.compile(r"^\s*(//|#|--|/\*|\*|\"\"\")")
    total = 0
    for p in paths:
        for line in p.read_text().splitlines():
            if line.strip() and not comment.match(line):
                total += 1
    return total


def generated_loc(ctx: Path) -> int:
    files = [p for p in (ctx / "out").rglob("*")
             if p.is_file() and "vendor" not in p.parts and p.suffix in {".rs", ".sql", ".toml"}]
    return count_loc(files)


# ---------------------------------------------------------------- runtime

def cgroup(container: str) -> Path:
    cid = sh("docker", "inspect", "--format", "{{.Id}}", container).strip()
    for candidate in (Path(f"/sys/fs/cgroup/{SLICE}/docker-{cid}.scope"),
                      Path(f"/sys/fs/cgroup/system.slice/docker-{cid}.scope"),
                      Path(f"/sys/fs/cgroup/docker/{cid}")):
        if candidate.exists():
            return candidate
    raise RuntimeError(f"cgroup of {container} not found (cgroup v2 + systemd driver expected)")


def cpu_usec(cg: Path) -> int:
    for line in (cg / "cpu.stat").read_text().splitlines():
        if line.startswith("usage_usec"):
            return int(line.split()[1])
    return 0


def mem_bytes(cg: Path) -> int:
    return int((cg / "memory.current").read_text())


def cpu_mhz(cpu: int) -> float | None:
    f = Path(f"/sys/devices/system/cpu/cpu{cpu}/cpufreq/scaling_cur_freq")
    return int(f.read_text()) / 1000 if f.exists() else None


def cpu_mhz_max(cpu: int) -> float | None:
    f = Path(f"/sys/devices/system/cpu/cpu{cpu}/cpufreq/cpuinfo_max_freq")
    return int(f.read_text()) / 1000 if f.exists() else None


def power_source() -> str:
    """'ac', 'battery', or 'unknown' (desktop, VM): laptops cap CPU power on battery."""
    supplies = list(Path("/sys/class/power_supply").glob("*"))
    for d in supplies:
        if (d / "type").exists() and (d / "type").read_text().strip() == "Mains":
            return "ac" if (d / "online").read_text().strip() == "1" else "battery"
    return "unknown"


def slice_usec() -> int:
    d = Path("/sys/fs/cgroup") / SLICE
    return cpu_usec(d) if (d / "cpu.stat").exists() else 0


def cpu_busy_seconds(cpus: list[int] | None = None) -> float:
    """Busy time (all but idle/iowait) of some CPUs, or of all of them, from /proc/stat."""
    hz = os.sysconf("SC_CLK_TCK")
    busy = 0
    for line in Path("/proc/stat").read_text().splitlines():
        name, *fields = line.split()
        if not name.startswith("cpu") or name == "cpu":
            continue
        if cpus is not None and int(name[3:]) not in cpus:
            continue
        # user + nice + system only: irq/softirq time is mostly the network
        # stack serving the app's own traffic, never charged to its cgroup.
        values = [int(v) for v in fields[:3]]
        busy += sum(values)
    return busy / hz


def boost(container: str, nice: int) -> str:
    """Raise the scheduling priority of every thread in a container.

    The bench CPUs are not isolated: a background `cargo build` (often niced)
    still lands on them. At nice -10 against a nice 5 build, the scheduler
    gives the bench ~28x the CPU weight. Threads created later inherit it.
    """
    if not nice:
        return ""
    return sh("docker", "exec", "--privileged", "-u", "0", container, "sh", "-c",
       f"for t in /proc/[0-9]*/task/[0-9]*; do renice -n {nice} -p ${{t##*/}} >/dev/null 2>&1; done; "
       "cut -d' ' -f19 /proc/1/stat")


def host_busy_cores(seconds: float = 1.0) -> float:
    b0 = cpu_busy_seconds()
    time.sleep(seconds)
    return (cpu_busy_seconds() - b0) / seconds


def wait_quiet(max_busy: float, timeout: float) -> float:
    """Wait (between measurements, so the bench itself is idle) until the rest
    of the machine uses at most `max_busy` cores. Returns the last reading."""
    deadline = time.monotonic() + timeout
    busy = host_busy_cores()
    announced = False
    while busy > max_busy and time.monotonic() < deadline:
        if not announced:
            log(f"  machine busy ({busy:.1f} cores used elsewhere), waiting for it to calm down")
            announced = True
        time.sleep(5)
        busy = host_busy_cores()
    return busy


def siblings(cpus: list[int]) -> list[int]:
    """Hyperthread siblings of some CPUs (sharing their physical core), excluding them."""
    out = set()
    for c in cpus:
        f = Path(f"/sys/devices/system/cpu/cpu{c}/topology/thread_siblings_list")
        if f.exists():
            out.update(cpu_list(f.read_text().strip()))
    return sorted(out - set(cpus))


class Sampler(threading.Thread):
    """Peak memory and CPU time of the app while a load runs, and what other
    programs took meanwhile, from three places:

    - the app's own CPUs (time stolen outright),
    - their hyperthread siblings (sharing the app's physical cores),
    - the Postgres and load-generator CPUs (slowing the rest of the chain).

    Every bench container lives under SLICE, so "other" is exact: busy time on
    those CPUs minus what the slice used.
    """

    def __init__(self, cg: Path, args):
        super().__init__(daemon=True)
        self.cg, self.peak, self.stop_flag = cg, 0, threading.Event()
        self.app_cpus = cpu_list(args.app_cpus)
        self.sib_cpus = siblings(self.app_cpus)
        self.rest_cpus = sorted(set(cpu_list(args.pg_cpus)) | set(cpu_list(args.load_cpus)))

    def run(self):
        self.mhz: list[float] = []
        while not self.stop_flag.is_set():
            self.peak = max(self.peak, mem_bytes(self.cg))
            self.mhz.extend(m for m in (cpu_mhz(c) for c in self.app_cpus) if m)
            time.sleep(0.1)

    def __enter__(self):
        self.t0, self.cpu0 = time.monotonic(), cpu_usec(self.cg)
        self.busy0 = cpu_busy_seconds(self.app_cpus)
        self.sib0 = cpu_busy_seconds(self.sib_cpus) if self.sib_cpus else 0.0
        self.rest0, self.slice0 = cpu_busy_seconds(self.rest_cpus), slice_usec()
        self.start()
        return self

    def __exit__(self, *exc):
        self.stop_flag.set()
        self.join()
        self.cpu_seconds = (cpu_usec(self.cg) - self.cpu0) / 1e6
        self.avg_mhz = sum(self.mhz) / len(self.mhz) if self.mhz else None
        self.wall = time.monotonic() - self.t0
        foreign = cpu_busy_seconds(self.app_cpus) - self.busy0 - self.cpu_seconds
        self.interference_cores = max(0.0, foreign / self.wall)
        sib = (cpu_busy_seconds(self.sib_cpus) - self.sib0) if self.sib_cpus else 0.0
        self.sibling_cores = max(0.0, sib / self.wall)
        others_in_slice = (slice_usec() - self.slice0) / 1e6 - self.cpu_seconds  # postgres + oha
        rest = cpu_busy_seconds(self.rest_cpus) - self.rest0 - others_in_slice
        self.rest_cores = max(0.0, rest / self.wall)


def http(method: str, path: str, body: str | None = None) -> tuple[int, object]:
    req = urllib.request.Request(f"http://127.0.0.1:{HOST_PORT}{path}", method=method,
                                 data=body.encode() if body else None,
                                 headers={"content-type": "application/json"})
    with urllib.request.urlopen(req, timeout=5) as resp:
        return resp.status, json.loads(resp.read() or b"null")


def start_postgres(cpus: str) -> None:
    sh("docker", "rm", "-f", PG, check=False)
    sh("docker", "network", "create", NETWORK, check=False)
    log(f"starting {PG_IMAGE} on cpus {cpus}")
    sh("docker", "run", "-d", "--name", PG, "--cgroup-parent", SLICE, "--network", NETWORK, "--network-alias", "db",
       "--cpuset-cpus", cpus, "--shm-size", "512m",
       "-e", "POSTGRES_USER=bench", "-e", "POSTGRES_PASSWORD=bench", "-e", "POSTGRES_DB=bench",
       PG_IMAGE, "-c", "max_connections=300", "-c", "shared_buffers=512MB")
    for _ in range(120):
        if subprocess.run(["docker", "exec", PG, "pg_isready", "-U", "bench", "-d", "bench"],
                          capture_output=True).returncode == 0:
            # pg_isready answers during the init restart too; a real query settles it.
            if subprocess.run(["docker", "exec", PG, "psql", "-U", "bench", "-d", "bench", "-c", "select 1"],
                              capture_output=True).returncode == 0:
                boost(PG, NICE)  # backends forked later inherit the postmaster's priority
                return
        time.sleep(0.5)
    raise RuntimeError("postgres did not start")


def reset_db() -> None:
    sql = f"""
        DROP TABLE IF EXISTS users;
        CREATE TABLE users (id BIGSERIAL PRIMARY KEY, name TEXT NOT NULL,
                            email TEXT NOT NULL, age BIGINT NOT NULL);
        INSERT INTO users (name, email, age)
            SELECT 'user ' || g, 'user' || g || '@example.com', 18 + g % 60
            FROM generate_series(1, {SEED_ROWS}) g;
        VACUUM ANALYZE users;
        CHECKPOINT;
    """
    # Through stdin, not -c: a multi-statement -c is one transaction, and VACUUM refuses that.
    sh("docker", "exec", "-i", PG, "psql", "-q", "-v", "ON_ERROR_STOP=1", "-U", "bench", "-d", "bench",
       input=sql)


def start_app(app: dict, image: str, cpus: str, memory: str, pool: int) -> float:
    sh("docker", "rm", "-f", APP, check=False)
    workers = len(cpu_list(cpus)) if app.get("workers") else 1
    env = {"DATABASE_URL": "postgres://bench:bench@db:5432/bench", "APP_PORT": "8080",
           "DB_POOL": str(pool), "DATABASE_POOL_SIZE": str(pool), "WORKERS": str(workers), **app["env"]}
    t0 = time.monotonic()
    sh("docker", "run", "-d", "--name", APP, "--cgroup-parent", SLICE, "--network", NETWORK, "--network-alias", "app",
       "--cpuset-cpus", cpus, "--memory", memory, "-p", f"127.0.0.1:{HOST_PORT}:8080",
       *[x for k, v in env.items() for x in ("-e", f"{k}={v}")], image)
    deadline = t0 + 60
    while time.monotonic() < deadline:
        try:
            if http("GET", "/ping")[0] == 200:
                startup = (time.monotonic() - t0) * 1000
                boost(APP, NICE)
                return startup
        except Exception:
            pass
        time.sleep(0.01)
    raise RuntimeError(f"{app['id']} did not answer /ping within 60s:\n{sh('docker', 'logs', APP, check=False)}")


def check_contract() -> None:
    """Every contender must answer the same shapes, or the comparison is moot."""
    assert http("GET", "/ping") == (200, {"ok": True}), "ping"
    status, body = http("GET", "/users/42")
    assert status == 200 and body["user"] == {"id": 42, "name": "user 42",
                                              "email": "user42@example.com", "age": 18 + 42 % 60}, body
    status, body = http("GET", "/users?limit=5&offset=10")
    assert status == 200 and [u["id"] for u in body["users"]] == [11, 12, 13, 14, 15], body
    status, body = http("POST", "/users", CREATE_BODY)
    assert status in (200, 201) and body["user"]["name"] == "Ada Lovelace" and body["user"]["id"] > SEED_ROWS, body
    try:
        http("GET", "/users/999999999")
        raise AssertionError("missing user must be 404")
    except urllib.error.HTTPError as e:
        assert e.code == 404, e.code


def oha(scenario: dict, connections: int, seconds: float, cpus: str) -> dict:
    # Detached, so its threads can be reniced like the app's before the load ramps up.
    sh("docker", "rm", "-f", LOAD, check=False)
    sh("docker", "run", "-d", "--name", LOAD, "--cgroup-parent", SLICE, "--network", NETWORK,
       "--cpuset-cpus", cpus, OHA_IMAGE, "--no-tui", "--output-format", "json",
       "-z", f"{seconds}s", "-c", str(connections), *scenario["args"])
    try:
        try:
            boost(LOAD, NICE)
        except RuntimeError:
            pass  # a very short run may already be over
        sh("docker", "wait", LOAD)
        out = sh("docker", "logs", LOAD)
        return json.loads(out[out.index("{"):])
    finally:
        sh("docker", "rm", "-f", LOAD, check=False)


def summarize(raw: dict) -> dict:
    s, p = raw["summary"], raw.get("latencyPercentiles", {})
    ms = lambda v: round(v * 1000, 3) if v is not None else None
    codes = {str(k): v for k, v in raw.get("statusCodeDistribution", {}).items()}
    errors = {k: v for k, v in raw.get("errorDistribution", {}).items() if "deadline" not in k.lower()}
    ok = sum(v for k, v in codes.items() if k.startswith("2"))
    total = sum(codes.values()) + sum(errors.values())
    return {
        "rps": round(s["requestsPerSec"], 1),
        "requests": total,
        "success_rate": round(ok / total, 5) if total else 0,
        "latency_ms": {"avg": ms(s.get("average")), "p50": ms(p.get("p50")), "p90": ms(p.get("p90")),
                        "p99": ms(p.get("p99")), "p999": ms(p.get("p99.9")), "max": ms(s.get("slowest"))},
        "status_codes": codes,
        "errors": errors,
    }


# ---------------------------------------------------------------- main

def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--apps", default=",".join(a["id"] for a in APPS), help="comma-separated app ids")
    ap.add_argument("--scenarios", default=",".join(s["id"] for s in SCENARIOS))
    ap.add_argument("--connections", default="16,64,256", help="concurrency levels, comma-separated")
    ap.add_argument("--duration", type=float, default=10, help="seconds per measured run")
    ap.add_argument("--warmup", type=float, default=3, help="seconds of unmeasured load first")
    ap.add_argument("--app-cpus", default="1,3", help="cpuset of the app container")
    ap.add_argument("--pg-cpus", default="6,8,10", help="cpuset of postgres")
    ap.add_argument("--load-cpus", default="12-19", help="cpuset of the load generator")
    ap.add_argument("--memory", default="1g", help="memory limit of the app container")
    ap.add_argument("--db-pool", type=int, default=32, help="total DB connections per app")
    ap.add_argument("--no-cache", action="store_true", help="cold docker builds (times real build cost)")
    ap.add_argument("--keep", action="store_true", help="leave postgres running afterwards")
    ap.add_argument("--max-busy", type=float, default=2.0,
                    help="before each measurement, wait until the rest of the machine uses at most this many "
                         "cores (0 disables the wait)")
    ap.add_argument("--noisy-retries", type=int, default=2,
                    help="measure a point again (up to this many times) when other processes disturbed it")
    ap.add_argument("--merge", action="store_true",
                    help="keep the other contenders of results/latest.json and replace only the ones run now")
    ap.add_argument("--quiet-timeout", type=float, default=600,
                    help="seconds to wait for a quiet machine before measuring anyway (flagged noisy)")
    ap.add_argument("--retry-noisy", action="store_true",
                    help="measure again only the points results/latest.json flags noisy (no rebuild)")
    ap.add_argument("--allow-battery", action="store_true",
                    help="measure even when a laptop runs on battery (results swing with the CPU clock)")
    ap.add_argument("--nice", type=int, default=NICE,
                    help="nice value given to the app and postgres threads (0 leaves them alone)")
    args = ap.parse_args()
    globals()["NICE"] = args.nice
    pin_harness(args)
    if power_source() == "battery" and not args.allow_battery:
        print("The machine runs on battery: a laptop then caps CPU power, and the app's clock swings "
              "(1.3-3.4 GHz seen instead of 4.8), which moves results by up to 2x. Plug it in, or pass "
              "--allow-battery to measure anyway (the report records it).", file=sys.stderr)
        return 2
    if args.retry_noisy:
        return retry_noisy(args)

    wanted = args.apps.split(",")
    apps = [a for a in APPS if a["id"] in wanted]
    scenarios = [s for s in SCENARIOS if s["id"] in args.scenarios.split(",")]
    levels = [int(c) for c in args.connections.split(",")]
    if not apps or not scenarios:
        ap.error("no app or scenario selected")

    commit = sh("git", "rev-parse", "--short", "HEAD", cwd=REPO, check=False).strip()
    cpu_model = next((l.split(":", 1)[1].strip() for l in Path("/proc/cpuinfo").read_text().splitlines()
                      if l.startswith("model name")), platform.processor())
    report = {
        "meta": {
            "date": dt.datetime.now().astimezone().isoformat(timespec="seconds"),
            "trust_commit": commit,
            "host": {"cpu": cpu_model, "threads": os.cpu_count(), "kernel": platform.release(),
                     "power": power_source(),
                     "max_mhz": cpu_mhz_max(cpu_list(args.app_cpus)[0]),
                     "docker": sh("docker", "version", "--format", "{{.Server.Version}}", check=False).strip()},
            "config": {"duration_s": args.duration, "warmup_s": args.warmup, "connections": levels,
                       "app_cpus": args.app_cpus, "pg_cpus": args.pg_cpus, "load_cpus": args.load_cpus,
                       "memory": args.memory, "db_pool": args.db_pool, "seed_rows": SEED_ROWS,
                       "postgres": PG_IMAGE, "load_generator": OHA_IMAGE, "nice": args.nice},
            "scenarios": [{k: s[k] for k in ("id", "label", "desc")} for s in scenarios],
        },
        "apps": [],
        "runs": [],
    }

    contexts = {}
    if any(a["dir"] == "trust" for a in apps):
        contexts["trust"] = prepare_trust()
    built: dict[str, dict] = {}
    for app in apps:
        if app["dir"] not in built:
            ctx = contexts.get(app["dir"], BENCH / "apps" / app["dir"])
            built[app["dir"]] = build_image(app, ctx, args.no_cache)

    stamp = dt.datetime.now().strftime("%Y%m%d-%H%M%S")
    previous_apps: dict[str, dict] = {}
    if args.merge and (RESULTS / "latest.json").exists():
        # Re-measure some contenders and keep everyone else from the last report.
        previous = json.loads((RESULTS / "latest.json").read_text())
        previous_apps = {a["id"]: a for a in previous["apps"]}
        rerun = {a["id"] for a in apps}
        report["apps"] = [a for a in previous["apps"] if a["id"] not in rerun]
        report["runs"] = [r for r in previous["runs"] if r["app"] not in rerun]
        report["meta"]["merged_from"] = previous["meta"].get("date")

    start_postgres(args.pg_cpus)
    try:
        for app in apps:
            info = {k: app[k] for k in ("id", "name", "family")} | built[app["dir"]]
            old = previous_apps.get(app["id"], {})
            if not info["build_cold"] and old.get("build_cold"):
                # A cached build says nothing about build cost: keep the cold timing.
                info["build_s"], info["build_cold"] = old["build_s"], True
            info["loc"] = count_loc([BENCH / "apps" / app["dir"] / f for f in app["sources"]])
            info["sources"] = [f"bench/apps/{app['dir']}/{f}" for f in app["sources"]]
            if app["dir"] == "trust":
                info["generated_loc"] = generated_loc(contexts["trust"])
            report["apps"].append(info)
            try:
                reset_db()
                info["startup_ms"] = round(start_app(app, info["image"], args.app_cpus, args.memory, args.db_pool), 1)
                check_contract()
                cg = cgroup(APP)
                time.sleep(1)
                info["idle_mem_mb"] = round(mem_bytes(cg) / 1e6, 1)
                log(f"{app['id']}: up in {info['startup_ms']} ms, {info['idle_mem_mb']} MB idle, contract ok")
                for scenario in scenarios:
                    for c in levels:
                        for attempt in range(args.noisy_retries + 1):
                            run = measure(app, scenario, c, cg, args)
                            if not run["noisy"]:
                                break
                            if attempt < args.noisy_retries:
                                log("  noisy measurement, measuring again")
                        report["runs"].append(run)
            except Exception as e:  # keep going: one broken contender shouldn't sink the run
                info["error"] = str(e)[:2000]
                log(f"{app['id']} FAILED: {e}")
            finally:
                sh("docker", "rm", "-f", APP, check=False)
            # Saved after every contender, so an interrupted run keeps what it measured.
            save(report, stamp)
    finally:
        if not args.keep:
            sh("docker", "rm", "-f", PG, check=False)

    log(f"wrote bench/results/run-{stamp}.json, latest.json and dashboard/data.js")
    log("open bench/dashboard/index.html")
    return 0


def retry_noisy(args) -> int:
    """Measure again only the points of results/latest.json flagged noisy."""
    report = json.loads((RESULTS / "latest.json").read_text())
    todo = [r for r in report["runs"] if r.get("noisy")]
    if not todo:
        log("no noisy point in results/latest.json")
        return 0
    log(f"{len(todo)} noisy points to measure again")
    stamp = dt.datetime.now().strftime("%Y%m%d-%H%M%S")
    by_id = {a["id"]: a for a in APPS}
    scen = {s["id"]: s for s in SCENARIOS}
    start_postgres(args.pg_cpus)
    try:
        for app_id in dict.fromkeys(r["app"] for r in todo):
            app, info = by_id[app_id], next(a for a in report["apps"] if a["id"] == app_id)
            if sh("docker", "image", "inspect", info["image"], check=False, capture=True).startswith("[]"):
                raise RuntimeError(f"image {info['image']} is missing: run a full bench first")
            try:
                reset_db()
                start_app(app, info["image"], args.app_cpus, args.memory, args.db_pool)
                check_contract()
                cg = cgroup(APP)
                time.sleep(1)
                log(f"{app_id}: up")
                for old in [r for r in todo if r["app"] == app_id]:
                    best = None
                    for attempt in range(args.noisy_retries + 1):
                        run = measure(app, scen[old["scenario"]], old["connections"], cg, args)
                        # Keep the least disturbed attempt if none comes out clean.
                        if best is None or noise(run) < noise(best):
                            best = run
                        if not run["noisy"]:
                            break
                    best["remeasured"] = True
                    report["runs"][report["runs"].index(old)] = best
            except Exception as e:
                log(f"{app_id} FAILED: {e}")
            finally:
                sh("docker", "rm", "-f", APP, check=False)
            save(report, stamp)
    finally:
        if not args.keep:
            sh("docker", "rm", "-f", PG, check=False)
    left = sum(1 for r in report["runs"] if r.get("noisy"))
    log(f"done: {len(todo) - left} of {len(todo)} points now clean")
    return 0


def pin_harness(args) -> None:
    """Keep this script and the docker CLIs it spawns off the bench CPUs (and
    the app's hyperthread siblings), so the harness never counts as noise."""
    used = set(cpu_list(args.app_cpus)) | set(cpu_list(args.pg_cpus)) | set(cpu_list(args.load_cpus))
    used |= set(siblings(cpu_list(args.app_cpus)))
    free = set(range(os.cpu_count() or 1)) - used
    if free:
        os.sched_setaffinity(0, free)


def noise(run: dict) -> float:
    return run.get("interference_cores", 0) + run.get("sibling_cores", 0) + run.get("rest_cores", 0) / 3


def measure(app: dict, scenario: dict, c: int, cg: Path, args) -> dict:
    busy_before = wait_quiet(args.max_busy, args.quiet_timeout) if args.max_busy else None
    if args.warmup:
        oha(scenario, c, args.warmup, args.load_cpus)
    with Sampler(cg, args) as sample:
        raw = oha(scenario, c, args.duration, args.load_cpus)
    # Divide by oha's own load time, not the sampler's wall time:
    # the latter includes the load container starting up idle.
    cores = sample.cpu_seconds / raw["summary"]["total"]
    run = {"app": app["id"], "scenario": scenario["id"], "connections": c,
           **summarize(raw), "cpu_cores": round(cores, 2),
           "mem_peak_mb": round(sample.peak / 1e6, 1)}
    run["rps_per_core"] = round(run["rps"] / cores, 1) if cores else None
    # Noise: other processes on the app's CPUs during the run, and
    # elsewhere on the host just before it.
    run["interference_cores"] = round(sample.interference_cores, 2)
    run["sibling_cores"] = round(sample.sibling_cores, 2)
    run["rest_cores"] = round(sample.rest_cores, 2)
    run["app_mhz"] = round(sample.avg_mhz) if sample.avg_mhz else None
    run["host_busy_before"] = round(busy_before, 2) if busy_before is not None else None
    run["noisy"] = (run["interference_cores"] > NOISY_INTERFERENCE
                    or run["sibling_cores"] > NOISY_SIBLING
                    or run["rest_cores"] > NOISY_REST
                    or (busy_before is not None and args.max_busy > 0 and busy_before > args.max_busy))
    log(f"  {scenario['id']:<6} c={c:<4} {run['rps']:>10,.0f} req/s  "
        f"p99 {run['latency_ms']['p99']} ms  ok {run['success_rate']:.2%}  "
        f"cpu {run['cpu_cores']}  mem {run['mem_peak_mb']} MB"
        + (f"  NOISY (others on app CPUs {run['interference_cores']}, HT siblings {run['sibling_cores']}, "
           f"postgres/load CPUs {run['rest_cores']})" if run["noisy"] else ""))
    return run


def save(report: dict, stamp: str) -> None:
    RESULTS.mkdir(exist_ok=True)
    # Canonical order whatever was (re)measured: the dashboard colours each
    # contender by its position, so it must not move between reports.
    rank = {a["id"]: i for i, a in enumerate(APPS)}
    report["apps"].sort(key=lambda a: rank.get(a["id"], len(rank)))
    report["runs"].sort(key=lambda r: rank.get(r["app"], len(rank)))
    payload = json.dumps(report, indent=2)
    (RESULTS / f"run-{stamp}.json").write_text(payload)
    (RESULTS / "latest.json").write_text(payload)
    (BENCH / "dashboard" / "data.js").write_text(f"window.BENCH_RESULTS = {payload};\n")


if __name__ == "__main__":
    sys.exit(main())
