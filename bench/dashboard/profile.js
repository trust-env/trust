window.BENCH_PROFILE = {
  "meta": {
    "date": "2026-11-13T16:13:22+0100",
    "connections": 64,
    "duration_s": 10,
    "frequency_hz": 1999,
    "apps": [
      "trust",
      "rust-axum"
    ],
    "components": [
      {
        "id": "kernel",
        "label": "Noyau (r\u00e9seau, syscalls)"
      },
      {
        "id": "tracing",
        "label": "Tracing / logs"
      },
      {
        "id": "alloc",
        "label": "Allocation m\u00e9moire"
      },
      {
        "id": "json",
        "label": "JSON (serde)"
      },
      {
        "id": "sqlx",
        "label": "Driver Postgres (sqlx)"
      },
      {
        "id": "http",
        "label": "HTTP (hyper)"
      },
      {
        "id": "router",
        "label": "Routage (axum / tower)"
      },
      {
        "id": "runtime",
        "label": "Runtime async (tokio)"
      },
      {
        "id": "app",
        "label": "Code de l'app"
      },
      {
        "id": "libc",
        "label": "libc (m\u00e9moire, cha\u00eenes)"
      },
      {
        "id": "other",
        "label": "Autre"
      }
    ],
    "scenarios": [
      {
        "id": "ping",
        "label": "GET /ping"
      },
      {
        "id": "get",
        "label": "GET /users/:id"
      },
      {
        "id": "list",
        "label": "GET /users?limit=20"
      },
      {
        "id": "create",
        "label": "POST /users"
      }
    ]
  },
  "profiles": {
    "trust-ping": {
      "samples": 62635908026,
      "self_pct": {
        "kernel": 62.89,
        "libc": 5.56,
        "other": 2.44,
        "runtime": 4.26,
        "router": 5.51,
        "http": 16.03,
        "alloc": 3.07,
        "tracing": 0.1,
        "json": 0.13
      },
      "tracing_cost_pct": 0.32,
      "top_functions": [
        {
          "name": "[[nf_tables]]_[k]",
          "pct": 5.58
        },
        {
          "name": "[libc.so.6]",
          "pct": 5.56
        },
        {
          "name": "hyper::proto::h1::dispatch::Dispatcher<D,Bs,I,T>::poll_write",
          "pct": 2.2
        },
        {
          "name": "__raw_spin_lock_irqsave_[k]",
          "pct": 1.94
        },
        {
          "name": "malloc",
          "pct": 1.74
        },
        {
          "name": "skb_defer_free_flush_[k]",
          "pct": 1.43
        },
        {
          "name": "<hyper_util::server::conn::auto::UpgradeableConnection<I,S,E> as core::future::future::Future>::poll",
          "pct": 1.37
        },
        {
          "name": "call_function_single_prep_ipi_[k]",
          "pct": 1.35
        },
        {
          "name": "cfree",
          "pct": 1.32
        },
        {
          "name": "<hyper::proto::h1::role::Server as hyper::proto::h1::Http1Transaction>::parse",
          "pct": 1.29
        },
        {
          "name": "[[vdso]]_[k]",
          "pct": 1.26
        },
        {
          "name": "hyper::proto::h1::conn::Conn<I,B,T>::write_head",
          "pct": 1.17
        }
      ],
      "rps": 237027.8,
      "netfilter_pct": 7.2
    },
    "trust-get": {
      "samples": 55340678716,
      "self_pct": {
        "libc": 9.05,
        "other": 2.6,
        "kernel": 59.94,
        "runtime": 7.64,
        "router": 3.56,
        "sqlx": 8.01,
        "http": 6.41,
        "alloc": 2.19,
        "tracing": 0.09,
        "json": 0.51
      },
      "tracing_cost_pct": 0.12,
      "top_functions": [
        {
          "name": "[libc.so.6]",
          "pct": 9.05
        },
        {
          "name": "[[nf_tables]]_[k]",
          "pct": 4.76
        },
        {
          "name": "__raw_spin_lock_irqsave_[k]",
          "pct": 1.95
        },
        {
          "name": "tokio::runtime::task::raw::poll",
          "pct": 1.62
        },
        {
          "name": "malloc",
          "pct": 1.25
        },
        {
          "name": "skb_defer_free_flush_[k]",
          "pct": 1.02
        },
        {
          "name": "skb_release_data_[k]",
          "pct": 0.99
        },
        {
          "name": "fdget_[k]",
          "pct": 0.99
        },
        {
          "name": "hyper::proto::h1::dispatch::Dispatcher<D,Bs,I,T>::poll_write",
          "pct": 0.95
        },
        {
          "name": "entry_SYSCALL_64_[k]",
          "pct": 0.87
        },
        {
          "name": "cfree",
          "pct": 0.87
        },
        {
          "name": "sqlx_core::pool::executor::<impl sqlx_core::executor::Executor for &sqlx_core::pool::Pool<DB>>::fetch_optional::{{closure}}",
          "pct": 0.87
        }
      ],
      "rps": 43216.5,
      "netfilter_pct": 6.14
    },
    "trust-list": {
      "samples": 52683140661,
      "self_pct": {
        "other": 3.81,
        "libc": 13.73,
        "runtime": 9.43,
        "kernel": 43.64,
        "router": 3.3,
        "sqlx": 13.44,
        "http": 4.98,
        "alloc": 4.09,
        "tracing": 0.11,
        "json": 3.47
      },
      "tracing_cost_pct": 0.16,
      "top_functions": [
        {
          "name": "[libc.so.6]",
          "pct": 13.73
        },
        {
          "name": "<tracing::instrument::Instrumented<T> as core::future::future::Future>::poll",
          "pct": 3.44
        },
        {
          "name": "[[nf_tables]]_[k]",
          "pct": 3.29
        },
        {
          "name": "<&mut serde_json::ser::Serializer<W,F> as serde_core::ser::Serializer>::serialize_str",
          "pct": 2.21
        },
        {
          "name": "malloc",
          "pct": 2.16
        },
        {
          "name": "sqlx_core::ext::async_stream::Yielder<T>::yield::{{closure}}",
          "pct": 2.16
        },
        {
          "name": "<sqlx_core::ext::async_stream::TryAsyncStream<T> as futures_core::stream::Stream>::poll_next",
          "pct": 2.14
        },
        {
          "name": "sqlx_core::row::Row::try_get",
          "pct": 1.86
        },
        {
          "name": "tokio::runtime::task::raw::poll",
          "pct": 1.26
        },
        {
          "name": "<F as axum::handler::Handler<",
          "pct": 1.26
        },
        {
          "name": "cfree",
          "pct": 1.19
        },
        {
          "name": "__raw_spin_lock_irqsave_[k]",
          "pct": 1.14
        }
      ],
      "rps": 23005.4,
      "netfilter_pct": 4.09
    },
    "trust-create": {
      "samples": 10470190360,
      "self_pct": {
        "libc": 10.4,
        "other": 3.04,
        "runtime": 8.34,
        "kernel": 55.61,
        "http": 6.55,
        "sqlx": 8.6,
        "router": 4.37,
        "alloc": 2.24,
        "json": 0.78,
        "tracing": 0.06
      },
      "tracing_cost_pct": 0.12,
      "top_functions": [
        {
          "name": "[libc.so.6]",
          "pct": 10.4
        },
        {
          "name": "[[nf_tables]]_[k]",
          "pct": 4.29
        },
        {
          "name": "tokio::runtime::task::raw::poll",
          "pct": 2.07
        },
        {
          "name": "__raw_spin_lock_irqsave_[k]",
          "pct": 1.72
        },
        {
          "name": "<F as axum::handler::Handler<",
          "pct": 1.34
        },
        {
          "name": "sqlx_core::pool::executor::<impl sqlx_core::executor::Executor for &sqlx_core::pool::Pool<DB>>::fetch_optional::{{closure}}",
          "pct": 1.28
        },
        {
          "name": "malloc",
          "pct": 1.25
        },
        {
          "name": "[[vdso]]_[k]",
          "pct": 1.03
        },
        {
          "name": "call_function_single_prep_ipi_[k]",
          "pct": 1.03
        },
        {
          "name": "hyper::proto::h1::dispatch::Dispatcher<D,Bs,I,T>::poll_write",
          "pct": 0.93
        },
        {
          "name": "fdget_[k]",
          "pct": 0.89
        },
        {
          "name": "skb_defer_free_flush_[k]",
          "pct": 0.87
        }
      ],
      "rps": 8243.9,
      "pg_waits_pct": {
        "LWLock:WALWrite": 93.8,
        "IO:WalSync": 3.1,
        "Client:ClientRead": 2.2,
        "CPU": 0.6,
        "IO:WalInitSync": 0.1,
        "LWLock:WALInsert": 0.1,
        "IO:WalWrite": 0.1
      },
      "netfilter_pct": 5.51
    },
    "rust-axum-ping": {
      "samples": 59702560059,
      "self_pct": {
        "libc": 5.05,
        "other": 1.93,
        "runtime": 4.4,
        "kernel": 65.46,
        "router": 3.28,
        "http": 17.04,
        "alloc": 2.67,
        "json": 0.16
      },
      "tracing_cost_pct": 0.0,
      "top_functions": [
        {
          "name": "[[nf_tables]]_[k]",
          "pct": 5.73
        },
        {
          "name": "[libc.so.6]",
          "pct": 5.05
        },
        {
          "name": "hyper::proto::h1::dispatch::Dispatcher<D,Bs,I,T>::poll_write",
          "pct": 2.67
        },
        {
          "name": "__raw_spin_lock_irqsave_[k]",
          "pct": 2.28
        },
        {
          "name": "skb_defer_free_flush_[k]",
          "pct": 1.68
        },
        {
          "name": "<hyper_util::server::conn::auto::UpgradeableConnection<I,S,E> as core::future::future::Future>::poll",
          "pct": 1.62
        },
        {
          "name": "<hyper::proto::h1::role::Server as hyper::proto::h1::Http1Transaction>::parse",
          "pct": 1.62
        },
        {
          "name": "malloc",
          "pct": 1.49
        },
        {
          "name": "call_function_single_prep_ipi_[k]",
          "pct": 1.36
        },
        {
          "name": "tokio::runtime::task::raw::poll",
          "pct": 1.28
        },
        {
          "name": "hyper::proto::h1::conn::Conn<I,B,T>::write_head",
          "pct": 1.22
        },
        {
          "name": "cfree",
          "pct": 1.19
        }
      ],
      "rps": 215826.4,
      "netfilter_pct": 7.39
    },
    "rust-axum-get": {
      "samples": 52481862425,
      "self_pct": {
        "libc": 7.48,
        "other": 2.56,
        "kernel": 61.88,
        "runtime": 7.49,
        "router": 2.52,
        "sqlx": 9.09,
        "http": 6.4,
        "alloc": 2.09,
        "json": 0.48,
        "tracing": 0.0
      },
      "tracing_cost_pct": 0.0,
      "top_functions": [
        {
          "name": "[libc.so.6]",
          "pct": 7.48
        },
        {
          "name": "[[nf_tables]]_[k]",
          "pct": 4.66
        },
        {
          "name": "__raw_spin_lock_irqsave_[k]",
          "pct": 2.05
        },
        {
          "name": "tokio::runtime::task::raw::poll",
          "pct": 1.61
        },
        {
          "name": "malloc",
          "pct": 1.25
        },
        {
          "name": "sqlx_core::pool::executor::<impl sqlx_core::executor::Executor for &sqlx_core::pool::Pool<DB>>::fetch_optional::{{closure}}",
          "pct": 1.16
        },
        {
          "name": "skb_defer_free_flush_[k]",
          "pct": 1.13
        },
        {
          "name": "hyper::proto::h1::dispatch::Dispatcher<D,Bs,I,T>::poll_write",
          "pct": 1.1
        },
        {
          "name": "call_function_single_prep_ipi_[k]",
          "pct": 1.04
        },
        {
          "name": "fdget_[k]",
          "pct": 1.0
        },
        {
          "name": "entry_SYSCALL_64_[k]",
          "pct": 1.0
        },
        {
          "name": "sqlx_postgres::connection::stream::PgStream::recv::{{closure}}",
          "pct": 0.99
        }
      ],
      "rps": 47216.7,
      "netfilter_pct": 6.13
    },
    "rust-axum-list": {
      "samples": 42185167223,
      "self_pct": {
        "libc": 11.23,
        "other": 3.31,
        "runtime": 9.38,
        "kernel": 45.8,
        "router": 2.87,
        "sqlx": 15.08,
        "http": 5.09,
        "alloc": 3.54,
        "json": 3.68,
        "tracing": 0.01
      },
      "tracing_cost_pct": 0.01,
      "top_functions": [
        {
          "name": "[libc.so.6]",
          "pct": 11.23
        },
        {
          "name": "<tracing::instrument::Instrumented<T> as core::future::future::Future>::poll",
          "pct": 3.93
        },
        {
          "name": "[[nf_tables]]_[k]",
          "pct": 3.33
        },
        {
          "name": "sqlx_core::ext::async_stream::Yielder<T>::yield::{{closure}}",
          "pct": 2.57
        },
        {
          "name": "sqlx_core::row::Row::try_get",
          "pct": 2.43
        },
        {
          "name": "<&mut serde_json::ser::Serializer<W,F> as serde_core::ser::Serializer>::serialize_str",
          "pct": 2.42
        },
        {
          "name": "<sqlx_core::ext::async_stream::TryAsyncStream<T> as futures_core::stream::Stream>::poll_next",
          "pct": 2.32
        },
        {
          "name": "malloc",
          "pct": 2.03
        },
        {
          "name": "<F as axum::handler::Handler<",
          "pct": 1.69
        },
        {
          "name": "__raw_spin_lock_irqsave_[k]",
          "pct": 1.48
        },
        {
          "name": "tokio::runtime::task::raw::poll",
          "pct": 1.13
        },
        {
          "name": "hashbrown::map::HashMap<K,V,S,A>::get",
          "pct": 1.0
        }
      ],
      "rps": 21946.6,
      "netfilter_pct": 4.29
    },
    "rust-axum-create": {
      "samples": 7767821361,
      "self_pct": {
        "libc": 8.04,
        "other": 2.72,
        "runtime": 7.38,
        "kernel": 60.0,
        "router": 3.16,
        "sqlx": 8.55,
        "http": 7.39,
        "alloc": 1.93,
        "json": 0.82
      },
      "tracing_cost_pct": 0.0,
      "top_functions": [
        {
          "name": "[libc.so.6]",
          "pct": 8.04
        },
        {
          "name": "[[nf_tables]]_[k]",
          "pct": 5.32
        },
        {
          "name": "__raw_spin_lock_irqsave_[k]",
          "pct": 1.74
        },
        {
          "name": "hyper::proto::h1::dispatch::Dispatcher<D,Bs,I,T>::poll_write",
          "pct": 1.52
        },
        {
          "name": "tokio::runtime::task::raw::poll",
          "pct": 1.46
        },
        {
          "name": "sqlx_core::pool::executor::<impl sqlx_core::executor::Executor for &sqlx_core::pool::Pool<DB>>::fetch_optional::{{closure}}",
          "pct": 1.39
        },
        {
          "name": "<hyper_util::server::conn::auto::UpgradeableConnection<I,S,E> as core::future::future::Future>::poll",
          "pct": 1.22
        },
        {
          "name": "malloc",
          "pct": 1.08
        },
        {
          "name": "tcp_rcv_established_[k]",
          "pct": 1.05
        },
        {
          "name": "skb_defer_free_flush_[k]",
          "pct": 1.05
        },
        {
          "name": "__inet_lookup_established_[k]",
          "pct": 1.01
        },
        {
          "name": "axum::serve::handle_connection::{{closure}}::{{closure}}",
          "pct": 0.95
        }
      ],
      "rps": 5844.8,
      "pg_waits_pct": {
        "LWLock:WALWrite": 94.6,
        "IO:WalSync": 3.4,
        "LWLock:BufferContent": 0.6,
        "CPU": 0.5,
        "Client:ClientRead": 0.5,
        "LWLock:WALInsert": 0.3,
        "IO:WalWrite": 0.0,
        "LWLock:LockManager": 0.0
      },
      "netfilter_pct": 6.45
    }
  }
};
