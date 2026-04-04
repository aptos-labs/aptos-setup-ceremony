FROM archlinux:latest AS build_server

RUN pacman -Syy && pacman -S rustup gcc make lld pkg-config --noconfirm

COPY . .

RUN --mount=type=cache,target=/root/.cargo/registry \
    --mount=type=cache,target=/root/.cargo/git \
    --mount=type=cache,target=/target \
    cargo build -p server --release && \
    cp ./target/release/server ./server/

FROM archlinux:latest

COPY --link --from=build_server ./server/server ./server
COPY --link --from=build_server ./config.toml ./config.toml

CMD ["./server"]

