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
extern crate lapin_async;
extern crate uuid;
extern crate warp10;

extern crate serde;
extern crate serde_json;

use futures::Future;
use futures::Stream;
use futures::sync::mpsc;
use futures::sync::mpsc::*;
use lapin::channel::{BasicConsumeOptions, ExchangeDeclareOptions, QueueBindOptions, QueueDeclareOptions};
use lapin::client::ConnectionOptions;
use lapin::types::FieldTable;
use tokio_core::net::TcpStream;
use tokio_core::reactor::Core;

use rand::{Rng, thread_rng};
use std::collections::HashMap;
use structopt::StructOpt;

use reqwest::{Client, Result};
use reqwest::header::ContentLength;
use std::error::Error;
use std::io::{self};
use time::{Duration, PreciseTime, SteadyTime};
use uuid::Uuid;
use std::convert::From;

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
        #[structopt(help = "domaine name")]
        domain_name: String,
    },

    #[structopt(name = "daemon")]
    Daemon {
        #[structopt(short = "s", long = "buffer_in_seconds", parse(try_from_str), default_value = "10", help = "Time in seconds, for buffer to send data in warp10")]
        buffer_in_seconds: u64,
        #[structopt(short = "w10url", long = "warp10-url", default_value = "http://localhost:8080/", help = "Url of the Warp10 datastore")]
        warp10_url: String,
        #[structopt(short = "w10tk", long = "warp10-token", help = "Token to write in the Warp10 datastore")]
        warp10_token: String,
        #[structopt(help = "url of the nats server")]
        // TODO manage clusterization
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

impl From<BufferedDomainTestResult> for Vec<T=warp10::Data> {
    fn from(item: BufferedDomainTestResult) -> Self {
        vec![
        warp10::Data::new(
            item.timestamp,
            None,
            item.request_bench_event.status.class_name,
            vec![
                warp10::Label::new("label 1 name", "label 1 value"),
                warp10::Label::new("label 2 name", "label 2 value")
            ],
            warp10::Value::int(item.http_status.as_u16)
        ),
        warp10::Data::new(
            item.timestamp,
            None,
            item.request_bench_event.latency.class_name,
            vec![
                warp10::Label::new("label 1 name", "label 1 value"),
                warp10::Label::new("label 2 name", "label 2 value")
            ],
            warp10::Value::Long(item.answer_time.num_milliseconds())
        ),
        ]
    }
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
    request_bench_event: RequestBenchEvent,
}

#[derive(Debug)]
enum MyStreamUnificationType {
    DeliveryTag { delivery_tag: u64 },
    AmqpMessage { message: lapin_async::queue::Message, },
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

    // TODO, real error management and make it a real usable data
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

    // create the reactor
    let mut core = Core::new().unwrap();
    let handle = core.handle();


    let addr = rabbitmq_url.parse().unwrap();

    let queue_name = "http-agent-queue"; //format!("http-agent-{}", Uuid::new_v4());
    let exchange_name = "checks.http";
    let consumer_id = format!("http-rust-agent-{}", Uuid::new_v4());

    core.run({

        let (sender, receiver): (std::sync::mpsc::Sender<BufferedDomainTestResult>, std::sync::mpsc::Receiver<BufferedDomainTestResult>) = std::sync::mpsc::channel();
        let (sender_ack, receiver_ack): (UnboundedSender<Result<MyStreamUnificationType>>, UnboundedReceiver<Result<MyStreamUnificationType>>) = mpsc::unbounded();

        let re_cloned_args = cloned_args.clone();
        thread::spawn(move || loop {
            thread::sleep(std::time::Duration::from_secs(buffer_in_seconds));
            if re_cloned_args.debug {
                println!(" ⏰  loop tick every {}s", buffer_in_seconds);
            }
            let iter = receiver.try_iter();

            for x in iter {
                println!(" 📠  {:?}", x.domain_test_results);
                // TODO warp10 send here
                sender_ack.unbounded_send(Ok(MyStreamUnificationType::DeliveryTag {
                    delivery_tag: x.delivery_tag,
                }));
            }

        });

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
                                                (stream.map(|x| Ok(MyStreamUnificationType::AmqpMessage { message: x })))
                                                    .select(receiver_ack.map_err( // no error coming here, we get the stream
                                                        |_| io::Error::new(io::ErrorKind::Other, "boom"),
                                                    ))
                                                    .for_each(move |item| {
                                                        if re_cloned_args.debug {
                                                            println!(" 🍼  get on the stream: {:?}", item);
                                                        }
                                                        match item {
                                                            Ok(MyStreamUnificationType::DeliveryTag { delivery_tag }) => {
                                                                if re_cloned_args.debug {
                                                                    println!(" 🐇  👌  ACK for message id {:?}", delivery_tag);
                                                                }
                                                                channel.basic_ack(delivery_tag);
                                                            }
                                                            Ok(MyStreamUnificationType::AmqpMessage { message }) => {
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
                                                                    request_bench_event: deserialized
                                                                });
                                                            }
                                                            x => println!("   ❌ 🤔 Unknow type on the stream:   {:?}", x),
                                                        }

                                                        Ok(())
                                                    })
                                            })
                                    })
                            })
                    })
            })
    }).unwrap();

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
