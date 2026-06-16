# syntax=docker/dockerfile:1
# Image de production carbon-fr (ADR-0007). Multi-stage : build via rustls (pas
# d'OpenSSL système) puis image runtime minimale, non-root.

# ─── Build ───────────────────────────────────────────────────────────────────
FROM rust:1-bookworm AS build
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY bin ./bin

# Cache mounts BuildKit : le registre Cargo et `target/` sont **persistés entre
# builds** (recompilation incrémentale, deps non re-téléchargées). `target/` étant
# un cache (hors couche d'image), on extrait le binaire dans la même étape `RUN`.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --locked -p server --bin carbonfr-server \
    && cp /app/target/release/carbonfr-server /carbonfr-server

# ─── Runtime ─────────────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime
# Certificats TLS (clients sortants ODRÉ/Open-Meteo/ENTSO-E/webhooks via rustls).
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Utilisateur non-root.
RUN useradd --system --no-create-home --uid 10001 carbonfr
USER carbonfr

COPY --from=build /carbonfr-server /usr/local/bin/carbonfr-server

EXPOSE 8080
# Les migrations sont appliquées au démarrage ; DATABASE_URL est requis.
# Mettre l'API derrière un reverse proxy TLS qui pose X-Forwarded-For
# (cf. deploy/) et activer CARBONFR_TRUST_PROXY=1.
#
# Reproductibilité : `--locked` fige les deps Cargo. Pour figer aussi la toolchain
# et l'OS, épingler les images de base par digest (`rust:1-bookworm@sha256:…`,
# `debian:bookworm-slim@sha256:…`) — au prix d'un bump manuel pour les CVE de base.
ENTRYPOINT ["carbonfr-server"]
