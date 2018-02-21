# http agent for warp-poke

[![Join the chat at https://gitter.im/warp-poke/Lobby](https://badges.gitter.im/Join%20Chat.svg)](https://gitter.im/warp-poke/Lobby?utm_source=badge&utm_medium=badge&utm_campaign=pr-badge&utm_content=badge)
[![Build Status](https://travis-ci.org/warp-poke/http-rust-agent.svg?branch=master)](https://travis-ci.org/warp-poke/http-rust-agent)

This agent is a kafka `consumer` that read data from kafka and _format/add other_ data to forward them to the `warp10` cluster.

```
+--------+          +-------------+          +----- ----+
|        |          | http agent  |          |          |
| kafka  +--------->+ warp-poke   +--------->+  warp10  |
|        |          | (rust)      |          |          |
+--------+          +-------------+          +----------+
```

# Requirements

You need to install this packages:

- libssl-dev
- libsasl2-dev
- [librdkafka](https://github.com/edenhill/librdkafka) v0.11.3

NOTE: If you distribution don't provide a `librdkafka` package, follow this [instructions](https://github.com/edenhill/librdkafka#building) to recompile and install librdkafka.

# Build

`cargo build` (add ̀`--release` for production).

# Run it

You can configure the `poke-agent` by environment variables or by a `toml` configuration file. (See config.toml).

Possible environment variables are:

- BROKER
- TOPIC
- CONSUMER_GROUP
- USERNAME
- PASSWORD
- HOST
- ZONE

## Production

`RUSTLOG=info poke-agent daemon -c <path of the config file>`

## Development

`RUSTLOG=info ./target/debug/poke-agent once -u <warp10_url> -t <warp10_token>`

If you need to seed your kafka, you can do it with:

`RUSTLOG=info ./target/debug/poke-agent send-kafka DOMAIN_NAME -b <broker> -t <topic> --warp10-url <warp10-url> --warp10-token <warp10-token> -u <url>`