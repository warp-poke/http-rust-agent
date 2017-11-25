extern crate structopt;
#[macro_use]
extern crate structopt_derive;
extern crate reqwest;
extern crate time;
extern crate rand;

use rand::{thread_rng, Rng};
use structopt::StructOpt;

use reqwest::{Client, Result};
use reqwest::header::ContentLength;
use time::{Duration, SteadyTime};


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
            println!("I need to launch daemon");


        }
    }




    /*
    let mut content = String::new();
    resp.read_to_string(&mut content);
*/
}
