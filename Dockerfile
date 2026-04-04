FROM archlinux:latest as build_server

RUN pacman -Syy && pacman -S rustup gcc make lld pkg-config --noconfirm

COPY . .

RUN cargo build -p server --release

FROM archlinux:latest

COPY --link --from=build_server ./target/release/server ./server

CMD ["./server"]

