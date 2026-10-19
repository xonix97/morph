FROM rust:1.99.0-bookworm

ENV CARGO_NET_OFFLINE=true \
    RUSTUP_AUTO_INSTALL=0 \
    PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1

RUN apt-get update \
    && apt-get install --no-install-recommends --yes \
        build-essential \
        ca-certificates \
        make \
        python3 \
        python3.11 \
        python3.11-venv \
        python-is-python3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .

RUN cargo build --release --locked --offline
RUN make test

CMD ["/app/target/release/morph", "demo", "all"]
