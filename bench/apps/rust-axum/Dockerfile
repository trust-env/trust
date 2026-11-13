FROM rust:1.96-slim-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=build /src/target/release/rust-axum-bench /app
EXPOSE 8080
CMD ["/app"]
