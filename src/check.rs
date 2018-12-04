use super::DomainTestResult;
use logs::ANIMALS;
use rand::{Rng, thread_rng};
use reqwest::{Client, Result};
use reqwest::header::CONTENT_LENGTH;
use time::SteadyTime;

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
            .get(CONTENT_LENGTH)
            .and_then(|ct_len| ct_len.to_str().ok()
            .and_then(|ct_len| ct_len.parse().ok()))
            .unwrap_or(0u64),
    };

    if verbose {
        let mut rng = thread_rng();
        let animal = rng.choose(ANIMALS).unwrap();

        debug!("{}  - {} ------", animal, url);
        debug!("{}  --- Status: {}", animal, res.status());
        debug!("{}  --- Headers:", animal);
        for (name, value) in res.headers().iter() {
            debug!("{}  ----- {}: {:?}", animal, name, value.to_str());
        }
        debug!("{}  --- Duration: {}", animal, dur);

    }

    // TODO, real error management and make it a real usable data
    Ok(dtr)
}
