# --- STAGE 1: Builder ---
FROM rust:1.93-slim-bookworm AS builder

# Gerekli derleme araçlarını kur
RUN apt-get update && \
    apt-get install -y git pkg-config libssl-dev protobuf-compiler curl && \
    rm -rf /var/lib/apt/lists/*

# YENİ: Build argümanlarını tanımla
ARG GIT_COMMIT="unknown"
ARG BUILD_DATE="unknown"
ARG SERVICE_VERSION="0.0.0"

WORKDIR /app

COPY . .

# YENİ: Build-time environment değişkenlerini ayarla
ENV GIT_COMMIT=${GIT_COMMIT}
ENV BUILD_DATE=${BUILD_DATE}
ENV SERVICE_VERSION=${SERVICE_VERSION}

# Derlemeyi yap
RUN cargo build --release --bin sentiric-sip-registrar-service

# --- STAGE 2: Final (Minimal) Image ---
FROM debian:bookworm-slim

# --- Çalışma zamanı sistem bağımlılıkları ---
RUN apt-get update && apt-get install -y --no-install-recommends \
    netcat-openbsd \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# YENİ: Argümanları environment değişkenlerine ata
ARG GIT_COMMIT
ARG BUILD_DATE
ARG SERVICE_VERSION
ENV GIT_COMMIT=${GIT_COMMIT}
ENV BUILD_DATE=${BUILD_DATE}
ENV SERVICE_VERSION=${SERVICE_VERSION}

WORKDIR /app

# Executable adı Cargo.toml'daki name ile uyumlu olmalı
COPY --from=builder /app/target/release/sentiric-sip-registrar-service .

# Güvenlik için root olmayan bir kullanıcıyla çalıştır
RUN useradd -m -u 1001 appuser
USER appuser
ENTRYPOINT ["./sentiric-sip-registrar-service"]