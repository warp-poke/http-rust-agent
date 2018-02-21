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
mod logs;

use config::Config;
use check::run_check_for_url;
use kafka::{run_async_processor, send_message};
use warp10::Label;

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
        #[structopt(short = "t", long = "topic", default_value = "test", help = "Topic kafka to read")]
        topic: String,
        #[structopt(short = "u", long = "sasl-user", help = "SASL username")]
        username: Option<String>,
        #[structopt(short = "p", long = "sasl-password", help = "SASL password")]
        password: Option<String>,
        #[structopt(long = "warp10-url", default_value = "http://localhost:8080/", help = "Url of the Warp10 datastore")]
        warp10_url: String,
        #[structopt(long = "warp10-token", help = "Token to write in the Warp10 datastore")]
        warp10_token: String,
        #[structopt(long = "url", help = "url")]
        url: String,
    },
}

#[derive(Debug)]
pub struct DomainTestResult {
    url: String,
    http_status: reqwest::StatusCode,
    answer_time: Duration,
    content_length: u64,
}


impl From<BufferedDomainTestResult> for Vec<warp10::Data> {
    fn from(item: BufferedDomainTestResult) -> Self {
        let mut res = Vec::new();

        for result in item.domain_test_results.into_iter() {
            if let Ok(dtr) = result {

                res.push(warp10::Data::new(
                    item.timestamp,
                    None,
                    "http.response.status".to_string(),
                    item.labels.clone(),
                    warp10::Value::Int(dtr.http_status.as_u16() as i32),
                ));

                res.push(warp10::Data::new(
                    item.timestamp,
                    None,
                    "http.response.time".to_string(),
                    item.labels.clone(),
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
pub struct RequestBenchEvent {
    domain_name: String,
    url: String,
    warp10_endpoint: String,
    token: String,
}

#[derive(Debug)]
pub struct BufferedDomainTestResult {
    domain_test_results: Vec<Result<DomainTestResult>>,
    timestamp: time::Timespec,
    request_bench_event: RequestBenchEvent,
    labels: Vec<Label>,
}

fn run(domain_name: &str, args: Opt) -> Vec<warp10::Data> {
    let http = run_check_for_url(format!("http://{}", domain_name).as_str(), args.verbose);
    let https = run_check_for_url(format!("https://{}", domain_name).as_str(), args.verbose);

    let mut rbe = RequestBenchEvent::default();

    let result = BufferedDomainTestResult {
        domain_test_results: vec![http, https],
        timestamp: time::now_utc().to_timespec(),
        request_bench_event: rbe,
        labels: Vec::with_capacity(3),
    };

    debug!("result:\n{:#?}", result);

    let data: Vec<warp10::Data> = result.into();

    debug!("data:\n{:#?}", data);

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

    let cloned_args = args.clone();

    match args.cmd {
        Cmd::Once {
            domain_name,
            warp10_url,
            warp10_token,
        } => {
            let data = run(domain_name.as_str(), cloned_args);

            let res = warp10_post(data, warp10_url, warp10_token);
            info!("result warp10 post: {:#?}", res);
        }
        Cmd::Daemon { path_config_file } => {
            let cfg = Config::new(&path_config_file);
            run_async_processor(
                &cfg.broker,
                &cfg.consumer_group,
                &cfg.topic,
                cfg.username,
                cfg.password,
                cfg.host,
                cfg.zone
            )
        }
        Cmd::SendKafka {
            domain_name,
            broker,
            topic,
            username,
            password,
            warp10_url,
            warp10_token,
            url,
        } => {
            send_message(&broker, &topic, &domain_name, &warp10_url, &warp10_token, &url, username, password);
        }
    }

}
