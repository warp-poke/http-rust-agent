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
extern crate env_logger;
extern crate futures;
extern crate futures_cpupool;
extern crate tokio_core;
extern crate uuid;
extern crate warp10;
extern crate rdkafka;

extern crate serde;
extern crate serde_json;
extern crate toml;

use std::collections::HashMap;
use structopt::StructOpt;

use reqwest::Result;
use std::convert::From;
use time::Duration;

mod config;
mod kafka;
mod check;

use config::Config;
use check::run_check_for_url;
use kafka::{run_async_processor, send_message};

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
        #[structopt(short = "c", long = "path-config", default_value = "./config.toml", help = "Path to the config file")]
        path_config_file: String,
    },

    #[structopt(name = "send-kafka")]
    SendKafka {
        #[structopt(help = "domaine name")]
        domain_name: String,
        #[structopt(short = "b", long = "broker", default_value = "localhost:9092", help = "Url of a kafka broker")]
        broker: String,
        #[structopt(short = "o", long = "topic", default_value = "test", help = "Topic kafka to read")]
        topic: String,
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
pub struct DomainTestResult {
    url: String,
    http_status: reqwest::StatusCode,
    answer_time: Duration,
    content_length: u64,
}


impl From<BufferedDomainTestResult> for Vec<warp10::Data> {
    fn from(item: BufferedDomainTestResult) -> Self {
        let mut status_labels = item.request_bench_event.labels.clone();
        item.request_bench_event.checks.status.labels.as_ref().map(
            |l| {
                for (ref k, ref v) in l.iter() {
                    status_labels.insert(k.clone().to_string(), v.clone().to_string());
                }
            },
        );

        let status_labels: Vec<warp10::Label> = status_labels
            .into_iter()
            .map(|(k, v)| warp10::Label::new(&k, &v))
            .collect();

        let mut latency_labels = item.request_bench_event.labels.clone();
        item.request_bench_event
            .checks
            .latency
            .labels
            .as_ref()
            .map(|l| for (ref k, ref v) in l.iter() {
                latency_labels.insert(k.clone().to_string(), v.clone().to_string());
            });

        let latency_labels: Vec<warp10::Label> = latency_labels
            .iter()
            .map(|(k, v)| warp10::Label::new(&k, &v))
            .collect();


        let mut res = Vec::new();

        for result in item.domain_test_results.into_iter() {
            if let Ok(dtr) = result {


                res.push(warp10::Data::new(
                    item.timestamp,
                    None,
                    item.request_bench_event.checks.status.class_name.clone(),
                    status_labels.clone(),
                    warp10::Value::Int(dtr.http_status.as_u16() as i32),
                ));

                res.push(warp10::Data::new(
                    item.timestamp,
                    None,
                    item.request_bench_event.checks.latency.class_name.clone(),
                    latency_labels.clone(),
                    warp10::Value::Int(
                        dtr.answer_time.num_milliseconds() as i32,
                    ),
                ));
            }
        }

        res
    }
}


#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Checks {
    latency: CheckCreds,
    status: CheckCreds,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct CheckCreds {
    class_name: String,
    labels: Option<HashMap<String, String>>,
}


#[derive(Serialize, Deserialize, Debug, Default)]
pub struct RequestBenchEvent {
    labels: HashMap<String, String>,
    url: String,
    checks: Checks,
}

#[derive(Debug)]
pub struct BufferedDomainTestResult {
    domain_test_results: Vec<Result<DomainTestResult>>,
    timestamp: time::Timespec,
    request_bench_event: RequestBenchEvent,
}


fn run(domain_name: &str, args: Opt) -> Vec<warp10::Data> {
    let http = run_check_for_url(format!("http://{}", domain_name).as_str(), args.verbose);
    let https = run_check_for_url(format!("https://{}", domain_name).as_str(), args.verbose);

    let mut rbe = RequestBenchEvent::default();
    rbe.checks.latency.class_name = String::from("http-latency");
    rbe.checks.status.class_name = String::from("http-status");
    rbe.labels.insert(
        String::from("domain"),
        domain_name.to_string(),
    );

    let result = BufferedDomainTestResult {
        domain_test_results: vec![http, https],
        timestamp: time::now_utc().to_timespec(),
        request_bench_event: rbe,
    };

    println!("result:\n{:#?}", result);

    let data: Vec<warp10::Data> = result.into();

    println!("data:\n{:#?}", data);

    data
}

pub fn warp10_post(data: Vec<warp10::Data>, url: String, token: String) -> std::result::Result<warp10::Response, warp10::Error> {
    let client = warp10::Client::new(&url)?;
    let writer = client.get_writer(token);
    let res = writer.post(data)?;
    Ok(res)
}

fn main() {
    env_logger::init();
    let args = Opt::from_args();

    if args.debug {
        println!("CLI arguments parsing : {:#?}", args);
    }

    let cloned_args = args.clone();

    match args.cmd {
        Cmd::Once {
            domain_name,
            warp10_url,
            warp10_token,
        } => {
            //send_message(&broker, &topic, &domain_name);
            let data = run(domain_name.as_str(), cloned_args);

            let res = warp10_post(data, warp10_url, warp10_token);
            println!("{:#?}", res);
        }
        Cmd::Daemon { path_config_file } => {
            let cfg = Config::new(&path_config_file);
            run_async_processor(
                &cfg.broker,
                &cfg.consumer_group,
                &cfg.topic,
                &cfg.warp10_url,
                &cfg.warp10_token,
            )
        }
        Cmd::SendKafka {
            domain_name,
            broker,
            topic,
        } => {
            send_message(&broker, &topic, &domain_name);
        }
    }

}
