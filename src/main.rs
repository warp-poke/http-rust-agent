extern crate structopt;
#[macro_use]
extern crate structopt_derive;
extern crate reqwest;
extern crate time;
extern crate rand;
#[macro_use]
extern crate serde_derive;
extern crate futures;
extern crate tokio_core;
extern crate lapin_futures as lapin;
extern crate uuid;

extern crate serde;
extern crate serde_json;

use futures::Stream;
use futures::future::Future;
use tokio_core::reactor::Core;
use tokio_core::net::TcpStream;
use lapin::client::ConnectionOptions;
use lapin::channel::{BasicConsumeOptions, ExchangeDeclareOptions, QueueBindOptions, QueueDeclareOptions};
use lapin::types::FieldTable;
use lapin::channel::Channel;

use rand::{thread_rng, Rng};
use std::collections::HashMap;
use structopt::StructOpt;

use reqwest::{Client, Result};
use reqwest::header::ContentLength;
use time::{Duration, SteadyTime, PreciseTime};
use uuid::Uuid;
use std::error::Error;

use std::fmt;
use std::sync::{Arc, Mutex};

use std::sync::mpsc::{channel, Sender, Receiver};
use std::thread;

#[derive(StructOpt, PartialEq, Debug, Clone)]
#[structopt(name = "poke-agent", about = "HTTP poke agent")]
struct Opt {
    #[structopt(short = "d", long = "debug", help = "Activate debug mode")]
    debug: bool,

    #[structopt(short = "v", long = "verbose", help = "Activate verbose mode")]
    verbose: bool,
    #[structopt(subcommand)]
    cmd: Cmd,
}

#[derive(StructOpt, PartialEq, Debug, Clone)]
enum Cmd {
    #[structopt(name = "once")]
    Once {
        /// Needed parameter, the first on the command line.
        #[structopt(help = "domaine name")]
        domain_name: String,
    },

    #[structopt(name = "daemon")]
    Daemon {
        #[structopt(short = "s", long = "buffer_in_seconds", parse(try_from_str), default_value = "10",
                    help = "Time in seconds, for buffer to send data in warp10")]
        buffer_in_seconds: u64,
        /// Needed parameter, the first on the command line.
        #[structopt(help = "url of the nats server")]
        // TODO manage NATS cluster (multiples url)
        rabbitmq_url: String,
    },
}

pub const ANIMALS: &'static [&'static str] = &[
    "🐶",
    "🐱",
    "🐭",
    "🐹",
    "🦊",
    "🐻",
    "🐼",
    "🐨",
    "🐯",
    "🦁",
    "🐮",
    "🐷",
    "🐸",
    "🐒",
    "🦆",
    "🦉",
    "🦀",
    "🐡",
    "🦑",
    "🐙",
    "🦎",
    "🐿",
    "🐕",
    "🐁",
    "🐝",
    "🐞",
    "🦋",
    "🦔",
    "🕊",
    "🦃",
    "🐩",
    "🦒",
    "🐓",
    "🐳",
    "🙈",
    "🐥",
];

#[derive(Debug)]
struct DomainTestResult {
    url: String,
    http_status: reqwest::StatusCode,
    answer_time: Duration,
    content_length: u64,
}

#[derive(Serialize, Deserialize, Debug)]
struct Checks {
    latency: CheckCreds,
    status: CheckCreds,
}

#[derive(Serialize, Deserialize, Debug)]
struct CheckCreds {
    class_name: String,
    labels: Option<HashMap<String, String>>,
}


#[derive(Serialize, Deserialize, Debug)]
struct RequestBenchEvent {
    labels: HashMap<String, String>,
    url: String,
    checks: Checks,
}


struct BufferedDomainTestResult {
    domain_test_results: Result<DomainTestResult>,
    timestamp: time::PreciseTime,
    delivery_tag: u64,
}



fn run_check_for_url(url: &str, args: &Opt) -> Result<DomainTestResult> {
    let client = Client::new();
    let start = SteadyTime::now();
    let res = client.get(url).send()?;
    let dur = SteadyTime::now() - start;

    //  build infos
    let dtr = DomainTestResult {
        url: url.to_owned(),
        http_status: res.status(),
        answer_time: dur,
        content_length: res.headers()
            .get::<ContentLength>()
            .cloned()
            .map(|ct| match ct {
                ContentLength(u) => u,
            })
            .unwrap_or(0u64),
    };

    if args.verbose {
        let mut rng = thread_rng();
        let animal = rng.choose(ANIMALS).unwrap();

        println!("{}  - {} ------", animal, url);
        println!("{}  --- Status: {}", animal, res.status());
        println!("{}  --- Headers:", animal);
        for h in res.headers().iter() {
            println!("{}  ----- {}: {:?}", animal, h.name(), h.value_string());
        }
        println!("{}  --- Duration: {}", animal, dur);

    }
    Ok(dtr)
}

type ChecksResult = Result<(Result<DomainTestResult>, Result<DomainTestResult>)>;
fn run(domain_name: &str, args: Opt) -> ChecksResult {
    let http = run_check_for_url(format!("http://{}", domain_name).as_str(), &args);
    let https = run_check_for_url(format!("https://{}", domain_name).as_str(), &args);

    Ok((http, https))
}

// arg is a list of pairs (timestamp, result)
fn warp10_post(data: &[(u64, ChecksResult)]) -> std::result::Result<(), Box<Error>> {
    unimplemented!()
}


