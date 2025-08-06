# docker build -t arkvote:latest .
FROM docker.io/rust:1.86-slim

WORKDIR /usr/src/ark-vote

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    gcc \
    clang \
    openssl \
    libssl-dev \
    mold \
    pkg-config \
    libc6-dev && \
    rm -rf /var/lib/apt/lists/*

COPY . /usr/src/ark-vote

ENV RUSTFLAGS="-C linker=clang -C link-arg=-fuse-ld=/usr/bin/mold"
RUN cargo build --release --bin ark-vote

RUN mkdir -p /app/config /app/logs /app/static /app/templates && \
    cp target/release/ark-vote /app/ && \
    cp -r static templates /app/

WORKDIR /app

EXPOSE 3000

CMD ["./ark-vote", "web-server"]