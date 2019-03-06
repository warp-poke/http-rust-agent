use rand::seq::SliceRandom;
use rand::thread_rng;
use reqwest::{
    Client,
    header::{CONTENT_LENGTH, USER_AGENT}, Result,
};
use time::SteadyTime;

use logs::ANIMALS;

use super::DomainTestResult;

const VERSION: &'static str = env!("CARGO_PKG_VERSION");

pub fn run_check_for_url(url: &str, verbose: bool) -> Result<DomainTestResult> {
    let client = Client::new();
    let start = SteadyTime::now();
    let res = client.get(url).header(USER_AGENT, format!("Poke/{}", VERSION)).send()?;
    let dur = SteadyTime::now() - start;

    //  build infos
    let dtr = DomainTestResult {
        url: url.to_owned(),
        http_status: res.status(),
        answer_time: dur,
        content_length: res
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|ct_len| ct_len.to_str().ok().and_then(|ct_len| ct_len.parse().ok()))
            .unwrap_or(0u64),
    };

    if verbose {
        let mut rng = thread_rng();
        let animal = ANIMALS.choose(&mut rng).expect("to retrieve an animal for debug");

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
