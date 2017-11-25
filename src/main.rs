extern crate structopt;
#[macro_use]
extern crate structopt_derive;
extern crate reqwest;
extern crate time;
extern crate rand;
extern crate nats;

use rand::{thread_rng, Rng};
use structopt::StructOpt;

use reqwest::{Client, Result};
use reqwest::header::ContentLength;
use time::{Duration, SteadyTime};
use nats::*;


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
        /// Needed parameter, the first on the command line.
        #[structopt(help = "url of the nats server")]
        // TODO manage NATS cluster (multiples url)
        nats_url: String,
    },
}

pub const ANIMALS: &'static [&'static str] = &[
    "🐶",
    "🐱",
    "🐭",
    "🐹",
    "🐰",
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
    domain_name: String,
    url: String,
    http_status: reqwest::StatusCode,
    answer_time: Duration,
    content_length: u64,
}

#[derive(Serialize, Deserialize, Debug)]
enum CheckType {
    Latency,
    Status,
}

#[derive(Serialize, Deserialize, Debug)]
struct CheckCreds {
    className: String,
    labels: Option<HashMap<String, String>>,
}

#[derive(Serialize, Deserialize, Debug)]
struct RequestBenchEvent {
    labels: HashMap<String, String>,
    url: String,
    checks: HashMap<CheckType, CheckCreds>,
}


fn run_check_for_url(url: &str, domain_name: &str, args: &Opt) -> Result<DomainTestResult> {
    let client = Client::new();
    let start = SteadyTime::now();
    let res = client.get(url).send()?;
    let dur = SteadyTime::now() - start;

    //  build infos
    let dtr = DomainTestResult {
        domain_name: domain_name.to_owned(),
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
    }
    Ok(dtr)
}

fn run(
    domain_name: &str,
    args: Opt,
) -> Result<(Result<DomainTestResult>, Result<DomainTestResult>)> {
    let http = run_check_for_url(
        format!("http://{}", domain_name).as_str(),
        domain_name,
        &args,
    );
    let https = run_check_for_url(
        format!("https://{}", domain_name).as_str(),
        domain_name,
        &args,
    );

    Ok((http, https))
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
        Cmd::Daemon { nats_url } => {
            println!("Connect to NATS server....");
            let mut client = nats::Client::new(nats_url).unwrap();
            let s2 = client.subscribe("subject.*", Some("http-agent")).unwrap();
            for event in client.events() {
                println!("{:#?}", event);
                println!("{:#?}", String::from_utf8(event.msg));
            }

        }
    }

}
