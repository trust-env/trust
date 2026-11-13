# Built from the workspace `trust app.tr --vendor` generates (bench/bench.py
# does it into bench/.build/trust), so no SSH access to private crates is needed.
FROM rust:1.96-slim-bookworm AS build
WORKDIR /src
COPY out/ .
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=build /src/target/release/main /app
EXPOSE 8080
CMD ["/app"]
