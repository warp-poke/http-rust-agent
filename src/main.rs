extern crate structopt;
#[macro_use]
extern crate structopt_derive;
extern crate reqwest;

use structopt::StructOpt;

use reqwest::{Client, Result};

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


fn run(domain_name: &str) -> Result<()> {
    let client = Client::new();
    let res = client
        .get(format!("http://{}", domain_name).as_str())
        .send()?;
    let res_ssl = client
        .get(format!("https://{}", domain_name).as_str())
        .send()?;

    println!("Status: {}", res.status());
    println!("Headers:\n{}", res.headers());
    println!("Status: {}", res_ssl.status());
    println!("Headers:\n{}", res_ssl.headers());


    println!("\n\nDone.");
    Ok(())
}


fn main() {
    let opt = Opt::from_args();

    run(opt.domain_name.as_str());



    /*
    let mut content = String::new();
    resp.read_to_string(&mut content);
*/
    println!("{:?}", opt);
}
