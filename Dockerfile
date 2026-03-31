FROM archlinux:latest

RUN pacman -Syy && pacman -S rustup gcc make lld pkg-config --noconfirm

COPY . .

RUN cargo build -p server --release

CMD ["./target/release/server"]

