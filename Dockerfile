# syntax=docker/dockerfile:1

# Builds the Plait server (target `server`, the default) and the demo-data
# seeder (target `seed`, development only). Use with compose.yaml:
#   docker compose up --build

FROM rust:1.97.0-slim-trixie AS build
# Use the image's toolchain as-is; rust-toolchain.toml would otherwise make
# rustup download rustfmt and clippy, which a release build doesn't need.
ENV RUSTUP_TOOLCHAIN=1.97.0
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p plait-server -p plait-devtools \
    && cp target/release/plait-server target/release/plait-seed /usr/local/bin/

FROM debian:trixie-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --no-create-home plait
USER plait

# Demo-data seeder. Kept out of the server image so it can't be run against
# a production database by accident.
FROM runtime AS seed
COPY --from=build /usr/local/bin/plait-seed /usr/local/bin/plait-seed
ENTRYPOINT ["plait-seed"]

# The server. Last, so it is the default target.
FROM runtime AS server
COPY --from=build /usr/local/bin/plait-server /usr/local/bin/plait-server
ENV PLAIT_BIND=0.0.0.0:8080
EXPOSE 8080
# The server shuts down gracefully on SIGINT.
STOPSIGNAL SIGINT
ENTRYPOINT ["plait-server"]
CMD ["serve"]
