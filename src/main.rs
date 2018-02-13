extern crate structopt;
#[macro_use]
extern crate structopt_derive;
extern crate reqwest;
extern crate time;
extern crate rand;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate log;
extern crate futures;
extern crate futures_cpupool;
extern crate tokio_core;
extern crate uuid;
extern crate warp10;
extern crate rdkafka;

extern crate serde;
extern crate serde_json;

use futures::Future;
use futures::Stream;
use futures::sync::mpsc;
use futures::sync::mpsc::*;
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
use std::vec;

mod kafka;

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
        #[structopt(short = "u", long = "warp10-url", default_value = "http://localhost:8080/", help = "Url of the Warp10 datastore")]
        warp10_url: String,
        #[structopt(short = "t", long = "warp10-token", help = "Token to write in the Warp10 datastore")]
        warp10_token: String,
    },

    #[structopt(name = "daemon")]
    Daemon {
        #[structopt(short = "s", long = "buffer_in_seconds", parse(try_from_str), default_value = "10", help = "Time in seconds, for buffer to send data in warp10")]
        buffer_in_seconds: u64,
        #[structopt(short = "u", long = "warp10-url", default_value = "http://localhost:8080/", help = "Url of the Warp10 datastore")]
        warp10_url: String,
        #[structopt(short = "t", long = "warp10-token", help = "Token to write in the Warp10 datastore")]
        warp10_token: String,
        #[structopt(help = "url of the rabbit  server")]
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


impl From<BufferedDomainTestResult> for Vec<warp10::Data>  {
    fn from(item: BufferedDomainTestResult) -> Self {
        let mut res = Vec::new();

        for result in item.domain_test_results.into_iter() {
            if let Ok(dtr) = result {

                let mut status_labels = item.request_bench_event.labels.clone();
                item.request_bench_event.checks.status.labels.as_ref().map(|l| {
                    for (ref k, ref v) in l.iter() {
                        status_labels.insert(k.clone().to_string(), v.clone().to_string());
                    }
                });

                let status_labels: Vec<warp10::Label> = status_labels.into_iter().map(|(k, v)| {
                    warp10::Label::new(&k, &v)
                }).collect();

                res.push(warp10::Data::new(
                    item.timestamp,
                    None,
                    item.request_bench_event.checks.status.class_name.clone(),
                    status_labels,
                    warp10::Value::Int(dtr.http_status.as_u16() as i32)
                ));

                let mut latency_labels = item.request_bench_event.labels.clone();
                item.request_bench_event.checks.latency.labels.as_ref().map(|l| {
                    for (ref k, ref v) in l.iter() {
                        latency_labels.insert(k.clone().to_string(), v.clone().to_string());
                    }
                });

                let latency_labels: Vec<warp10::Label> = latency_labels.iter().map(|(k, v)| {
                    warp10::Label::new(&k, &v)
                }).collect();

                res.push(warp10::Data::new(
                    item.timestamp,
                    None,
                    item.request_bench_event.checks.latency.class_name.clone(),
                    latency_labels,
                    warp10::Value::Int(dtr.answer_time.num_milliseconds() as i32)
                ));
            }
        }

        res
    }
}


#[derive(Serialize, Deserialize, Debug, Default)]
struct Checks {
    latency: CheckCreds,
    status: CheckCreds,
}

#[derive(Serialize, Deserialize, Debug, Default)]
struct CheckCreds {
    class_name: String,
    labels: Option<HashMap<String, String>>,
}


#[derive(Serialize, Deserialize, Debug, Default)]
struct RequestBenchEvent {
    labels: HashMap<String, String>,
    url: String,
    checks: Checks,
}

#[derive(Debug)]
struct BufferedDomainTestResult {
    domain_test_results: Vec<Result<DomainTestResult>>,
    timestamp: time::Timespec,
    delivery_tag: u64,
    request_bench_event: RequestBenchEvent,
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

fn run(domain_name: &str, args: Opt) -> Vec<warp10::Data> {
    let http = run_check_for_url(format!("http://{}", domain_name).as_str(), &args);
    let https = run_check_for_url(format!("https://{}", domain_name).as_str(), &args);

    let mut rbe = RequestBenchEvent::default();
    rbe.checks.latency.class_name = String::from("http-latency");
    rbe.checks.status.class_name = String::from("http-status");
    rbe.labels.insert(String::from("domain"), domain_name.to_string());

    let result = BufferedDomainTestResult {
      domain_test_results: vec![http, https],
      timestamp: time::now_utc().to_timespec(),
      delivery_tag: 42,
      request_bench_event: rbe
    };

    println!("result:\n{:#?}", result);

    let data: Vec<warp10::Data> = result.into();

    println!("data:\n{:#?}", data);

    data
}

fn warp10_post(data: Vec<warp10::Data>, url: String, token: String) -> std::result::Result<warp10::Response, warp10::Error> {
    let client = warp10::Client::new(&url)?;
    let writer = client.get_writer(token);
    let res    = writer.post(data)?;
    Ok(res)
}

fn main() {
    let args = Opt::from_args();

    if args.debug {
        println!("CLI arguments parsing : {:#?}", args);
    }

    let cloned_args = args.clone();

    match args.cmd {
        Cmd::Once { domain_name, warp10_url, warp10_token } => {
            let data = run(domain_name.as_str(), cloned_args);

            let res = warp10_post(data, warp10_url, warp10_token);
            println!("{:#?}", res);
        }
        Cmd::Daemon {
            buffer_in_seconds,
            rabbitmq_url,
            warp10_url,
            warp10_token
        } => {},//daemonify(rabbitmq_url, buffer_in_seconds, cloned_args),
    }

}
