# perf + Brendan Gregg's FlameGraph scripts, run next to a bench container
# (--pid=container:...) with CAP_PERFMON, so the host's perf_event_paranoid
# setting never has to change.
FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends linux-perf perl git ca-certificates \
 && git clone --depth 1 https://github.com/brendangregg/FlameGraph /fg \
 && ln -s /usr/bin/perf /usr/local/bin/perf-bin \
 && rm -rf /var/lib/apt/lists/*
