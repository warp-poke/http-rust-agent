use std::fs::File;
use std::io::*;
use std::io;
use std::env;
use toml;

#[derive(Deserialize, Debug)]
pub struct Config {
    pub warp10_token: String,
    pub warp10_url: String,
    pub broker: String,
    pub topic: String,
    pub consumer_group: String,
}

impl Config {
    pub fn new(path: &str) -> Self {
        let err_msg = "Missing config file are values missing";
        let cfg = load_from_path(path);

        let warp10_url = env::var("WARP10_URL").unwrap_or(cfg.as_ref().map(|c| c.warp10_url.clone()).expect(err_msg));
        let warp10_token = env::var("WARP10_TOKEN").unwrap_or(cfg.as_ref().map(|c| c.warp10_token.clone()).expect(err_msg));
        let broker = env::var("BROKER").unwrap_or(cfg.as_ref().map(|c| c.broker.clone()).expect(err_msg));
        let topic = env::var("TOPIC").unwrap_or(cfg.as_ref().map(|c| c.topic.clone()).expect(err_msg));
        let consumer_group = env::var("CONSUMER_GROUP").unwrap_or(cfg.as_ref().map(|c| c.consumer_group.clone()).expect(err_msg));

        Self {
            warp10_url,
            warp10_token,
            broker,
            topic,
            consumer_group,
        }
    }
}

pub fn load_from_path(path: &str) -> io::Result<Config> {
    let data = try!(load_file(path));

    match toml::from_str(&data) {
      Ok(config) => Ok(config),
      Err(e) => {
        println!("decoding error: {:?}", e);
        Err(Error::new(
          ErrorKind::InvalidData,
          format!("decoding error: {:?}", e))
        )
      }
    }
}

pub fn load_file(path: &str) -> io::Result<String> {
    let mut f = try!(File::open(path));
    let mut data = String::new();

    try!(f.read_to_string(&mut data));
    Ok(data)
}