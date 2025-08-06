# docker build -t arkvote:latest .
FROM docker.io/rust:1.86-slim
ARG MOLD_VERSION=2.40.3

WORKDIR /usr/src/ark-vote

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    wget \
    gcc \
    clang \
    openssl \
    libssl-dev \
    pkg-config \
    libc6-dev && \
    rm -rf /var/lib/apt/lists/*

RUN set -x; \
    wget -qO- "https://github.com/rui314/mold/releases/download/v${MOLD_VERSION}/mold-${MOLD_VERSION}-$(uname -m)-linux.tar.gz" | \
    tar -C /usr/local --strip-components=1 -xzf -

COPY . /usr/src/ark-vote

ENV RUSTFLAGS="-C linker=clang -C link-arg=-fuse-ld=/usr/local/bin/mold"
RUN cargo build --release --bin ark-vote

RUN mkdir -p /app/config /app/logs /app/static /app/templates && \
    cp target/release/ark-vote /app/ && \
    cp -r static templates /app/

WORKDIR /app

EXPOSE 3000

CMD ["./ark-vote", "web-server"]