FROM alpine as builder

COPY src /source/src/
COPY Cargo.toml /source/
COPY Cargo.lock /source/
#COPY config.toml /src/

RUN apk add --no-cache --virtual .build-dependencies \
  cargo \
  build-base \
  file \
  libgcc \
  musl-dev \
  rust \
  librdkafka-dev
RUN apk add --no-cache librdkafka  openssl-dev
WORKDIR /source/
RUN cargo build --release

FROM alpine
COPY config.toml /etc/poke-agent/config.toml
RUN apk add --no-cache openssl llvm-libunwind libgcc librdkafka
COPY --from=builder /source/target/release/poke-agent /usr/bin/poke-agent
CMD ["/usr/bin/poke-agent", "--help"]
