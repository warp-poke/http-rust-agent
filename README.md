# http agent for warp-poke

[![Join the chat at https://gitter.im/warp-poke/Lobby](https://badges.gitter.im/Join%20Chat.svg)](https://gitter.im/warp-poke/Lobby?utm_source=badge&utm_medium=badge&utm_campaign=pr-badge&utm_content=badge)
[![Build Status](https://travis-ci.org/warp-poke/http-rust-agent.svg?branch=master)](https://travis-ci.org/warp-poke/http-rust-agent)

This agent is a kafka `consumer` which reads data from kafka, checks response time and status for each service and sends that data to Warp10.

```
+--------+          +-------------+          +----- ----+
|        |          | http agent  |          |          |
| kafka  +--------->+ warp-poke   +--------->+  warp10  |
|        |          | (rust)      |          |          |
+--------+          +-------------+          +----------+
```

# Requirements

You need to install these packages:

- libssl-dev
- libsasl2-dev
- [librdkafka](https://github.com/edenhill/librdkafka) v0.11.3

NOTE: If your distribution doesn't provide a `librdkafka` package, follow these [instructions](https://github.com/edenhill/librdkafka#building) to compile and install librdkafka.

# Build

`cargo build` (add ̀`--release` for production).

# Run it

You can configure this agent with environment variables or with a `toml` configuration file. (See config.toml).

A default config.toml file is provided, you should update the relevant fields.

If using a Kafka with SASL auth, set `username` and `password`. It will
automatically set the `PLAIN` mechanism for SASL and `SASL_SSL` for the
security protocol.

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
