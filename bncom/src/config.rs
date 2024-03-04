use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
use std::str::FromStr;
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Server {
    pub key: String,
    pub port: u16,
    #[serde(rename = "-limit-port")]
    pub _limit_port: Option<(u16, u16)>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Iomap {
    pub inner: String,
    pub outer: u16,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Client {
    pub key: String,
    pub server: String,
    pub map: Vec<Iomap>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub server: Option<Server>,
    pub client: Option<Client>,
}

impl Config {
    pub fn from_str(js: &str) -> Option<Config> {
        let mut res: Option<Config> = match serde_json::from_str(js) {
            Ok(cfg) => Some(cfg),
            Err(e) => {
                panic!("error {}", e);
            }
        };
        // 加密key
        if let Some(k) = &mut res {
            let mut hasher = DefaultHasher::new();
            if let Some(s) = &mut k.server {
                hasher.write(&s.key.as_bytes());
                s.key = format!("{:x}", hasher.finish());
            }
            if let Some(c) = &mut k.client {
                hasher.write(&c.key.as_bytes());
                c.key = format!("{:x}", hasher.finish());
            }
        }
        return res;
    }

    pub fn from_file(filename: &str) -> (String, Option<Config>) {
        let f = File::open(filename);
        match f {
            Ok(mut file) => {
                let mut s = String::new();
                file.read_to_string(&mut s).unwrap();
                let c = Config::from_str(&s);
                (s, c)
            }
            Err(e) => {
                panic!("error {}", e)
            }
        }
    }
}

/// The config
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Config type: 1: json file, 2: base64 string
    #[arg(short, long, default_value_t = 1)]
    type_of_config: u8,

    /// Json file config
    #[arg(short, long, default_value_t = String::from_str("config.json").unwrap())]
    json_file_config: String,

    /// Base64 config
    #[arg(short, long, default_value_t = String::from_str("").unwrap())]
    base64_config: String,
}

// 获取配置
pub fn get_config() -> (String, Config) {
    let args = Args::parse();
    match args.type_of_config {
        1 => {
            let (s, o) = Config::from_file(&args.json_file_config);
            (s, o.unwrap())
        }
        2 => {
            let b64b = crate::base64::base64_decode(&args.base64_config);
            let b64s = String::from_utf8_lossy(&b64b).to_string();
            let c = Config::from_str(&b64s).unwrap();
            (b64s, c)
        }
        _ => {
            panic!("No config")
        }
    }
}

#[test]
fn test_from_str() {
    let js = r#"{
        "server": {
            "key": "helloworld",
            "port": 8808,
            "-limit-port": [
                9100,
                9110
            ]
        }
    }"#;
    let c = Config::from_str(&js).unwrap();
    assert_eq!(c.server.unwrap()._limit_port, Some((9100, 9110)));
    if let Some(v) = c.client {
        panic!("{:?}", v);
    }

    let js = r#"{
        "server": {
            "key": "helloworld",
            "port": 8808
        }
    }"#;
    let c = Config::from_str(&js).unwrap();
    assert_eq!(c.server.unwrap()._limit_port, None);

    let js = r#"{
        "client": {
            "key": "helloworld",
            "server": "127.0.0.1:8808",
            "map": [
                {
                    "inner": "127.0.0.1:6379",
                    "outer": 9100
                },
                {
                    "inner": "127.0.0.1:6379",
                    "outer": 9101
                }
            ]
        }
    }"#;
    let c = Config::from_str(&js).unwrap();
    if let Some(v) = c.server {
        panic!("{:?}", v);
    }
    assert_eq!(c.client.unwrap().map[0].outer, 9100);
}

#[test]
fn test_from_file() {
    let (_, c) = Config::from_file("config.json");
    let c = c.unwrap();
    assert_eq!(c.server.as_ref().unwrap()._limit_port, Some((9100, 9110)));
    assert_eq!(c.client.as_ref().unwrap().map[0].outer, 9100);

    let s = serde_json::to_string(&c).expect("Couldn't serialize config");
    assert_eq!(s.contains("9110"), true);
}
