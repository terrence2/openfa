FROM rust:1.33

COPY . .

RUN apt-get update && apt-get install -y cmake
RUN cargo install cross

RUN cross build --release --target x86_64-pc-windows-gnu -p pedump -p picdump -p picpack -p unlib

CMD ["cargo", "help"]
