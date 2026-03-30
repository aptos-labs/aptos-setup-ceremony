FROM archlinux:latest

CMD pacman -Syy & \
    pacman -S rustup

COPY . .

CMD cargo build



