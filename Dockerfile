FROM rust:1.33

WORKDIR /build
COPY . .

RUN apt-get update && apt-get install -y cmake
RUN rustup component add clippy
RUN cargo install cross

RUN make release-windows

CMD ["make", "build"]
