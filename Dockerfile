# syntax=docker/dockerfile:1
# Image de production carbon-fr (ADR-0007). Multi-stage : build statique-ish via
# rustls (pas d'OpenSSL système) puis image runtime minimale.

# ─── Build ───────────────────────────────────────────────────────────────────
FROM rust:1-bookworm AS build
WORKDIR /app

# Cache des dépendances : on copie d'abord les manifestes.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY bin ./bin

# Build du seul binaire serveur, en release (profil optimisé du workspace).
RUN cargo build --release --locked -p server --bin carbonfr-server

# ─── Runtime ─────────────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime
# Certificats TLS (clients sortants ODRÉ/Open-Meteo/ENTSO-E/webhooks via rustls).
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Utilisateur non-root.
RUN useradd --system --no-create-home --uid 10001 carbonfr
USER carbonfr

COPY --from=build /app/target/release/carbonfr-server /usr/local/bin/carbonfr-server

EXPOSE 8080
# Les migrations sont appliquées au démarrage ; DATABASE_URL est requis.
# Mettre l'API derrière un reverse proxy TLS qui pose X-Forwarded-For
# (cf. deploy/Caddyfile) et activer CARBONFR_TRUST_PROXY=1.
ENTRYPOINT ["carbonfr-server"]
