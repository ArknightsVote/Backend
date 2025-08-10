# docker build -t arkvote:latest .
# ====== Planner Stage ======
FROM rust:1.88-slim AS chef
ARG MOLD_VERSION=2.40.3

WORKDIR /usr/src/ark-vote

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    ca-certificates curl wget gcc clang \
    libssl-dev pkg-config libc6-dev && \
    rm -rf /var/lib/apt/lists/*

RUN wget -qO- "https://github.com/rui314/mold/releases/download/v${MOLD_VERSION}/mold-${MOLD_VERSION}-$(uname -m)-linux.tar.gz" \
    | tar -C /usr/local --strip-components=1 -xzf -

RUN cargo install cargo-chef

COPY . .

RUN cargo chef prepare --recipe-path recipe.json


# ====== Builder Stage ======
FROM chef AS builder

COPY --from=chef /usr/src/ark-vote/recipe.json recipe.json

ENV RUSTFLAGS="-C linker=clang -C link-arg=-fuse-ld=/usr/local/bin/mold"
RUN cargo chef cook --release --recipe-path recipe.json

COPY . .
RUN cargo build --release --bin ark-vote


# ====== Runtime Stage ======
FROM gcr.io/distroless/cc-debian12

WORKDIR /app

COPY --from=builder /usr/src/ark-vote/target/release/ark-vote .
COPY --from=builder /usr/src/ark-vote/static ./static
COPY --from=builder /usr/src/ark-vote/templates ./templates

VOLUME ["/app/config", "/app/logs"]

EXPOSE 3000

ENTRYPOINT  ["/app/ark-vote"]
