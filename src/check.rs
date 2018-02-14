use super::{ANIMALS,DomainTestResult};
use time::SteadyTime;
use reqwest::{Client,Result};
use reqwest::header::ContentLength;
use rand::{Rng,thread_rng};

pub fn run_check_for_url(url: &str, verbose: bool) -> Result<DomainTestResult> {
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

    if verbose {
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