fn daemonify(rabbitmq_url: String, buffer_in_seconds: u64, cloned_args: Opt) {
    println!(" 🐇  Connect to rabbitMQ server using 🐰:");

    let (sender, receiver): (Sender<BufferedDomainTestResult>, Receiver<BufferedDomainTestResult>) = channel();
    let (sender_ack, receiver_ack): (Sender<u64>, Receiver<u64>) = channel();

    let re_cloned_args = cloned_args.clone();
    thread::spawn(move || loop {
        thread::sleep(std::time::Duration::from_secs(buffer_in_seconds));
        if re_cloned_args.debug {
            println!(" ⏰  loop tick every {}s", buffer_in_seconds);
        }
        let mut iter = receiver.try_iter();

        for x in iter {
            println!("{:?}", x.domain_test_results);
            sender_ack.send(x.delivery_tag);
        }
    });

    // create the reactor
    let mut core = Core::new().unwrap();
    let handle = core.handle();
    let addr = rabbitmq_url.parse().unwrap();

    let queue_name = "http-agent-queue"; //format!("http-agent-{}", Uuid::new_v4());
    let exchange_name = "checks.http";
    let consumer_id = format!("http-rust-agent-{}", Uuid::new_v4());

    core.run(
        TcpStream::connect(&addr, &handle)
            .and_then(|stream| {
                println!(" 🐇  TCP..................................✅");

                lapin::client::Client::connect(stream, &ConnectionOptions::default())
            })
            .and_then(|(client, heartbeat_future_fn)| {
                println!(" 🐇  Rabbit Client........................✅");


                let heartbeat_client = client.clone();
                handle.spawn(heartbeat_future_fn(&heartbeat_client).map_err(|_| ()));

                client.create_channel()
            })
            .and_then(|channel| {
                let id = channel.id;
                println!(" 🐇  Channel Created, id is {:.<13}.✅", id);


                let ch = channel.clone();
                let qdod = &QueueDeclareOptions::default();
                let qdo = QueueDeclareOptions {
                    ticket: qdod.ticket,
                    passive: qdod.exclusive,
                    durable: qdod.exclusive,
                    exclusive: qdod.exclusive,
                    auto_delete: true,
                    nowait: qdod.nowait,
                };
                channel
                    .queue_declare(queue_name, &qdo, &FieldTable::new())
                    .and_then(move |_| {
                        println!(" 🐇  Channel {} declared queue {}", id, queue_name);

                        channel
                            .exchange_declare(
                                exchange_name,
                                "direct",
                                &ExchangeDeclareOptions::default(),
                                &FieldTable::new(),
                            )
                            .and_then(move |_| {
                                println!(" 🐇  Exchange {} declared", exchange_name);
                                channel
                                    .queue_bind(
                                        queue_name,
                                        exchange_name,
                                        "",
                                        &QueueBindOptions::default(),
                                        &FieldTable::new(),
                                    )
                                    .and_then(move |_| {
                                        println!(" 🐇  Queue {} bind to {}", queue_name, exchange_name);

                                        let bcod = &BasicConsumeOptions::default();
                                        let bco = BasicConsumeOptions {
                                            ticket: bcod.ticket,
                                            no_local: bcod.no_local,
                                            no_ack: false,
                                            exclusive: bcod.exclusive,
                                            no_wait: bcod.no_wait,
                                        };
                                        channel
                                            .basic_consume(queue_name, consumer_id.as_str(), &bco, &FieldTable::new())
                                            .and_then(|stream| {
                                                println!(" 🐇  got consumer stream, ready.");

                                                let re_cloned_args = cloned_args.clone();

                                                let shared_channel = Arc::new(Mutex::new(channel));
                                                thread::spawn(move || {
                                                    loop {
                                                        thread::sleep(std::time::Duration::from_secs(1));
                                                        let mut iter = receiver_ack.try_iter();
                                                        let mut shared_channel = shared_channel.try_lock();

                                                        if let Ok(ref mut shared_channel) = shared_channel {
                                                            for x in iter {
                                                                if re_cloned_args.debug {
                                                                    println!(" 🐇  👌  ACK for message id {:?}", x);
                                                                }
                                                                shared_channel.basic_ack(x);
                                                            }
                                                        } else {
                                                            println!("try_lock failed");
                                                        }



                                                    }
                                                });



                                                stream.for_each(move |message| {
                                                    if cloned_args.debug {
                                                        println!(" 🐇  got message: {:?}", message);
                                                    }
                                                    let deserialized: RequestBenchEvent = serde_json::from_slice(&message.data).unwrap();
                                                    if cloned_args.verbose {
                                                        println!(
                                                            " 🐇  deserialized message get from rabbitmq: {:?}",
                                                            deserialized
                                                        );
                                                    }
                                                    let res = run_check_for_url(deserialized.url.as_str(), &cloned_args);
                                                    sender.send(BufferedDomainTestResult {
                                                        domain_test_results: res,
                                                        timestamp: PreciseTime::now(),
                                                        delivery_tag: message.delivery_tag,
                                                    });
                                                    //
/*
                                                    let mut iter = receiver_ack.try_iter();

                                                    for x in iter {
                                                        if re_cloned_args.debug {
                                                            println!(" 🐇  👌  ACK for message id {:?}", x);
                                                        }
                                                        ch.basic_ack(x);
                                                    }
                                                    */
                                                    Ok(())
                                                })
                                            })
                                    })
                            })
                    })
            }),
    ).unwrap();

}

fn main() {
    let args = Opt::from_args();

    if args.debug {
        println!("CLI arguments parsing : {:#?}", args);
    }

    let cloned_args = args.clone();

    match args.cmd {
        Cmd::Once { domain_name } => {
            let rr = run(domain_name.as_str(), cloned_args);

            println!("{:#?}", rr);
        }
        Cmd::Daemon {
            buffer_in_seconds,
            rabbitmq_url,
        } => daemonify(rabbitmq_url, buffer_in_seconds, cloned_args),
    }

}
