window.BENCH_RESULTS = {
  "meta": {
    "date": "2026-11-13T13:13:52+01:00",
    "trust_commit": "f9b6963",
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
    ],
    "merged_from": "2026-11-13T13:05:07+01:00"
  },
  "apps": [
    {
      "id": "trust",
      "name": "Trust",
      "family": "Rust (generated)",
      "image": "trust-bench/trust",
      "build_s": 96.0,
      "build_cold": true,
      "image_mb": 30.7,
      "loc": 32,
      "sources": [
        "bench/apps/trust/app.tr",
        "bench/apps/trust/ping.rs"
      ],
      "generated_loc": 1101,
      "startup_ms": 207.3,
      "idle_mem_mb": 2.1
    },
    {
      "id": "rust-axum",
      "name": "Rust \u00b7 axum",
      "family": "Rust",
      "image": "trust-bench/rust-axum",
      "build_s": 99.7,
      "build_cold": true,
      "image_mb": 29.3,
      "loc": 84,
      "sources": [
        "bench/apps/rust-axum/src/main.rs"
      ],
      "startup_ms": 245.8,
      "idle_mem_mb": 2.2
    },
    {
      "id": "go",
      "name": "Go \u00b7 net/http",
      "family": "Go",
      "image": "trust-bench/go",
      "build_s": 24.6,
      "build_cold": true,
      "image_mb": 7.1,
      "loc": 111,
      "sources": [
        "bench/apps/go/main.go"
      ],
      "startup_ms": 246.0,
      "idle_mem_mb": 3.1
    },
    {
      "id": "fastify",
      "name": "Node \u00b7 Fastify",
      "family": "Node.js",
      "image": "trust-bench/fastify",
      "build_s": 7.1,
      "build_cold": true,
      "image_mb": 67.5,
      "loc": 45,
      "sources": [
        "bench/apps/fastify/server.js"
      ],
      "startup_ms": 1389.6,
      "idle_mem_mb": 64.1
    },
    {
      "id": "nestjs-express",
      "name": "NestJS \u00b7 Express",
      "family": "Node.js",
      "image": "trust-bench/nestjs",
      "build_s": 22.1,
      "build_cold": true,
      "image_mb": 65.5,
      "loc": 69,
      "sources": [
        "bench/apps/nestjs/src/main.ts"
      ],
      "startup_ms": 922.4,
      "idle_mem_mb": 101.8
    },
    {
      "id": "nestjs-fastify",
      "name": "NestJS \u00b7 Fastify",
      "family": "Node.js",
      "image": "trust-bench/nestjs",
      "build_s": 20.1,
      "build_cold": true,
      "image_mb": 65.5,
      "loc": 69,
      "sources": [
        "bench/apps/nestjs/src/main.ts"
      ],
      "startup_ms": 852.4,
      "idle_mem_mb": 98.7
    },
    {
      "id": "fastapi",
      "name": "Python \u00b7 FastAPI",
      "family": "Python",
      "image": "trust-bench/fastapi",
      "build_s": 10.9,
      "build_cold": true,
      "image_mb": 62.9,
      "loc": 40,
      "sources": [
        "bench/apps/fastapi/main.py"
      ],
      "startup_ms": 788.1,
      "idle_mem_mb": 95.3
    }
  ],
  "runs": [
    {
      "app": "trust",
      "scenario": "ping",
      "connections": 16,
      "rps": 258583.4,
      "requests": 2587294,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.06,
        "p50": 0.059,
        "p90": 0.08,
        "p99": 0.096,
        "p999": 0.114,
        "max": 20.084
      },
      "status_codes": {
        "200": 2587294
      },
      "errors": {},
      "cpu_cores": 1.98,
      "mem_peak_mb": 4.1,
      "rps_per_core": 130713.9,
      "interference_cores": 0.0,
      "host_busy_before": 0.42,
      "noisy": false
    },
    {
      "app": "trust",
      "scenario": "ping",
      "connections": 64,
      "rps": 239197.9,
      "requests": 2395350,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.265,
        "p50": 0.265,
        "p90": 0.36,
        "p99": 0.462,
        "p999": 0.655,
        "max": 22.636
      },
      "status_codes": {
        "200": 2395350
      },
      "errors": {},
      "cpu_cores": 1.97,
      "mem_peak_mb": 5.7,
      "rps_per_core": 121311.8,
      "interference_cores": 0.02,
      "host_busy_before": 0.39,
      "noisy": false
    },
    {
      "app": "trust",
      "scenario": "ping",
      "connections": 256,
      "rps": 254623.8,
      "requests": 2548189,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 1.0,
        "p50": 1.001,
        "p90": 1.376,
        "p99": 1.655,
        "p999": 2.535,
        "max": 47.813
      },
      "status_codes": {
        "200": 2548189
      },
      "errors": {},
      "cpu_cores": 1.99,
      "mem_peak_mb": 14.3,
      "rps_per_core": 127859.6,
      "interference_cores": 0.01,
      "host_busy_before": 0.65,
      "noisy": false
    },
    {
      "app": "trust",
      "scenario": "get",
      "connections": 16,
      "rps": 52146.0,
      "requests": 521626,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.303,
        "p50": 0.3,
        "p90": 0.374,
        "p99": 0.456,
        "p999": 0.723,
        "max": 114.541
      },
      "status_codes": {
        "200": 521626
      },
      "errors": {},
      "cpu_cores": 1.97,
      "mem_peak_mb": 15.3,
      "rps_per_core": 26437.6,
      "interference_cores": 0.01,
      "host_busy_before": 0.25,
      "noisy": false
    },
    {
      "app": "trust",
      "scenario": "get",
      "connections": 64,
      "rps": 50303.4,
      "requests": 503123,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 1.267,
        "p50": 1.237,
        "p90": 1.415,
        "p99": 2.101,
        "p999": 3.734,
        "max": 96.478
      },
      "status_codes": {
        "200": 503123
      },
      "errors": {},
      "cpu_cores": 1.92,
      "mem_peak_mb": 16.7,
      "rps_per_core": 26176.2,
      "interference_cores": 0.02,
      "host_busy_before": 0.39,
      "noisy": false
    },
    {
      "app": "trust",
      "scenario": "get",
      "connections": 256,
      "rps": 48955.7,
      "requests": 489471,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 5.201,
        "p50": 5.15,
        "p90": 5.489,
        "p99": 6.39,
        "p999": 8.258,
        "max": 156.65
      },
      "status_codes": {
        "200": 489471
      },
      "errors": {},
      "cpu_cores": 1.97,
      "mem_peak_mb": 22.6,
      "rps_per_core": 24868.8,
      "interference_cores": 0.0,
      "host_busy_before": 0.67,
      "noisy": false
    },
    {
      "app": "trust",
      "scenario": "list",
      "connections": 16,
      "rps": 30352.8,
      "requests": 303645,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.523,
        "p50": 0.498,
        "p90": 0.732,
        "p99": 1.065,
        "p999": 2.961,
        "max": 33.404
      },
      "status_codes": {
        "200": 303645
      },
      "errors": {},
      "cpu_cores": 1.85,
      "mem_peak_mb": 20.9,
      "rps_per_core": 16385.7,
      "interference_cores": 0.01,
      "host_busy_before": 0.36,
      "noisy": false
    },
    {
      "app": "trust",
      "scenario": "list",
      "connections": 64,
      "rps": 23209.1,
      "requests": 232202,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 2.749,
        "p50": 2.647,
        "p90": 3.331,
        "p99": 4.902,
        "p999": 6.654,
        "max": 456.282
      },
      "status_codes": {
        "200": 232202
      },
      "errors": {},
      "cpu_cores": 1.46,
      "mem_peak_mb": 21.1,
      "rps_per_core": 15896.4,
      "interference_cores": 0.46,
      "host_busy_before": 1.11,
      "noisy": true
    },
    {
      "app": "trust",
      "scenario": "list",
      "connections": 256,
      "rps": 20515.1,
      "requests": 204992,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 12.44,
        "p50": 13.054,
        "p90": 15.165,
        "p99": 18.107,
        "p999": 24.818,
        "max": 240.88
      },
      "status_codes": {
        "200": 204992
      },
      "errors": {},
      "cpu_cores": 1.3,
      "mem_peak_mb": 27.5,
      "rps_per_core": 15793.7,
      "interference_cores": 0.61,
      "host_busy_before": 1.41,
      "noisy": true
    },
    {
      "app": "trust",
      "scenario": "create",
      "connections": 16,
      "rps": 7067.3,
      "requests": 70670,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 2.259,
        "p50": 2.072,
        "p90": 3.335,
        "p99": 4.072,
        "p999": 8.557,
        "max": 61.454
      },
      "status_codes": {
        "200": 70670
      },
      "errors": {},
      "cpu_cores": 0.28,
      "mem_peak_mb": 25.3,
      "rps_per_core": 25026.5,
      "interference_cores": 0.86,
      "host_busy_before": 1.35,
      "noisy": true
    },
    {
      "app": "trust",
      "scenario": "create",
      "connections": 64,
      "rps": 13942.9,
      "requests": 139416,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 4.581,
        "p50": 4.243,
        "p90": 5.626,
        "p99": 6.723,
        "p999": 18.538,
        "max": 53.234
      },
      "status_codes": {
        "200": 139416
      },
      "errors": {},
      "cpu_cores": 0.61,
      "mem_peak_mb": 25.4,
      "rps_per_core": 22891.3,
      "interference_cores": 0.07,
      "host_busy_before": 1.24,
      "noisy": false
    },
    {
      "app": "trust",
      "scenario": "create",
      "connections": 256,
      "rps": 9400.3,
      "requests": 93785,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 27.173,
        "p50": 19.388,
        "p90": 53.315,
        "p99": 82.041,
        "p999": 85.859,
        "max": 106.521
      },
      "status_codes": {
        "200": 93785
      },
      "errors": {},
      "cpu_cores": 0.46,
      "mem_peak_mb": 27.7,
      "rps_per_core": 20505.6,
      "interference_cores": 0.57,
      "host_busy_before": 1.63,
      "noisy": true
    },
    {
      "app": "rust-axum",
      "scenario": "ping",
      "connections": 16,
      "rps": 261705.2,
      "requests": 2618075,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.059,
        "p50": 0.058,
        "p90": 0.079,
        "p99": 0.097,
        "p999": 0.118,
        "max": 22.573
      },
      "status_codes": {
        "200": 2618075
      },
      "errors": {},
      "cpu_cores": 1.97,
      "mem_peak_mb": 3.5,
      "rps_per_core": 132528.4,
      "interference_cores": 0.04,
      "host_busy_before": 1.28,
      "noisy": false
    },
    {
      "app": "rust-axum",
      "scenario": "ping",
      "connections": 64,
      "rps": 246421.7,
      "requests": 2467898,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.257,
        "p50": 0.254,
        "p90": 0.352,
        "p99": 0.511,
        "p999": 0.67,
        "max": 15.13
      },
      "status_codes": {
        "200": 2467898
      },
      "errors": {},
      "cpu_cores": 1.95,
      "mem_peak_mb": 5.7,
      "rps_per_core": 126309.8,
      "interference_cores": 0.06,
      "host_busy_before": 1.48,
      "noisy": false
    },
    {
      "app": "rust-axum",
      "scenario": "ping",
      "connections": 256,
      "rps": 270703.5,
      "requests": 2708663,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.941,
        "p50": 0.932,
        "p90": 1.276,
        "p99": 1.727,
        "p999": 2.957,
        "max": 38.307
      },
      "status_codes": {
        "200": 2708663
      },
      "errors": {},
      "cpu_cores": 1.99,
      "mem_peak_mb": 14.0,
      "rps_per_core": 136074.1,
      "interference_cores": 0.03,
      "host_busy_before": 1.37,
      "noisy": false
    },
    {
      "app": "rust-axum",
      "scenario": "get",
      "connections": 16,
      "rps": 38031.4,
      "requests": 380449,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.417,
        "p50": 0.423,
        "p90": 0.481,
        "p99": 0.865,
        "p999": 3.456,
        "max": 14.823
      },
      "status_codes": {
        "200": 380449
      },
      "errors": {},
      "cpu_cores": 1.21,
      "mem_peak_mb": 13.0,
      "rps_per_core": 31465.1,
      "interference_cores": 0.71,
      "host_busy_before": 1.35,
      "noisy": true
    },
    {
      "app": "rust-axum",
      "scenario": "get",
      "connections": 64,
      "rps": 50865.7,
      "requests": 508748,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 1.253,
        "p50": 1.215,
        "p90": 1.444,
        "p99": 2.076,
        "p999": 3.423,
        "max": 72.908
      },
      "status_codes": {
        "200": 508748
      },
      "errors": {},
      "cpu_cores": 1.92,
      "mem_peak_mb": 15.4,
      "rps_per_core": 26544.1,
      "interference_cores": 0.03,
      "host_busy_before": 1.31,
      "noisy": false
    },
    {
      "app": "rust-axum",
      "scenario": "get",
      "connections": 256,
      "rps": 50494.5,
      "requests": 504821,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 5.052,
        "p50": 4.963,
        "p90": 5.518,
        "p99": 6.792,
        "p999": 9.126,
        "max": 95.133
      },
      "status_codes": {
        "200": 504821
      },
      "errors": {},
      "cpu_cores": 1.96,
      "mem_peak_mb": 21.0,
      "rps_per_core": 25817.7,
      "interference_cores": 0.01,
      "host_busy_before": 1.64,
      "noisy": false
    },
    {
      "app": "rust-axum",
      "scenario": "list",
      "connections": 16,
      "rps": 30632.3,
      "requests": 306436,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.518,
        "p50": 0.492,
        "p90": 0.722,
        "p99": 1.084,
        "p999": 2.953,
        "max": 131.636
      },
      "status_codes": {
        "200": 306436
      },
      "errors": {},
      "cpu_cores": 1.78,
      "mem_peak_mb": 18.4,
      "rps_per_core": 17245.7,
      "interference_cores": 0.05,
      "host_busy_before": 0.69,
      "noisy": false
    },
    {
      "app": "rust-axum",
      "scenario": "list",
      "connections": 64,
      "rps": 31496.5,
      "requests": 315001,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 2.024,
        "p50": 1.991,
        "p90": 2.388,
        "p99": 3.102,
        "p999": 4.212,
        "max": 132.737
      },
      "status_codes": {
        "200": 315001
      },
      "errors": {},
      "cpu_cores": 1.85,
      "mem_peak_mb": 19.9,
      "rps_per_core": 17032.4,
      "interference_cores": 0.02,
      "host_busy_before": 0.26,
      "noisy": false
    },
    {
      "app": "rust-axum",
      "scenario": "list",
      "connections": 256,
      "rps": 26242.4,
      "requests": 262316,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 9.72,
        "p50": 8.999,
        "p90": 12.848,
        "p99": 20.082,
        "p999": 29.099,
        "max": 97.537
      },
      "status_codes": {
        "200": 262316
      },
      "errors": {},
      "cpu_cores": 1.79,
      "mem_peak_mb": 22.0,
      "rps_per_core": 14638.4,
      "interference_cores": 0.14,
      "host_busy_before": 0.56,
      "noisy": true
    },
    {
      "app": "rust-axum",
      "scenario": "create",
      "connections": 16,
      "rps": 5779.8,
      "requests": 57814,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 2.761,
        "p50": 2.068,
        "p90": 3.34,
        "p99": 36.93,
        "p999": 65.83,
        "max": 69.638
      },
      "status_codes": {
        "200": 57814
      },
      "errors": {},
      "cpu_cores": 0.27,
      "mem_peak_mb": 20.1,
      "rps_per_core": 21067.0,
      "interference_cores": 0.09,
      "host_busy_before": 1.46,
      "noisy": false
    },
    {
      "app": "rust-axum",
      "scenario": "create",
      "connections": 64,
      "rps": 11185.6,
      "requests": 111844,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 5.708,
        "p50": 4.22,
        "p90": 5.719,
        "p99": 40.406,
        "p999": 67.313,
        "max": 72.959
      },
      "status_codes": {
        "200": 111844
      },
      "errors": {},
      "cpu_cores": 0.45,
      "mem_peak_mb": 20.6,
      "rps_per_core": 24610.6,
      "interference_cores": 0.08,
      "host_busy_before": 0.37,
      "noisy": false
    },
    {
      "app": "rust-axum",
      "scenario": "create",
      "connections": 256,
      "rps": 11167.5,
      "requests": 111475,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 22.892,
        "p50": 17.985,
        "p90": 50.283,
        "p99": 80.097,
        "p999": 125.627,
        "max": 164.231
      },
      "status_codes": {
        "200": 111475
      },
      "errors": {},
      "cpu_cores": 0.49,
      "mem_peak_mb": 22.9,
      "rps_per_core": 22942.7,
      "interference_cores": 0.07,
      "host_busy_before": 0.46,
      "noisy": false
    },
    {
      "app": "go",
      "scenario": "ping",
      "connections": 16,
      "rps": 93063.4,
      "requests": 931053,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.17,
        "p50": 0.117,
        "p90": 0.27,
        "p99": 1.366,
        "p999": 3.731,
        "max": 17.878
      },
      "status_codes": {
        "200": 931053
      },
      "errors": {},
      "cpu_cores": 1.42,
      "mem_peak_mb": 10.0,
      "rps_per_core": 65746.9,
      "interference_cores": 0.23,
      "host_busy_before": 1.52,
      "noisy": true
    },
    {
      "app": "go",
      "scenario": "ping",
      "connections": 64,
      "rps": 145748.8,
      "requests": 1458286,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.436,
        "p50": 0.363,
        "p90": 0.867,
        "p99": 1.429,
        "p999": 3.867,
        "max": 44.397
      },
      "status_codes": {
        "200": 1458286
      },
      "errors": {},
      "cpu_cores": 1.92,
      "mem_peak_mb": 10.7,
      "rps_per_core": 75792.3,
      "interference_cores": 0.02,
      "host_busy_before": 1.44,
      "noisy": false
    },
    {
      "app": "go",
      "scenario": "ping",
      "connections": 256,
      "rps": 92093.0,
      "requests": 921094,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 2.769,
        "p50": 2.405,
        "p90": 5.136,
        "p99": 9.915,
        "p999": 14.639,
        "max": 53.611
      },
      "status_codes": {
        "200": 921094
      },
      "errors": {},
      "cpu_cores": 1.74,
      "mem_peak_mb": 15.9,
      "rps_per_core": 52986.5,
      "interference_cores": 0.26,
      "host_busy_before": 1.74,
      "noisy": true
    },
    {
      "app": "go",
      "scenario": "get",
      "connections": 16,
      "rps": 49727.3,
      "requests": 497442,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.318,
        "p50": 0.239,
        "p90": 0.462,
        "p99": 3.049,
        "p999": 4.661,
        "max": 20.309
      },
      "status_codes": {
        "200": 497442
      },
      "errors": {},
      "cpu_cores": 1.56,
      "mem_peak_mb": 11.9,
      "rps_per_core": 31800.0,
      "interference_cores": 0.06,
      "host_busy_before": 1.29,
      "noisy": false
    },
    {
      "app": "go",
      "scenario": "get",
      "connections": 64,
      "rps": 51836.7,
      "requests": 518448,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 1.23,
        "p50": 0.987,
        "p90": 2.025,
        "p99": 4.983,
        "p999": 6.895,
        "max": 28.624
      },
      "status_codes": {
        "200": 518448
      },
      "errors": {},
      "cpu_cores": 1.65,
      "mem_peak_mb": 17.6,
      "rps_per_core": 31446.4,
      "interference_cores": 0.05,
      "host_busy_before": 0.37,
      "noisy": false
    },
    {
      "app": "go",
      "scenario": "get",
      "connections": 256,
      "rps": 45155.9,
      "requests": 451668,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 5.638,
        "p50": 5.099,
        "p90": 8.414,
        "p99": 12.726,
        "p999": 18.344,
        "max": 88.168
      },
      "status_codes": {
        "200": 451668
      },
      "errors": {},
      "cpu_cores": 1.61,
      "mem_peak_mb": 26.8,
      "rps_per_core": 28094.5,
      "interference_cores": 0.08,
      "host_busy_before": 0.7,
      "noisy": false
    },
    {
      "app": "go",
      "scenario": "list",
      "connections": 16,
      "rps": 18403.1,
      "requests": 184071,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.864,
        "p50": 0.621,
        "p90": 1.418,
        "p99": 4.994,
        "p999": 11.457,
        "max": 19.871
      },
      "status_codes": {
        "200": 184071
      },
      "errors": {},
      "cpu_cores": 1.16,
      "mem_peak_mb": 18.6,
      "rps_per_core": 15868.0,
      "interference_cores": 0.49,
      "host_busy_before": 1.65,
      "noisy": true
    },
    {
      "app": "go",
      "scenario": "list",
      "connections": 64,
      "rps": 24159.9,
      "requests": 241592,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 2.64,
        "p50": 2.137,
        "p90": 4.862,
        "p99": 8.504,
        "p999": 13.667,
        "max": 67.984
      },
      "status_codes": {
        "200": 241592
      },
      "errors": {},
      "cpu_cores": 1.32,
      "mem_peak_mb": 21.7,
      "rps_per_core": 18301.0,
      "interference_cores": 0.15,
      "host_busy_before": 0.74,
      "noisy": true
    },
    {
      "app": "go",
      "scenario": "list",
      "connections": 256,
      "rps": 25622.3,
      "requests": 256068,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 9.95,
        "p50": 9.579,
        "p90": 12.59,
        "p99": 16.879,
        "p999": 23.085,
        "max": 75.54
      },
      "status_codes": {
        "200": 256068
      },
      "errors": {},
      "cpu_cores": 1.25,
      "mem_peak_mb": 28.9,
      "rps_per_core": 20575.3,
      "interference_cores": 0.05,
      "host_busy_before": 1.35,
      "noisy": false
    },
    {
      "app": "go",
      "scenario": "create",
      "connections": 16,
      "rps": 4718.2,
      "requests": 47177,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 3.385,
        "p50": 2.259,
        "p90": 3.819,
        "p99": 8.706,
        "p999": 206.892,
        "max": 685.17
      },
      "status_codes": {
        "200": 47177
      },
      "errors": {},
      "cpu_cores": 0.28,
      "mem_peak_mb": 14.3,
      "rps_per_core": 17153.8,
      "interference_cores": 0.42,
      "host_busy_before": 1.53,
      "noisy": true
    },
    {
      "app": "go",
      "scenario": "create",
      "connections": 64,
      "rps": 10983.9,
      "requests": 109809,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 5.817,
        "p50": 4.261,
        "p90": 5.851,
        "p99": 39.6,
        "p999": 70.411,
        "max": 598.652
      },
      "status_codes": {
        "200": 109809
      },
      "errors": {},
      "cpu_cores": 0.47,
      "mem_peak_mb": 21.2,
      "rps_per_core": 23174.1,
      "interference_cores": 0.16,
      "host_busy_before": 0.76,
      "noisy": true
    },
    {
      "app": "go",
      "scenario": "create",
      "connections": 256,
      "rps": 9113.0,
      "requests": 90909,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 28.04,
        "p50": 18.495,
        "p90": 52.401,
        "p99": 85.652,
        "p999": 650.126,
        "max": 653.023
      },
      "status_codes": {
        "200": 90909
      },
      "errors": {},
      "cpu_cores": 0.44,
      "mem_peak_mb": 32.2,
      "rps_per_core": 20937.8,
      "interference_cores": 0.25,
      "host_busy_before": 0.47,
      "noisy": true
    },
    {
      "app": "fastify",
      "scenario": "ping",
      "connections": 16,
      "rps": 95560.9,
      "requests": 956182,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.166,
        "p50": 0.159,
        "p90": 0.23,
        "p99": 0.362,
        "p999": 0.552,
        "max": 22.008
      },
      "status_codes": {
        "200": 956182
      },
      "errors": {},
      "cpu_cores": 2.0,
      "mem_peak_mb": 69.3,
      "rps_per_core": 47823.4,
      "interference_cores": 0.01,
      "host_busy_before": 0.11,
      "noisy": false
    },
    {
      "app": "fastify",
      "scenario": "ping",
      "connections": 64,
      "rps": 95187.9,
      "requests": 952126,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.67,
        "p50": 0.631,
        "p90": 0.821,
        "p99": 1.273,
        "p999": 1.531,
        "max": 32.772
      },
      "status_codes": {
        "200": 952126
      },
      "errors": {},
      "cpu_cores": 2.0,
      "mem_peak_mb": 71.0,
      "rps_per_core": 47695.8,
      "interference_cores": 0.01,
      "host_busy_before": 0.15,
      "noisy": false
    },
    {
      "app": "fastify",
      "scenario": "ping",
      "connections": 256,
      "rps": 91432.5,
      "requests": 914407,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 2.793,
        "p50": 2.776,
        "p90": 3.064,
        "p99": 4.176,
        "p999": 5.732,
        "max": 195.663
      },
      "status_codes": {
        "200": 914407
      },
      "errors": {},
      "cpu_cores": 2.0,
      "mem_peak_mb": 73.8,
      "rps_per_core": 45798.8,
      "interference_cores": 0.01,
      "host_busy_before": 0.21,
      "noisy": false
    },
    {
      "app": "fastify",
      "scenario": "get",
      "connections": 16,
      "rps": 28775.9,
      "requests": 287855,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.552,
        "p50": 0.516,
        "p90": 0.749,
        "p99": 1.006,
        "p999": 1.335,
        "max": 13.434
      },
      "status_codes": {
        "200": 287855
      },
      "errors": {},
      "cpu_cores": 2.0,
      "mem_peak_mb": 77.3,
      "rps_per_core": 14392.0,
      "interference_cores": 0.01,
      "host_busy_before": 0.33,
      "noisy": false
    },
    {
      "app": "fastify",
      "scenario": "get",
      "connections": 64,
      "rps": 30272.2,
      "requests": 302770,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 2.108,
        "p50": 2.056,
        "p90": 2.684,
        "p99": 3.252,
        "p999": 3.783,
        "max": 38.621
      },
      "status_codes": {
        "200": 302770
      },
      "errors": {},
      "cpu_cores": 2.0,
      "mem_peak_mb": 78.7,
      "rps_per_core": 15157.6,
      "interference_cores": 0.01,
      "host_busy_before": 0.25,
      "noisy": false
    },
    {
      "app": "fastify",
      "scenario": "get",
      "connections": 256,
      "rps": 29399.4,
      "requests": 293876,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 8.677,
        "p50": 8.707,
        "p90": 9.573,
        "p99": 11.17,
        "p999": 17.185,
        "max": 130.435
      },
      "status_codes": {
        "200": 293876
      },
      "errors": {},
      "cpu_cores": 2.0,
      "mem_peak_mb": 83.1,
      "rps_per_core": 14730.5,
      "interference_cores": 0.01,
      "host_busy_before": 0.2,
      "noisy": false
    },
    {
      "app": "fastify",
      "scenario": "list",
      "connections": 16,
      "rps": 20700.8,
      "requests": 207056,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.769,
        "p50": 0.761,
        "p90": 0.989,
        "p99": 1.313,
        "p999": 4.085,
        "max": 22.94
      },
      "status_codes": {
        "200": 207056
      },
      "errors": {},
      "cpu_cores": 2.0,
      "mem_peak_mb": 84.7,
      "rps_per_core": 10371.9,
      "interference_cores": 0.02,
      "host_busy_before": 0.1,
      "noisy": false
    },
    {
      "app": "fastify",
      "scenario": "list",
      "connections": 64,
      "rps": 20053.8,
      "requests": 200558,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 3.183,
        "p50": 3.075,
        "p90": 4.086,
        "p99": 4.912,
        "p999": 7.046,
        "max": 34.328
      },
      "status_codes": {
        "200": 200558
      },
      "errors": {},
      "cpu_cores": 2.0,
      "mem_peak_mb": 82.4,
      "rps_per_core": 10042.0,
      "interference_cores": 0.02,
      "host_busy_before": 1.04,
      "noisy": false
    },
    {
      "app": "fastify",
      "scenario": "list",
      "connections": 256,
      "rps": 18292.0,
      "requests": 182743,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 13.947,
        "p50": 13.455,
        "p90": 14.692,
        "p99": 26.592,
        "p999": 40.789,
        "max": 184.15
      },
      "status_codes": {
        "200": 182743
      },
      "errors": {},
      "cpu_cores": 2.0,
      "mem_peak_mb": 97.4,
      "rps_per_core": 9168.3,
      "interference_cores": 0.01,
      "host_busy_before": 0.27,
      "noisy": false
    },
    {
      "app": "fastify",
      "scenario": "create",
      "connections": 16,
      "rps": 7101.0,
      "requests": 71007,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 2.248,
        "p50": 2.069,
        "p90": 3.324,
        "p99": 3.995,
        "p999": 8.537,
        "max": 45.103
      },
      "status_codes": {
        "200": 71007
      },
      "errors": {},
      "cpu_cores": 0.64,
      "mem_peak_mb": 96.6,
      "rps_per_core": 11146.3,
      "interference_cores": 0.06,
      "host_busy_before": 0.39,
      "noisy": false
    },
    {
      "app": "fastify",
      "scenario": "create",
      "connections": 64,
      "rps": 13984.7,
      "requests": 139823,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 4.567,
        "p50": 4.229,
        "p90": 5.674,
        "p99": 6.869,
        "p999": 24.15,
        "max": 50.118
      },
      "status_codes": {
        "200": 139823
      },
      "errors": {},
      "cpu_cores": 1.08,
      "mem_peak_mb": 96.5,
      "rps_per_core": 12946.0,
      "interference_cores": 0.0,
      "host_busy_before": 0.22,
      "noisy": false
    },
    {
      "app": "fastify",
      "scenario": "create",
      "connections": 256,
      "rps": 10990.4,
      "requests": 109690,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 23.242,
        "p50": 23.136,
        "p90": 25.946,
        "p99": 32.559,
        "p999": 72.345,
        "max": 102.996
      },
      "status_codes": {
        "200": 109690
      },
      "errors": {},
      "cpu_cores": 1.06,
      "mem_peak_mb": 97.9,
      "rps_per_core": 10366.1,
      "interference_cores": 0.79,
      "host_busy_before": 1.65,
      "noisy": true
    },
    {
      "app": "nestjs-express",
      "scenario": "ping",
      "connections": 16,
      "rps": 18405.4,
      "requests": 184083,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.867,
        "p50": 0.74,
        "p90": 1.247,
        "p99": 5.88,
        "p999": 11.254,
        "max": 25.376
      },
      "status_codes": {
        "200": 184083
      },
      "errors": {},
      "cpu_cores": 1.94,
      "mem_peak_mb": 131.5,
      "rps_per_core": 9499.6,
      "interference_cores": 0.09,
      "host_busy_before": 1.42,
      "noisy": false
    },
    {
      "app": "nestjs-express",
      "scenario": "ping",
      "connections": 64,
      "rps": 19060.4,
      "requests": 190591,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 3.348,
        "p50": 2.692,
        "p90": 3.842,
        "p99": 14.02,
        "p999": 16.909,
        "max": 85.335
      },
      "status_codes": {
        "200": 190591
      },
      "errors": {},
      "cpu_cores": 1.99,
      "mem_peak_mb": 123.7,
      "rps_per_core": 9577.0,
      "interference_cores": 0.04,
      "host_busy_before": 1.53,
      "noisy": false
    },
    {
      "app": "nestjs-express",
      "scenario": "ping",
      "connections": 256,
      "rps": 18446.6,
      "requests": 184291,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 13.828,
        "p50": 11.786,
        "p90": 21.684,
        "p99": 28.49,
        "p999": 48.16,
        "max": 887.108
      },
      "status_codes": {
        "200": 184291
      },
      "errors": {},
      "cpu_cores": 1.95,
      "mem_peak_mb": 131.9,
      "rps_per_core": 9437.1,
      "interference_cores": 0.09,
      "host_busy_before": 1.72,
      "noisy": false
    },
    {
      "app": "nestjs-express",
      "scenario": "get",
      "connections": 16,
      "rps": 10511.2,
      "requests": 105122,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 1.516,
        "p50": 1.31,
        "p90": 1.99,
        "p99": 7.02,
        "p999": 14.97,
        "max": 41.367
      },
      "status_codes": {
        "200": 105122
      },
      "errors": {},
      "cpu_cores": 1.99,
      "mem_peak_mb": 132.5,
      "rps_per_core": 5289.5,
      "interference_cores": 0.02,
      "host_busy_before": 0.91,
      "noisy": false
    },
    {
      "app": "nestjs-express",
      "scenario": "get",
      "connections": 64,
      "rps": 10399.9,
      "requests": 103961,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 6.141,
        "p50": 5.422,
        "p90": 8.146,
        "p99": 20.361,
        "p999": 28.671,
        "max": 83.495
      },
      "status_codes": {
        "200": 103961
      },
      "errors": {},
      "cpu_cores": 1.96,
      "mem_peak_mb": 134.0,
      "rps_per_core": 5313.6,
      "interference_cores": 0.06,
      "host_busy_before": 0.58,
      "noisy": false
    },
    {
      "app": "nestjs-express",
      "scenario": "get",
      "connections": 256,
      "rps": 9001.4,
      "requests": 89789,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 28.349,
        "p50": 25.249,
        "p90": 41.99,
        "p99": 53.243,
        "p999": 233.693,
        "max": 382.711
      },
      "status_codes": {
        "200": 89789
      },
      "errors": {},
      "cpu_cores": 1.99,
      "mem_peak_mb": 133.9,
      "rps_per_core": 4525.7,
      "interference_cores": 0.02,
      "host_busy_before": 1.6,
      "noisy": false
    },
    {
      "app": "nestjs-express",
      "scenario": "list",
      "connections": 16,
      "rps": 9055.3,
      "requests": 90549,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 1.762,
        "p50": 1.635,
        "p90": 2.214,
        "p99": 7.287,
        "p999": 15.083,
        "max": 28.407
      },
      "status_codes": {
        "200": 90549
      },
      "errors": {},
      "cpu_cores": 2.0,
      "mem_peak_mb": 133.6,
      "rps_per_core": 4536.0,
      "interference_cores": 0.01,
      "host_busy_before": 0.54,
      "noisy": false
    },
    {
      "app": "nestjs-express",
      "scenario": "list",
      "connections": 64,
      "rps": 9249.4,
      "requests": 92453,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 6.911,
        "p50": 6.38,
        "p90": 8.818,
        "p99": 22.007,
        "p999": 28.828,
        "max": 55.732
      },
      "status_codes": {
        "200": 92453
      },
      "errors": {},
      "cpu_cores": 1.99,
      "mem_peak_mb": 134.1,
      "rps_per_core": 4636.7,
      "interference_cores": 0.02,
      "host_busy_before": 0.46,
      "noisy": false
    },
    {
      "app": "nestjs-express",
      "scenario": "list",
      "connections": 256,
      "rps": 7093.4,
      "requests": 70711,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 36.079,
        "p50": 32.088,
        "p90": 48.685,
        "p99": 60.389,
        "p999": 389.42,
        "max": 578.857
      },
      "status_codes": {
        "200": 70711
      },
      "errors": {},
      "cpu_cores": 1.96,
      "mem_peak_mb": 134.9,
      "rps_per_core": 3615.1,
      "interference_cores": 0.02,
      "host_busy_before": 0.55,
      "noisy": false
    },
    {
      "app": "nestjs-express",
      "scenario": "create",
      "connections": 16,
      "rps": 5985.2,
      "requests": 59850,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 2.669,
        "p50": 2.232,
        "p90": 3.754,
        "p99": 7.494,
        "p999": 16.119,
        "max": 47.617
      },
      "status_codes": {
        "201": 59850
      },
      "errors": {},
      "cpu_cores": 1.34,
      "mem_peak_mb": 134.4,
      "rps_per_core": 4460.7,
      "interference_cores": 0.05,
      "host_busy_before": 0.5,
      "noisy": false
    },
    {
      "app": "nestjs-express",
      "scenario": "create",
      "connections": 64,
      "rps": 8605.5,
      "requests": 86005,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 7.426,
        "p50": 6.614,
        "p90": 9.32,
        "p99": 22.955,
        "p999": 44.499,
        "max": 56.509
      },
      "status_codes": {
        "201": 86005
      },
      "errors": {},
      "cpu_cores": 1.91,
      "mem_peak_mb": 134.5,
      "rps_per_core": 4512.4,
      "interference_cores": 0.02,
      "host_busy_before": 0.46,
      "noisy": false
    },
    {
      "app": "nestjs-express",
      "scenario": "create",
      "connections": 256,
      "rps": 7118.1,
      "requests": 70952,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 35.949,
        "p50": 33.364,
        "p90": 50.605,
        "p99": 70.386,
        "p999": 181.126,
        "max": 302.7
      },
      "status_codes": {
        "201": 70952
      },
      "errors": {},
      "cpu_cores": 1.94,
      "mem_peak_mb": 135.1,
      "rps_per_core": 3671.8,
      "interference_cores": 0.03,
      "host_busy_before": 0.42,
      "noisy": false
    },
    {
      "app": "nestjs-fastify",
      "scenario": "ping",
      "connections": 16,
      "rps": 89275.5,
      "requests": 893299,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.177,
        "p50": 0.147,
        "p90": 0.291,
        "p99": 0.406,
        "p999": 0.638,
        "max": 19.538
      },
      "status_codes": {
        "200": 893299
      },
      "errors": {},
      "cpu_cores": 2.0,
      "mem_peak_mb": 104.1,
      "rps_per_core": 44696.8,
      "interference_cores": 0.01,
      "host_busy_before": 1.02,
      "noisy": false
    },
    {
      "app": "nestjs-fastify",
      "scenario": "ping",
      "connections": 64,
      "rps": 89724.4,
      "requests": 897486,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.711,
        "p50": 0.675,
        "p90": 0.925,
        "p99": 1.475,
        "p999": 2.397,
        "max": 31.467
      },
      "status_codes": {
        "200": 897486
      },
      "errors": {},
      "cpu_cores": 2.0,
      "mem_peak_mb": 97.0,
      "rps_per_core": 44873.5,
      "interference_cores": 0.02,
      "host_busy_before": 1.28,
      "noisy": false
    },
    {
      "app": "nestjs-fastify",
      "scenario": "ping",
      "connections": 256,
      "rps": 78606.3,
      "requests": 786132,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 3.246,
        "p50": 3.115,
        "p90": 3.744,
        "p99": 5.579,
        "p999": 8.455,
        "max": 271.061
      },
      "status_codes": {
        "200": 786132
      },
      "errors": {},
      "cpu_cores": 1.99,
      "mem_peak_mb": 99.9,
      "rps_per_core": 39583.1,
      "interference_cores": 0.06,
      "host_busy_before": 0.87,
      "noisy": false
    },
    {
      "app": "nestjs-fastify",
      "scenario": "get",
      "connections": 16,
      "rps": 22120.8,
      "requests": 221248,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.719,
        "p50": 0.673,
        "p90": 0.991,
        "p99": 1.453,
        "p999": 3.354,
        "max": 19.628
      },
      "status_codes": {
        "200": 221248
      },
      "errors": {},
      "cpu_cores": 1.99,
      "mem_peak_mb": 91.2,
      "rps_per_core": 11116.8,
      "interference_cores": 0.07,
      "host_busy_before": 1.67,
      "noisy": false
    },
    {
      "app": "nestjs-fastify",
      "scenario": "get",
      "connections": 64,
      "rps": 23672.2,
      "requests": 236747,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 2.695,
        "p50": 2.611,
        "p90": 3.456,
        "p99": 4.289,
        "p999": 6.92,
        "max": 38.779
      },
      "status_codes": {
        "200": 236747
      },
      "errors": {},
      "cpu_cores": 1.99,
      "mem_peak_mb": 94.8,
      "rps_per_core": 11870.4,
      "interference_cores": 0.04,
      "host_busy_before": 1.31,
      "noisy": false
    },
    {
      "app": "nestjs-fastify",
      "scenario": "get",
      "connections": 256,
      "rps": 21829.7,
      "requests": 218113,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 11.687,
        "p50": 11.561,
        "p90": 14.318,
        "p99": 16.325,
        "p999": 39.393,
        "max": 174.451
      },
      "status_codes": {
        "200": 218113
      },
      "errors": {},
      "cpu_cores": 2.0,
      "mem_peak_mb": 105.5,
      "rps_per_core": 10935.7,
      "interference_cores": 0.04,
      "host_busy_before": 1.49,
      "noisy": false
    },
    {
      "app": "nestjs-fastify",
      "scenario": "list",
      "connections": 16,
      "rps": 16289.9,
      "requests": 162930,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.977,
        "p50": 0.861,
        "p90": 1.416,
        "p99": 2.332,
        "p999": 4.597,
        "max": 25.86
      },
      "status_codes": {
        "200": 162930
      },
      "errors": {},
      "cpu_cores": 1.97,
      "mem_peak_mb": 105.3,
      "rps_per_core": 8283.6,
      "interference_cores": 0.04,
      "host_busy_before": 1.79,
      "noisy": false
    },
    {
      "app": "nestjs-fastify",
      "scenario": "list",
      "connections": 64,
      "rps": 16724.8,
      "requests": 167238,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 3.815,
        "p50": 3.615,
        "p90": 4.992,
        "p99": 7.541,
        "p999": 10.36,
        "max": 44.047
      },
      "status_codes": {
        "200": 167238
      },
      "errors": {},
      "cpu_cores": 1.99,
      "mem_peak_mb": 106.5,
      "rps_per_core": 8397.1,
      "interference_cores": 0.02,
      "host_busy_before": 1.54,
      "noisy": false
    },
    {
      "app": "nestjs-fastify",
      "scenario": "list",
      "connections": 256,
      "rps": 15418.6,
      "requests": 153994,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 16.55,
        "p50": 15.861,
        "p90": 18.541,
        "p99": 34.638,
        "p999": 47.402,
        "max": 194.744
      },
      "status_codes": {
        "200": 153994
      },
      "errors": {},
      "cpu_cores": 2.0,
      "mem_peak_mb": 112.5,
      "rps_per_core": 7721.1,
      "interference_cores": 0.04,
      "host_busy_before": 1.8,
      "noisy": false
    },
    {
      "app": "nestjs-fastify",
      "scenario": "create",
      "connections": 16,
      "rps": 7017.4,
      "requests": 70172,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 2.275,
        "p50": 2.082,
        "p90": 3.35,
        "p99": 4.348,
        "p999": 7.728,
        "max": 19.567
      },
      "status_codes": {
        "201": 70172
      },
      "errors": {},
      "cpu_cores": 0.71,
      "mem_peak_mb": 111.2,
      "rps_per_core": 9903.7,
      "interference_cores": 0.09,
      "host_busy_before": 1.45,
      "noisy": false
    },
    {
      "app": "nestjs-fastify",
      "scenario": "create",
      "connections": 64,
      "rps": 10303.1,
      "requests": 102997,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 6.184,
        "p50": 6.065,
        "p90": 7.798,
        "p99": 9.998,
        "p999": 21.503,
        "max": 52.178
      },
      "status_codes": {
        "201": 102997
      },
      "errors": {},
      "cpu_cores": 0.96,
      "mem_peak_mb": 111.5,
      "rps_per_core": 10681.9,
      "interference_cores": 0.96,
      "host_busy_before": 1.53,
      "noisy": true
    },
    {
      "app": "nestjs-fastify",
      "scenario": "create",
      "connections": 256,
      "rps": 9895.0,
      "requests": 98728,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 25.818,
        "p50": 25.426,
        "p90": 30.552,
        "p99": 45.558,
        "p999": 74.227,
        "max": 96.52
      },
      "status_codes": {
        "201": 98728
      },
      "errors": {},
      "cpu_cores": 1.05,
      "mem_peak_mb": 111.9,
      "rps_per_core": 9438.2,
      "interference_cores": 0.86,
      "host_busy_before": 1.27,
      "noisy": true
    },
    {
      "app": "fastapi",
      "scenario": "ping",
      "connections": 16,
      "rps": 26480.3,
      "requests": 264894,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 0.601,
        "p50": 0.552,
        "p90": 0.929,
        "p99": 1.256,
        "p999": 1.593,
        "max": 25.457
      },
      "status_codes": {
        "200": 264894
      },
      "errors": {},
      "cpu_cores": 2.0,
      "mem_peak_mb": 97.6,
      "rps_per_core": 13260.4,
      "interference_cores": 0.04,
      "host_busy_before": 1.52,
      "noisy": false
    },
    {
      "app": "fastapi",
      "scenario": "ping",
      "connections": 64,
      "rps": 20079.6,
      "requests": 200793,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 3.18,
        "p50": 2.566,
        "p90": 4.607,
        "p99": 7.27,
        "p999": 11.327,
        "max": 64.135
      },
      "status_codes": {
        "200": 200793
      },
      "errors": {},
      "cpu_cores": 1.98,
      "mem_peak_mb": 98.1,
      "rps_per_core": 10145.8,
      "interference_cores": 0.04,
      "host_busy_before": 1.64,
      "noisy": false
    },
    {
      "app": "fastapi",
      "scenario": "ping",
      "connections": 256,
      "rps": 21018.4,
      "requests": 210004,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 12.141,
        "p50": 11.316,
        "p90": 16.329,
        "p99": 26.278,
        "p999": 36.904,
        "max": 508.404
      },
      "status_codes": {
        "200": 210004
      },
      "errors": {},
      "cpu_cores": 1.99,
      "mem_peak_mb": 100.5,
      "rps_per_core": 10540.7,
      "interference_cores": 0.02,
      "host_busy_before": 1.46,
      "noisy": false
    },
    {
      "app": "fastapi",
      "scenario": "get",
      "connections": 16,
      "rps": 8466.5,
      "requests": 84663,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 1.884,
        "p50": 1.724,
        "p90": 2.691,
        "p99": 3.851,
        "p999": 5.812,
        "max": 26.974
      },
      "status_codes": {
        "200": 84663
      },
      "errors": {},
      "cpu_cores": 2.0,
      "mem_peak_mb": 83.2,
      "rps_per_core": 4232.4,
      "interference_cores": 0.07,
      "host_busy_before": 0.78,
      "noisy": false
    },
    {
      "app": "fastapi",
      "scenario": "get",
      "connections": 64,
      "rps": 10697.5,
      "requests": 106940,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 5.972,
        "p50": 5.759,
        "p90": 7.237,
        "p99": 9.235,
        "p999": 16.609,
        "max": 33.266
      },
      "status_codes": {
        "200": 106940
      },
      "errors": {},
      "cpu_cores": 2.0,
      "mem_peak_mb": 84.4,
      "rps_per_core": 5355.5,
      "interference_cores": 0.02,
      "host_busy_before": 1.13,
      "noisy": false
    },
    {
      "app": "fastapi",
      "scenario": "get",
      "connections": 256,
      "rps": 9134.6,
      "requests": 91128,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 27.928,
        "p50": 26.801,
        "p90": 34.902,
        "p99": 52.236,
        "p999": 81.485,
        "max": 130.888
      },
      "status_codes": {
        "200": 91128
      },
      "errors": {},
      "cpu_cores": 1.99,
      "mem_peak_mb": 88.1,
      "rps_per_core": 4581.8,
      "interference_cores": 0.02,
      "host_busy_before": 0.23,
      "noisy": false
    },
    {
      "app": "fastapi",
      "scenario": "list",
      "connections": 16,
      "rps": 4516.6,
      "requests": 45159,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 3.534,
        "p50": 3.48,
        "p90": 4.179,
        "p99": 5.066,
        "p999": 7.189,
        "max": 46.962
      },
      "status_codes": {
        "200": 45159
      },
      "errors": {},
      "cpu_cores": 2.0,
      "mem_peak_mb": 88.0,
      "rps_per_core": 2258.5,
      "interference_cores": 0.01,
      "host_busy_before": 0.68,
      "noisy": false
    },
    {
      "app": "fastapi",
      "scenario": "list",
      "connections": 64,
      "rps": 5758.8,
      "requests": 57537,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 11.095,
        "p50": 10.818,
        "p90": 13.64,
        "p99": 18.62,
        "p999": 27.124,
        "max": 71.355
      },
      "status_codes": {
        "200": 57537
      },
      "errors": {},
      "cpu_cores": 2.0,
      "mem_peak_mb": 88.7,
      "rps_per_core": 2884.1,
      "interference_cores": 0.01,
      "host_busy_before": 0.83,
      "noisy": false
    },
    {
      "app": "fastapi",
      "scenario": "list",
      "connections": 256,
      "rps": 4751.2,
      "requests": 47273,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 53.891,
        "p50": 53.583,
        "p90": 62.934,
        "p99": 101.712,
        "p999": 134.389,
        "max": 221.145
      },
      "status_codes": {
        "200": 47273
      },
      "errors": {},
      "cpu_cores": 2.0,
      "mem_peak_mb": 89.9,
      "rps_per_core": 2375.7,
      "interference_cores": 0.01,
      "host_busy_before": 0.31,
      "noisy": false
    },
    {
      "app": "fastapi",
      "scenario": "create",
      "connections": 16,
      "rps": 6586.6,
      "requests": 65863,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 2.424,
        "p50": 2.149,
        "p90": 3.435,
        "p99": 4.683,
        "p999": 8.435,
        "max": 59.16
      },
      "status_codes": {
        "200": 65863
      },
      "errors": {},
      "cpu_cores": 1.38,
      "mem_peak_mb": 90.0,
      "rps_per_core": 4770.0,
      "interference_cores": 0.01,
      "host_busy_before": 0.32,
      "noisy": false
    },
    {
      "app": "fastapi",
      "scenario": "create",
      "connections": 64,
      "rps": 8080.9,
      "requests": 80769,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 7.91,
        "p50": 6.703,
        "p90": 10.39,
        "p99": 41.937,
        "p999": 74.376,
        "max": 84.049
      },
      "status_codes": {
        "200": 80769
      },
      "errors": {},
      "cpu_cores": 1.67,
      "mem_peak_mb": 89.4,
      "rps_per_core": 4851.2,
      "interference_cores": 0.05,
      "host_busy_before": 0.27,
      "noisy": false
    },
    {
      "app": "fastapi",
      "scenario": "create",
      "connections": 256,
      "rps": 8167.5,
      "requests": 81457,
      "success_rate": 1.0,
      "latency_ms": {
        "avg": 31.275,
        "p50": 29.463,
        "p90": 43.733,
        "p99": 74.523,
        "p999": 102.416,
        "max": 183.623
      },
      "status_codes": {
        "200": 81457
      },
      "errors": {},
      "cpu_cores": 1.91,
      "mem_peak_mb": 90.0,
      "rps_per_core": 4271.4,
      "interference_cores": 0.03,
      "host_busy_before": 0.35,
      "noisy": false
    }
  ]
};
