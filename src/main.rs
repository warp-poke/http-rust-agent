extern crate structopt;
#[macro_use]
extern crate structopt_derive;
extern crate reqwest;
extern crate time;

use structopt::StructOpt;

use reqwest::{Client, Result};
use reqwest::header::ContentLength;
use time::{Duration, SteadyTime};

#[derive(StructOpt, Debug)]
#[structopt(name = "poke-agent", about = "HTTP poke agent")]
struct Opt {
    #[structopt(short = "d", long = "debug", help = "Activate debug mode")]
    debug: bool,

    #[structopt(short = "v", long = "verbose", help = "Activate verbose mode")]
    verbose: bool,

    /// Needed parameter, the first on the command line.
    #[structopt(help = "domaine name")]
    domain_name: String,
}

#[derive(Debug)]
struct DomainTestResult {
    domain_name: String,
    url: String,
    http_status: reqwest::StatusCode,
    answer_time: Duration,
    content_length: u64,
}


fn run_check_for_url(url: &str, domain_name: &str) -> Result<DomainTestResult> {
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

    println!("Headers:\n{}", res.headers());
    Ok(dtr)
}

fn run(domain_name: &str) -> Result<(Result<DomainTestResult>, Result<DomainTestResult>)> {
    let http = run_check_for_url(format!("http://{}", domain_name).as_str(), domain_name);
    let https = run_check_for_url(format!("https://{}", domain_name).as_str(), domain_name);

    Ok((http, https))
}


fn main() {
    let opt = Opt::from_args();

    let rr = run(opt.domain_name.as_str());

    println!("{:#?}", rr);


    /*
    let mut content = String::new();
    resp.read_to_string(&mut content);
*/
    println!("{:?}", opt);
}
