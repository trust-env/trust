window.BENCH_RESULTS = {
  "meta": {
    "date": "2026-11-13T09:46:57+01:00",
    "trust_commit": "8101893",
    "host": {
      "cpu": "Intel(R) Core(TM) Ultra 7 155H",
      "threads": 22,
      "kernel": "7.0.0-42-generic",
      "docker": "29.1.3"
    },
    "config": {
      "duration_s": 10,
      "warmup_s": 3,
      "connections": [
        16,
        64,
        256
      ],
      "app_cpus": "1,3",
      "pg_cpus": "6,8,10",
      "load_cpus": "12-19",
      "memory": "1g",
      "db_pool": 32,
      "seed_rows": 10000,
      "postgres": "postgres:17-alpine",
      "load_generator": "ghcr.io/hatoo/oha:1.16.0"
    },
    "scenarios": [
      {
        "id": "ping",
        "label": "GET /ping",
        "desc": "Framework overhead: routing + JSON, no I/O"
      },
      {
        "id": "get",
        "label": "GET /users/:id",
        "desc": "Primary-key read, random id among 10k rows"
      },
      {
        "id": "list",
        "label": "GET /users?limit=20",
        "desc": "Paged read of 20 rows at a random offset"
      },
      {
        "id": "create",
        "label": "POST /users",
        "desc": "JSON body validation + INSERT \u2026 RETURNING"
      }
    ]
  },
  "apps": [
    {
      "id": "trust",
      "name": "Trust",
      "family": "Rust (generated)",
      "image": "trust-bench/trust",
      "build_s": 124.7,
      "build_cold": true,
      "image_mb": 30.7,
      "loc": 32,
      "sources": [
        "bench/apps/trust/app.tr",
        "bench/apps/trust/ping.rs"
      ],
      "generated_loc": 1101,
      "startup_ms": 235.3,
      "idle_mem_mb": 2.1
    },
    {
      "id": "rust-axum",
      "name": "Rust \u00b7 axum",
      "family": "Rust",
      "image": "trust-bench/rust-axum",
      "build_s": 92.5,
      "build_cold": true,
      "image_mb": 29.3,
      "loc": 84,
      "sources": [
        "bench/apps/rust-axum/src/main.rs"
      ],
      "startup_ms": 224.4,
      "idle_mem_mb": 3.3
    },
    {
      "id": "go",
      "name": "Go \u00b7 net/http",
      "family": "Go",
      "image": "trust-bench/go",
      "build_s": 26.1,
      "build_cold": true,
      "image_mb": 7.1,
      "loc": 111,
      "sources": [
        "bench/apps/go/main.go"
      ],
      "startup_ms": 229.2,
      "idle_mem_mb": 11.3
    },
    {
      "id": "fastify",
      "name": "Node \u00b7 Fastify",
      "family": "Node.js",
      "image": "trust-bench/fastify",
      "build_s": 11.2,
      "build_cold": true,
      "image_mb": 67.5,
      "loc": 45,
      "sources": [
        "bench/apps/fastify/server.js"
      ],
      "startup_ms": 466.6,
      "idle_mem_mb": 65.0
    },
    {
      "id": "nestjs-express",
      "name": "NestJS \u00b7 Express",
      "family": "Node.js",
      "image": "trust-bench/nestjs",
      "build_s": 31.5,
      "build_cold": true,
      "image_mb": 65.5,
      "loc": 69,
      "sources": [
        "bench/apps/nestjs/src/main.ts"
      ],
      "startup_ms": 1333.9,
      "idle_mem_mb": 103.1
    },
    {
      "id": "nestjs-fastify",
      "name": "NestJS \u00b7 Fastify",
      "family": "Node.js",
      "image": "trust-bench/nestjs",
      "build_s": 31.5,
      "build_cold": true,
      "image_mb": 65.5,
      "loc": 69,
      "sources": [
        "bench/apps/nestjs/src/main.ts"
      ],
      "startup_ms": 2867.3,
      "idle_mem_mb": 147.5
    },
    {
      "id": "fastapi",
      "name": "Python \u00b7 FastAPI",
      "family": "Python",
      "image": "trust-bench/fastapi",
      "build_s": 11.0,
      "build_cold": true,
      "image_mb": 62.9,
      "loc": 40,
      "sources": [
        "bench/apps/fastapi/main.py"
      ],
      "startup_ms": 3725.2,
      "idle_mem_mb": 130.3
    }
  ],
  "runs": [
    {
      "app": "trust",
      "scenario": "ping",
      "connections": 16,
      "rps": 276709.7,
      "requests": 2768330,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.056,
        "p50": 0.055,
        "p90": 0.073,
        "p99": 0.086,
        "p999": 0.103,
        "max": 22.086
      },
      "status_codes": {
        "200": 2768330
      },
      "errors": {},
      "cpu_cores": 1.98,
      "mem_peak_mb": 4.4,
      "rps_per_core": 139411.9
    },
    {
      "app": "trust",
      "scenario": "ping",
      "connections": 64,
      "rps": 279326.0,
      "requests": 2796752,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.227,
        "p50": 0.227,
        "p90": 0.309,
        "p99": 0.346,
        "p999": 0.417,
        "max": 16.742
      },
      "status_codes": {
        "200": 2796752
      },
      "errors": {},
      "cpu_cores": 1.96,
      "mem_peak_mb": 5.9,
      "rps_per_core": 142365.9
    },
    {
      "app": "trust",
      "scenario": "ping",
      "connections": 256,
      "rps": 282281.8,
      "requests": 2824463,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.902,
        "p50": 0.908,
        "p90": 1.248,
        "p99": 1.393,
        "p999": 1.618,
        "max": 45.224
      },
      "status_codes": {
        "200": 2824463
      },
      "errors": {},
      "cpu_cores": 1.99,
      "mem_peak_mb": 13.4,
      "rps_per_core": 141611.9
    },
    {
      "app": "trust",
      "scenario": "get",
      "connections": 16,
      "rps": 55170.9,
      "requests": 551935,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.287,
        "p50": 0.281,
        "p90": 0.353,
        "p99": 0.453,
        "p999": 0.94,
        "max": 56.568
      },
      "status_codes": {
        "200": 551935
      },
      "errors": {},
      "cpu_cores": 1.94,
      "mem_peak_mb": 12.7,
      "rps_per_core": 28464.4
    },
    {
      "app": "trust",
      "scenario": "get",
      "connections": 64,
      "rps": 53842.3,
      "requests": 538553,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 1.183,
        "p50": 1.17,
        "p90": 1.32,
        "p99": 1.871,
        "p999": 2.126,
        "max": 107.736
      },
      "status_codes": {
        "200": 538553
      },
      "errors": {},
      "cpu_cores": 1.95,
      "mem_peak_mb": 13.9,
      "rps_per_core": 27675.3
    },
    {
      "app": "trust",
      "scenario": "get",
      "connections": 256,
      "rps": 46946.4,
      "requests": 469556,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 5.43,
        "p50": 5.229,
        "p90": 6.087,
        "p99": 8.831,
        "p999": 10.606,
        "max": 139.792
      },
      "status_codes": {
        "200": 469556
      },
      "errors": {},
      "cpu_cores": 1.95,
      "mem_peak_mb": 23.7,
      "rps_per_core": 24124.3
    },
    {
      "app": "trust",
      "scenario": "list",
      "connections": 16,
      "rps": 30146.8,
      "requests": 301576,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.527,
        "p50": 0.499,
        "p90": 0.74,
        "p99": 1.086,
        "p999": 2.996,
        "max": 98.458
      },
      "status_codes": {
        "200": 301576
      },
      "errors": {},
      "cpu_cores": 1.83,
      "mem_peak_mb": 22.8,
      "rps_per_core": 16502.6
    },
    {
      "app": "trust",
      "scenario": "list",
      "connections": 64,
      "rps": 29937.9,
      "requests": 299376,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 2.13,
        "p50": 2.045,
        "p90": 2.601,
        "p99": 3.559,
        "p999": 6.077,
        "max": 118.037
      },
      "status_codes": {
        "200": 299376
      },
      "errors": {},
      "cpu_cores": 1.87,
      "mem_peak_mb": 22.8,
      "rps_per_core": 15969.7
    },
    {
      "app": "trust",
      "scenario": "list",
      "connections": 256,
      "rps": 28882.9,
      "requests": 288682,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 8.828,
        "p50": 8.588,
        "p90": 9.774,
        "p99": 13.824,
        "p999": 17.104,
        "max": 97.617
      },
      "status_codes": {
        "200": 288682
      },
      "errors": {},
      "cpu_cores": 1.92,
      "mem_peak_mb": 24.5,
      "rps_per_core": 15035.5
    },
    {
      "app": "trust",
      "scenario": "create",
      "connections": 16,
      "rps": 7130.2,
      "requests": 71298,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 2.238,
        "p50": 2.057,
        "p90": 3.334,
        "p99": 4.041,
        "p999": 7.899,
        "max": 46.467
      },
      "status_codes": {
        "200": 71298
      },
      "errors": {},
      "cpu_cores": 0.36,
      "mem_peak_mb": 23.3,
      "rps_per_core": 19779.0
    },
    {
      "app": "trust",
      "scenario": "create",
      "connections": 64,
      "rps": 13950.6,
      "requests": 139476,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 4.578,
        "p50": 4.227,
        "p90": 5.643,
        "p99": 6.908,
        "p999": 24.238,
        "max": 50.939
      },
      "status_codes": {
        "200": 139476
      },
      "errors": {},
      "cpu_cores": 0.62,
      "mem_peak_mb": 23.7,
      "rps_per_core": 22672.1
    },
    {
      "app": "trust",
      "scenario": "create",
      "connections": 256,
      "rps": 13952.6,
      "requests": 139318,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 18.277,
        "p50": 17.924,
        "p90": 18.764,
        "p99": 24.001,
        "p999": 68.905,
        "max": 78.251
      },
      "status_codes": {
        "200": 139318
      },
      "errors": {},
      "cpu_cores": 0.63,
      "mem_peak_mb": 23.2,
      "rps_per_core": 22320.7
    },
    {
      "app": "rust-axum",
      "scenario": "ping",
      "connections": 16,
      "rps": 261062.7,
      "requests": 2612141,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.059,
        "p50": 0.058,
        "p90": 0.078,
        "p99": 0.1,
        "p999": 0.128,
        "max": 24.574
      },
      "status_codes": {
        "200": 2612141
      },
      "errors": {},
      "cpu_cores": 1.98,
      "mem_peak_mb": 5.9,
      "rps_per_core": 131831.9
    },
    {
      "app": "rust-axum",
      "scenario": "ping",
      "connections": 64,
      "rps": 260930.2,
      "requests": 2611991,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.243,
        "p50": 0.242,
        "p90": 0.328,
        "p99": 0.421,
        "p999": 0.631,
        "max": 18.065
      },
      "status_codes": {
        "200": 2611991
      },
      "errors": {},
      "cpu_cores": 1.96,
      "mem_peak_mb": 7.2,
      "rps_per_core": 133157.2
    },
    {
      "app": "rust-axum",
      "scenario": "ping",
      "connections": 256,
      "rps": 272564.9,
      "requests": 2727583,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.934,
        "p50": 0.935,
        "p90": 1.284,
        "p99": 1.566,
        "p999": 2.638,
        "max": 51.233
      },
      "status_codes": {
        "200": 2727583
      },
      "errors": {},
      "cpu_cores": 1.99,
      "mem_peak_mb": 14.4,
      "rps_per_core": 137088.9
    },
    {
      "app": "rust-axum",
      "scenario": "get",
      "connections": 16,
      "rps": 50846.1,
      "requests": 508618,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.311,
        "p50": 0.3,
        "p90": 0.398,
        "p99": 0.536,
        "p999": 1.493,
        "max": 17.372
      },
      "status_codes": {
        "200": 508618
      },
      "errors": {},
      "cpu_cores": 1.92,
      "mem_peak_mb": 14.2,
      "rps_per_core": 26530.2
    },
    {
      "app": "rust-axum",
      "scenario": "get",
      "connections": 64,
      "rps": 51612.4,
      "requests": 516233,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 1.234,
        "p50": 1.204,
        "p90": 1.425,
        "p99": 1.942,
        "p999": 2.567,
        "max": 70.295
      },
      "status_codes": {
        "200": 516233
      },
      "errors": {},
      "cpu_cores": 1.95,
      "mem_peak_mb": 16.9,
      "rps_per_core": 26451.5
    },
    {
      "app": "rust-axum",
      "scenario": "get",
      "connections": 256,
      "rps": 48222.0,
      "requests": 482174,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 5.288,
        "p50": 5.041,
        "p90": 6.021,
        "p99": 8.947,
        "p999": 11.036,
        "max": 125.216
      },
      "status_codes": {
        "200": 482174
      },
      "errors": {},
      "cpu_cores": 1.91,
      "mem_peak_mb": 20.5,
      "rps_per_core": 25304.3
    },
    {
      "app": "rust-axum",
      "scenario": "list",
      "connections": 16,
      "rps": 27342.6,
      "requests": 273531,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.581,
        "p50": 0.541,
        "p90": 0.819,
        "p99": 1.398,
        "p999": 3.799,
        "max": 25.134
      },
      "status_codes": {
        "200": 273531
      },
      "errors": {},
      "cpu_cores": 1.68,
      "mem_peak_mb": 19.7,
      "rps_per_core": 16264.0
    },
    {
      "app": "rust-axum",
      "scenario": "list",
      "connections": 64,
      "rps": 29904.5,
      "requests": 299064,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 2.134,
        "p50": 2.059,
        "p90": 2.646,
        "p99": 3.454,
        "p999": 6.468,
        "max": 17.022
      },
      "status_codes": {
        "200": 299064
      },
      "errors": {},
      "cpu_cores": 1.84,
      "mem_peak_mb": 19.2,
      "rps_per_core": 16247.6
    },
    {
      "app": "rust-axum",
      "scenario": "list",
      "connections": 256,
      "rps": 29054.1,
      "requests": 290426,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 8.781,
        "p50": 8.515,
        "p90": 9.739,
        "p99": 13.962,
        "p999": 17.983,
        "max": 73.728
      },
      "status_codes": {
        "200": 290426
      },
      "errors": {},
      "cpu_cores": 1.87,
      "mem_peak_mb": 23.8,
      "rps_per_core": 15556.9
    },
    {
      "app": "rust-axum",
      "scenario": "create",
      "connections": 16,
      "rps": 7127.8,
      "requests": 71275,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 2.239,
        "p50": 2.066,
        "p90": 3.331,
        "p99": 4.089,
        "p999": 7.996,
        "max": 16.427
      },
      "status_codes": {
        "200": 71275
      },
      "errors": {},
      "cpu_cores": 0.36,
      "mem_peak_mb": 20.9,
      "rps_per_core": 19743.2
    },
    {
      "app": "rust-axum",
      "scenario": "create",
      "connections": 64,
      "rps": 14053.0,
      "requests": 140495,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 4.545,
        "p50": 4.238,
        "p90": 5.64,
        "p99": 6.627,
        "p999": 11.745,
        "max": 23.844
      },
      "status_codes": {
        "200": 140495
      },
      "errors": {},
      "cpu_cores": 0.57,
      "mem_peak_mb": 22.3,
      "rps_per_core": 24757.6
    },
    {
      "app": "rust-axum",
      "scenario": "create",
      "connections": 256,
      "rps": 13419.9,
      "requests": 133999,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 19.016,
        "p50": 18.159,
        "p90": 21.234,
        "p99": 31.197,
        "p999": 43.414,
        "max": 73.485
      },
      "status_codes": {
        "200": 133999
      },
      "errors": {},
      "cpu_cores": 0.67,
      "mem_peak_mb": 22.6,
      "rps_per_core": 20083.8
    },
    {
      "app": "go",
      "scenario": "ping",
      "connections": 16,
      "rps": 55799.5,
      "requests": 558673,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.283,
        "p50": 0.176,
        "p90": 0.499,
        "p99": 3.051,
        "p999": 4.896,
        "max": 11.07
      },
      "status_codes": {
        "200": 558673
      },
      "errors": {},
      "cpu_cores": 1.39,
      "mem_peak_mb": 18.9,
      "rps_per_core": 40030.2
    },
    {
      "app": "go",
      "scenario": "ping",
      "connections": 64,
      "rps": 134009.0,
      "requests": 1345141,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.473,
        "p50": 0.378,
        "p90": 0.927,
        "p99": 1.841,
        "p999": 4.811,
        "max": 41.658
      },
      "status_codes": {
        "200": 1345141
      },
      "errors": {},
      "cpu_cores": 1.87,
      "mem_peak_mb": 18.9,
      "rps_per_core": 71492.0
    },
    {
      "app": "go",
      "scenario": "ping",
      "connections": 256,
      "rps": 102813.7,
      "requests": 1028282,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 2.481,
        "p50": 2.141,
        "p90": 4.581,
        "p99": 9.196,
        "p999": 13.79,
        "max": 55.19
      },
      "status_codes": {
        "200": 1028282
      },
      "errors": {},
      "cpu_cores": 1.73,
      "mem_peak_mb": 24.5,
      "rps_per_core": 59322.4
    },
    {
      "app": "go",
      "scenario": "get",
      "connections": 16,
      "rps": 43800.9,
      "requests": 438126,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.361,
        "p50": 0.254,
        "p90": 0.546,
        "p99": 3.338,
        "p999": 6.339,
        "max": 17.195
      },
      "status_codes": {
        "200": 438126
      },
      "errors": {},
      "cpu_cores": 1.47,
      "mem_peak_mb": 20.5,
      "rps_per_core": 29809.8
    },
    {
      "app": "go",
      "scenario": "get",
      "connections": 64,
      "rps": 36674.1,
      "requests": 366780,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 1.737,
        "p50": 1.372,
        "p90": 3.082,
        "p99": 6.64,
        "p999": 10.757,
        "max": 48.467
      },
      "status_codes": {
        "200": 366780
      },
      "errors": {},
      "cpu_cores": 1.59,
      "mem_peak_mb": 24.6,
      "rps_per_core": 23025.3
    },
    {
      "app": "go",
      "scenario": "get",
      "connections": 256,
      "rps": 24082.6,
      "requests": 240848,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 10.59,
        "p50": 8.138,
        "p90": 18.235,
        "p99": 41.409,
        "p999": 134.419,
        "max": 376.25
      },
      "status_codes": {
        "200": 240848
      },
      "errors": {},
      "cpu_cores": 1.45,
      "mem_peak_mb": 34.4,
      "rps_per_core": 16665.8
    },
    {
      "app": "go",
      "scenario": "list",
      "connections": 16,
      "rps": 8966.6,
      "requests": 92310,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 1.723,
        "p50": 1.217,
        "p90": 3.518,
        "p99": 8.088,
        "p999": 15.08,
        "max": 31.299
      },
      "status_codes": {
        "200": 92310
      },
      "errors": {},
      "cpu_cores": 1.08,
      "mem_peak_mb": 26.7,
      "rps_per_core": 8296.1
    },
    {
      "app": "go",
      "scenario": "list",
      "connections": 64,
      "rps": 18553.6,
      "requests": 185541,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 3.437,
        "p50": 2.696,
        "p90": 6.297,
        "p99": 11.305,
        "p999": 17.11,
        "max": 63.128
      },
      "status_codes": {
        "200": 185541
      },
      "errors": {},
      "cpu_cores": 1.15,
      "mem_peak_mb": 29.1,
      "rps_per_core": 16106.4
    },
    {
      "app": "go",
      "scenario": "list",
      "connections": 256,
      "rps": 22736.2,
      "requests": 227210,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 11.224,
        "p50": 10.673,
        "p90": 14.734,
        "p99": 21.04,
        "p999": 48.505,
        "max": 96.065
      },
      "status_codes": {
        "200": 227210
      },
      "errors": {},
      "cpu_cores": 1.27,
      "mem_peak_mb": 37.2,
      "rps_per_core": 17952.8
    },
    {
      "app": "go",
      "scenario": "create",
      "connections": 16,
      "rps": 4868.4,
      "requests": 48682,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 3.279,
        "p50": 2.244,
        "p90": 3.856,
        "p99": 37.276,
        "p999": 143.173,
        "max": 222.269
      },
      "status_codes": {
        "200": 48682
      },
      "errors": {},
      "cpu_cores": 0.3,
      "mem_peak_mb": 22.4,
      "rps_per_core": 16089.4
    },
    {
      "app": "go",
      "scenario": "create",
      "connections": 64,
      "rps": 5672.3,
      "requests": 56677,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 11.229,
        "p50": 8.114,
        "p90": 11.496,
        "p99": 74.539,
        "p999": 320.708,
        "max": 357.989
      },
      "status_codes": {
        "200": 56677
      },
      "errors": {},
      "cpu_cores": 0.29,
      "mem_peak_mb": 27.7,
      "rps_per_core": 19402.4
    },
    {
      "app": "go",
      "scenario": "create",
      "connections": 256,
      "rps": 10581.9,
      "requests": 105616,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 24.14,
        "p50": 18.459,
        "p90": 52.918,
        "p99": 74.222,
        "p999": 288.551,
        "max": 326.925
      },
      "status_codes": {
        "200": 105616
      },
      "errors": {},
      "cpu_cores": 0.5,
      "mem_peak_mb": 37.2,
      "rps_per_core": 21092.2
    },
    {
      "app": "fastify",
      "scenario": "ping",
      "connections": 16,
      "rps": 80637.7,
      "requests": 806769,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.196,
        "p50": 0.167,
        "p90": 0.327,
        "p99": 0.558,
        "p999": 1.082,
        "max": 18.815
      },
      "status_codes": {
        "200": 806769
      },
      "errors": {},
      "cpu_cores": 1.99,
      "mem_peak_mb": 71.0,
      "rps_per_core": 40437.7
    },
    {
      "app": "fastify",
      "scenario": "ping",
      "connections": 64,
      "rps": 89268.0,
      "requests": 892862,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.714,
        "p50": 0.654,
        "p90": 0.925,
        "p99": 1.47,
        "p999": 2.823,
        "max": 26.021
      },
      "status_codes": {
        "200": 892862
      },
      "errors": {},
      "cpu_cores": 2.0,
      "mem_peak_mb": 71.2,
      "rps_per_core": 44709.1
    },
    {
      "app": "fastify",
      "scenario": "ping",
      "connections": 256,
      "rps": 68057.7,
      "requests": 680829,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 3.746,
        "p50": 3.44,
        "p90": 5.34,
        "p99": 7.204,
        "p999": 9.479,
        "max": 280.176
      },
      "status_codes": {
        "200": 680829
      },
      "errors": {},
      "cpu_cores": 1.99,
      "mem_peak_mb": 74.6,
      "rps_per_core": 34152.9
    },
    {
      "app": "fastify",
      "scenario": "get",
      "connections": 16,
      "rps": 24213.3,
      "requests": 242174,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.656,
        "p50": 0.627,
        "p90": 0.842,
        "p99": 1.432,
        "p999": 4.196,
        "max": 45.016
      },
      "status_codes": {
        "200": 242174
      },
      "errors": {},
      "cpu_cores": 1.97,
      "mem_peak_mb": 79.9,
      "rps_per_core": 12308.5
    },
    {
      "app": "fastify",
      "scenario": "get",
      "connections": 64,
      "rps": 28022.4,
      "requests": 280235,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 2.276,
        "p50": 2.187,
        "p90": 2.994,
        "p99": 3.82,
        "p999": 5.682,
        "max": 35.946
      },
      "status_codes": {
        "200": 280235
      },
      "errors": {},
      "cpu_cores": 2.0,
      "mem_peak_mb": 79.3,
      "rps_per_core": 14041.5
    },
    {
      "app": "fastify",
      "scenario": "get",
      "connections": 256,
      "rps": 21694.2,
      "requests": 216975,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 11.75,
        "p50": 11.175,
        "p90": 14.232,
        "p99": 23.783,
        "p999": 31.452,
        "max": 193.067
      },
      "status_codes": {
        "200": 216975
      },
      "errors": {},
      "cpu_cores": 1.99,
      "mem_peak_mb": 83.7,
      "rps_per_core": 10896.7
    },
    {
      "app": "fastify",
      "scenario": "list",
      "connections": 16,
      "rps": 13874.5,
      "requests": 138773,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 1.147,
        "p50": 1.003,
        "p90": 1.84,
        "p99": 3.76,
        "p999": 5.871,
        "max": 24.662
      },
      "status_codes": {
        "200": 138773
      },
      "errors": {},
      "cpu_cores": 1.95,
      "mem_peak_mb": 84.4,
      "rps_per_core": 7106.0
    },
    {
      "app": "fastify",
      "scenario": "list",
      "connections": 64,
      "rps": 14731.8,
      "requests": 148841,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 4.288,
        "p50": 4.04,
        "p90": 5.827,
        "p99": 8.844,
        "p999": 12.559,
        "max": 46.437
      },
      "status_codes": {
        "200": 148841
      },
      "errors": {},
      "cpu_cores": 1.97,
      "mem_peak_mb": 81.0,
      "rps_per_core": 7461.8
    },
    {
      "app": "fastify",
      "scenario": "list",
      "connections": 256,
      "rps": 13270.5,
      "requests": 132494,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 19.214,
        "p50": 18.804,
        "p90": 22.832,
        "p99": 38.167,
        "p999": 64.408,
        "max": 224.984
      },
      "status_codes": {
        "200": 132494
      },
      "errors": {},
      "cpu_cores": 1.99,
      "mem_peak_mb": 94.1,
      "rps_per_core": 6659.6
    },
    {
      "app": "fastify",
      "scenario": "create",
      "connections": 16,
      "rps": 5470.0,
      "requests": 54931,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 2.9,
        "p50": 2.141,
        "p90": 3.492,
        "p99": 37.368,
        "p999": 68.52,
        "max": 109.74
      },
      "status_codes": {
        "200": 54931
      },
      "errors": {},
      "cpu_cores": 0.6,
      "mem_peak_mb": 93.5,
      "rps_per_core": 9082.1
    },
    {
      "app": "fastify",
      "scenario": "create",
      "connections": 64,
      "rps": 8983.5,
      "requests": 89806,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 7.105,
        "p50": 5.285,
        "p90": 8.197,
        "p99": 42.83,
        "p999": 73.632,
        "max": 80.358
      },
      "status_codes": {
        "200": 89806
      },
      "errors": {},
      "cpu_cores": 0.81,
      "mem_peak_mb": 92.8,
      "rps_per_core": 11141.3
    },
    {
      "app": "fastify",
      "scenario": "create",
      "connections": 256,
      "rps": 10172.2,
      "requests": 101907,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 24.888,
        "p50": 23.304,
        "p90": 29.061,
        "p99": 60.375,
        "p999": 85.436,
        "max": 100.433
      },
      "status_codes": {
        "200": 101907
      },
      "errors": {},
      "cpu_cores": 1.13,
      "mem_peak_mb": 93.2,
      "rps_per_core": 9004.5
    },
    {
      "app": "nestjs-express",
      "scenario": "ping",
      "connections": 16,
      "rps": 10101.3,
      "requests": 104860,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 1.528,
        "p50": 0.709,
        "p90": 1.859,
        "p99": 8.034,
        "p999": 92.361,
        "max": 1080.242
      },
      "status_codes": {
        "200": 104860
      },
      "errors": {},
      "cpu_cores": 1.42,
      "mem_peak_mb": 130.8,
      "rps_per_core": 7119.3
    },
    {
      "app": "nestjs-express",
      "scenario": "ping",
      "connections": 64,
      "rps": 15428.8,
      "requests": 154340,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 4.136,
        "p50": 3.246,
        "p90": 6.753,
        "p99": 16.289,
        "p999": 31.583,
        "max": 103.838
      },
      "status_codes": {
        "200": 154340
      },
      "errors": {},
      "cpu_cores": 1.93,
      "mem_peak_mb": 133.5,
      "rps_per_core": 7977.1
    },
    {
      "app": "nestjs-express",
      "scenario": "ping",
      "connections": 256,
      "rps": 16220.9,
      "requests": 162013,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 15.745,
        "p50": 12.986,
        "p90": 24.071,
        "p99": 40.627,
        "p999": 92.493,
        "max": 1056.842
      },
      "status_codes": {
        "200": 162013
      },
      "errors": {},
      "cpu_cores": 1.97,
      "mem_peak_mb": 144.0,
      "rps_per_core": 8235.8
    },
    {
      "app": "nestjs-express",
      "scenario": "get",
      "connections": 16,
      "rps": 8278.9,
      "requests": 82788,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 1.925,
        "p50": 1.62,
        "p90": 2.708,
        "p99": 8.207,
        "p999": 19.353,
        "max": 38.016
      },
      "status_codes": {
        "200": 82788
      },
      "errors": {},
      "cpu_cores": 1.99,
      "mem_peak_mb": 147.4,
      "rps_per_core": 4165.3
    },
    {
      "app": "nestjs-express",
      "scenario": "get",
      "connections": 64,
      "rps": 9816.8,
      "requests": 98130,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 6.505,
        "p50": 5.747,
        "p90": 8.757,
        "p99": 21.25,
        "p999": 33.806,
        "max": 84.541
      },
      "status_codes": {
        "200": 98130
      },
      "errors": {},
      "cpu_cores": 1.98,
      "mem_peak_mb": 148.0,
      "rps_per_core": 4946.1
    },
    {
      "app": "nestjs-express",
      "scenario": "get",
      "connections": 256,
      "rps": 6908.9,
      "requests": 68867,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 36.95,
        "p50": 34.132,
        "p90": 61.494,
        "p99": 97.412,
        "p999": 350.989,
        "max": 641.991
      },
      "status_codes": {
        "200": 68867
      },
      "errors": {},
      "cpu_cores": 1.95,
      "mem_peak_mb": 146.1,
      "rps_per_core": 3543.7
    },
    {
      "app": "nestjs-express",
      "scenario": "list",
      "connections": 16,
      "rps": 6714.0,
      "requests": 67145,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 2.375,
        "p50": 2.062,
        "p90": 3.263,
        "p99": 9.518,
        "p999": 20.487,
        "max": 40.181
      },
      "status_codes": {
        "200": 67145
      },
      "errors": {},
      "cpu_cores": 1.99,
      "mem_peak_mb": 146.5,
      "rps_per_core": 3377.2
    },
    {
      "app": "nestjs-express",
      "scenario": "list",
      "connections": 64,
      "rps": 6356.9,
      "requests": 63529,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 10.044,
        "p50": 8.958,
        "p90": 14.092,
        "p99": 29.921,
        "p999": 41.565,
        "max": 69.0
      },
      "status_codes": {
        "200": 63529
      },
      "errors": {},
      "cpu_cores": 1.99,
      "mem_peak_mb": 146.0,
      "rps_per_core": 3191.6
    },
    {
      "app": "nestjs-express",
      "scenario": "list",
      "connections": 256,
      "rps": 3662.6,
      "requests": 36394,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 69.858,
        "p50": 63.629,
        "p90": 102.554,
        "p99": 149.559,
        "p999": 835.398,
        "max": 1131.662
      },
      "status_codes": {
        "200": 36394
      },
      "errors": {},
      "cpu_cores": 2.0,
      "mem_peak_mb": 140.8,
      "rps_per_core": 1835.1
    },
    {
      "app": "nestjs-express",
      "scenario": "create",
      "connections": 16,
      "rps": 2863.6,
      "requests": 28630,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 5.579,
        "p50": 3.853,
        "p90": 7.367,
        "p99": 39.794,
        "p999": 106.859,
        "max": 172.566
      },
      "status_codes": {
        "201": 28630
      },
      "errors": {},
      "cpu_cores": 1.15,
      "mem_peak_mb": 140.2,
      "rps_per_core": 2499.8
    },
    {
      "app": "nestjs-express",
      "scenario": "create",
      "connections": 64,
      "rps": 5491.6,
      "requests": 55089,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 11.532,
        "p50": 9.294,
        "p90": 18.265,
        "p99": 44.079,
        "p999": 58.144,
        "max": 85.947
      },
      "status_codes": {
        "201": 55089
      },
      "errors": {},
      "cpu_cores": 1.75,
      "mem_peak_mb": 132.8,
      "rps_per_core": 3142.1
    },
    {
      "app": "nestjs-express",
      "scenario": "create",
      "connections": 256,
      "rps": 2621.6,
      "requests": 26043,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 97.728,
        "p50": 75.656,
        "p90": 167.382,
        "p99": 651.136,
        "p999": 854.332,
        "max": 1005.585
      },
      "status_codes": {
        "201": 26043
      },
      "errors": {},
      "cpu_cores": 1.42,
      "mem_peak_mb": 131.9,
      "rps_per_core": 1843.9
    },
    {
      "app": "nestjs-fastify",
      "scenario": "ping",
      "connections": 16,
      "rps": 44478.5,
      "requests": 445002,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.356,
        "p50": 0.293,
        "p90": 0.586,
        "p99": 1.123,
        "p999": 3.09,
        "max": 23.744
      },
      "status_codes": {
        "200": 445002
      },
      "errors": {},
      "cpu_cores": 1.99,
      "mem_peak_mb": 152.3,
      "rps_per_core": 22342.0
    },
    {
      "app": "nestjs-fastify",
      "scenario": "ping",
      "connections": 64,
      "rps": 70739.8,
      "requests": 707708,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.9,
        "p50": 0.773,
        "p90": 1.258,
        "p99": 2.044,
        "p999": 2.974,
        "max": 42.887
      },
      "status_codes": {
        "200": 707708
      },
      "errors": {},
      "cpu_cores": 1.99,
      "mem_peak_mb": 145.4,
      "rps_per_core": 35481.4
    },
    {
      "app": "nestjs-fastify",
      "scenario": "ping",
      "connections": 256,
      "rps": 79706.6,
      "requests": 797124,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 3.201,
        "p50": 3.074,
        "p90": 3.72,
        "p99": 5.494,
        "p999": 7.928,
        "max": 237.657
      },
      "status_codes": {
        "200": 797124
      },
      "errors": {},
      "cpu_cores": 2.0,
      "mem_peak_mb": 149.3,
      "rps_per_core": 39921.4
    },
    {
      "app": "nestjs-fastify",
      "scenario": "get",
      "connections": 16,
      "rps": 19051.5,
      "requests": 190583,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.834,
        "p50": 0.785,
        "p90": 1.196,
        "p99": 1.858,
        "p999": 3.379,
        "max": 16.683
      },
      "status_codes": {
        "200": 190583
      },
      "errors": {},
      "cpu_cores": 1.99,
      "mem_peak_mb": 155.1,
      "rps_per_core": 9550.8
    },
    {
      "app": "nestjs-fastify",
      "scenario": "get",
      "connections": 64,
      "rps": 17859.3,
      "requests": 178596,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 3.574,
        "p50": 2.992,
        "p90": 5.439,
        "p99": 12.03,
        "p999": 24.588,
        "max": 171.884
      },
      "status_codes": {
        "200": 178596
      },
      "errors": {},
      "cpu_cores": 2.0,
      "mem_peak_mb": 139.3,
      "rps_per_core": 8949.9
    },
    {
      "app": "nestjs-fastify",
      "scenario": "get",
      "connections": 256,
      "rps": 17095.2,
      "requests": 170753,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 14.931,
        "p50": 14.094,
        "p90": 17.627,
        "p99": 24.99,
        "p999": 64.614,
        "max": 296.294
      },
      "status_codes": {
        "200": 170753
      },
      "errors": {},
      "cpu_cores": 2.0,
      "mem_peak_mb": 143.5,
      "rps_per_core": 8561.9
    },
    {
      "app": "nestjs-fastify",
      "scenario": "list",
      "connections": 16,
      "rps": 14473.9,
      "requests": 144767,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 1.1,
        "p50": 1.008,
        "p90": 1.561,
        "p99": 3.62,
        "p999": 5.528,
        "max": 20.455
      },
      "status_codes": {
        "200": 144767
      },
      "errors": {},
      "cpu_cores": 1.95,
      "mem_peak_mb": 143.8,
      "rps_per_core": 7430.1
    },
    {
      "app": "nestjs-fastify",
      "scenario": "list",
      "connections": 64,
      "rps": 10472.5,
      "requests": 104697,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 6.096,
        "p50": 5.725,
        "p90": 9.056,
        "p99": 13.524,
        "p999": 17.82,
        "max": 43.181
      },
      "status_codes": {
        "200": 104697
      },
      "errors": {},
      "cpu_cores": 2.0,
      "mem_peak_mb": 140.9,
      "rps_per_core": 5248.1
    },
    {
      "app": "nestjs-fastify",
      "scenario": "list",
      "connections": 256,
      "rps": 9858.7,
      "requests": 98419,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 25.813,
        "p50": 17.649,
        "p90": 54.355,
        "p99": 76.629,
        "p999": 239.627,
        "max": 358.677
      },
      "status_codes": {
        "200": 98419
      },
      "errors": {},
      "cpu_cores": 1.98,
      "mem_peak_mb": 135.1,
      "rps_per_core": 4990.7
    },
    {
      "app": "nestjs-fastify",
      "scenario": "create",
      "connections": 16,
      "rps": 1007.5,
      "requests": 10097,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 15.842,
        "p50": 8.006,
        "p90": 13.965,
        "p99": 304.285,
        "p999": 605.002,
        "max": 722.424
      },
      "status_codes": {
        "201": 10097
      },
      "errors": {},
      "cpu_cores": 0.63,
      "mem_peak_mb": 129.7,
      "rps_per_core": 1601.6
    },
    {
      "app": "nestjs-fastify",
      "scenario": "create",
      "connections": 64,
      "rps": 2721.9,
      "requests": 27172,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 23.482,
        "p50": 20.144,
        "p90": 36.993,
        "p99": 45.953,
        "p999": 114.885,
        "max": 259.82
      },
      "status_codes": {
        "201": 27172
      },
      "errors": {},
      "cpu_cores": 1.3,
      "mem_peak_mb": 126.4,
      "rps_per_core": 2100.9
    },
    {
      "app": "nestjs-fastify",
      "scenario": "create",
      "connections": 256,
      "rps": 2013.7,
      "requests": 19901,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 127.157,
        "p50": 116.402,
        "p90": 173.151,
        "p99": 792.958,
        "p999": 1040.97,
        "max": 1118.972
      },
      "status_codes": {
        "201": 19901
      },
      "errors": {},
      "cpu_cores": 1.06,
      "mem_peak_mb": 127.4,
      "rps_per_core": 1892.9
    },
    {
      "app": "fastapi",
      "scenario": "ping",
      "connections": 16,
      "rps": 6925.0,
      "requests": 69302,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 2.297,
        "p50": 2.014,
        "p90": 3.702,
        "p99": 4.527,
        "p999": 6.043,
        "max": 19.408
      },
      "status_codes": {
        "200": 69302
      },
      "errors": {},
      "cpu_cores": 1.99,
      "mem_peak_mb": 131.5,
      "rps_per_core": 3479.1
    },
    {
      "app": "fastapi",
      "scenario": "ping",
      "connections": 64,
      "rps": 7423.5,
      "requests": 74214,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 8.591,
        "p50": 8.117,
        "p90": 10.14,
        "p99": 17.983,
        "p999": 21.911,
        "max": 145.094
      },
      "status_codes": {
        "200": 74214
      },
      "errors": {},
      "cpu_cores": 1.99,
      "mem_peak_mb": 123.0,
      "rps_per_core": 3733.7
    },
    {
      "app": "fastapi",
      "scenario": "ping",
      "connections": 256,
      "rps": 9181.5,
      "requests": 91623,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 27.814,
        "p50": 27.532,
        "p90": 33.142,
        "p99": 57.775,
        "p999": 1204.759,
        "max": 2167.323
      },
      "status_codes": {
        "200": 91623
      },
      "errors": {},
      "cpu_cores": 1.9,
      "mem_peak_mb": 124.6,
      "rps_per_core": 4821.0
    },
    {
      "app": "fastapi",
      "scenario": "get",
      "connections": 16,
      "rps": 4064.9,
      "requests": 40683,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 3.916,
        "p50": 3.348,
        "p90": 6.219,
        "p99": 8.517,
        "p999": 16.831,
        "max": 212.826
      },
      "status_codes": {
        "200": 40683
      },
      "errors": {},
      "cpu_cores": 1.96,
      "mem_peak_mb": 124.4,
      "rps_per_core": 2069.5
    },
    {
      "app": "fastapi",
      "scenario": "get",
      "connections": 64,
      "rps": 5813.2,
      "requests": 59200,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 10.781,
        "p50": 5.986,
        "p90": 11.454,
        "p99": 68.116,
        "p999": 783.442,
        "max": 1967.562
      },
      "status_codes": {
        "200": 59200
      },
      "errors": {},
      "cpu_cores": 1.35,
      "mem_peak_mb": 105.7,
      "rps_per_core": 4299.8
    },
    {
      "app": "fastapi",
      "scenario": "get",
      "connections": 256,
      "rps": 3023.0,
      "requests": 30014,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 84.561,
        "p50": 84.029,
        "p90": 144.15,
        "p99": 233.936,
        "p999": 327.137,
        "max": 525.25
      },
      "status_codes": {
        "200": 30014
      },
      "errors": {},
      "cpu_cores": 1.98,
      "mem_peak_mb": 105.4,
      "rps_per_core": 1525.8
    },
    {
      "app": "fastapi",
      "scenario": "list",
      "connections": 16,
      "rps": 1684.5,
      "requests": 16837,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 9.471,
        "p50": 8.972,
        "p90": 12.889,
        "p99": 15.3,
        "p999": 18.226,
        "max": 37.556
      },
      "status_codes": {
        "200": 16837
      },
      "errors": {},
      "cpu_cores": 1.98,
      "mem_peak_mb": 105.0,
      "rps_per_core": 849.1
    },
    {
      "app": "fastapi",
      "scenario": "list",
      "connections": 64,
      "rps": 2155.9,
      "requests": 24143,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 26.424,
        "p50": 25.698,
        "p90": 37.719,
        "p99": 52.902,
        "p999": 83.645,
        "max": 101.737
      },
      "status_codes": {
        "200": 24143
      },
      "errors": {},
      "cpu_cores": 1.78,
      "mem_peak_mb": 103.5,
      "rps_per_core": 1208.9
    },
    {
      "app": "fastapi",
      "scenario": "list",
      "connections": 256,
      "rps": 2882.6,
      "requests": 29419,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 86.418,
        "p50": 88.631,
        "p90": 121.93,
        "p99": 171.454,
        "p999": 249.313,
        "max": 306.01
      },
      "status_codes": {
        "200": 29419
      },
      "errors": {},
      "cpu_cores": 1.94,
      "mem_peak_mb": 78.4,
      "rps_per_core": 1483.2
    },
    {
      "app": "fastapi",
      "scenario": "create",
      "connections": 16,
      "rps": 986.6,
      "requests": 9854,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 16.218,
        "p50": 7.611,
        "p90": 42.391,
        "p99": 68.456,
        "p999": 72.55,
        "max": 73.967
      },
      "status_codes": {
        "200": 9854
      },
      "errors": {},
      "cpu_cores": 0.21,
      "mem_peak_mb": 81.5,
      "rps_per_core": 4662.6
    },
    {
      "app": "fastapi",
      "scenario": "create",
      "connections": 64,
      "rps": 3498.5,
      "requests": 34932,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 18.275,
        "p50": 14.748,
        "p90": 20.135,
        "p99": 89.069,
        "p999": 125.846,
        "max": 182.359
      },
      "status_codes": {
        "200": 34932
      },
      "errors": {},
      "cpu_cores": 0.63,
      "mem_peak_mb": 81.5,
      "rps_per_core": 5540.5
    },
    {
      "app": "fastapi",
      "scenario": "create",
      "connections": 256,
      "rps": 7163.9,
      "requests": 71430,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 35.645,
        "p50": 32.673,
        "p90": 52.921,
        "p99": 70.037,
        "p999": 99.443,
        "max": 155.263
      },
      "status_codes": {
        "200": 71430
      },
      "errors": {},
      "cpu_cores": 1.86,
      "mem_peak_mb": 81.6,
      "rps_per_core": 3859.6
    }
  ]
};
