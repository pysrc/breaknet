use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
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

    pub fn from_file(filename: &str) -> Option<Config> {
        let f = File::open(filename);
        match f {
            Ok(mut file) => {
                let mut c = String::new();
                file.read_to_string(&mut c).unwrap();
                Config::from_str(&c)
            }
            Err(e) => {
                panic!("error {}", e)
            }
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
    let c = Config::from_file("config.json").unwrap();
    assert_eq!(c.server.as_ref().unwrap()._limit_port, Some((9100, 9110)));
    assert_eq!(c.client.as_ref().unwrap().map[0].outer, 9100);

    let s = serde_json::to_string(&c).expect("Couldn't serialize config");
    assert_eq!(s.contains("9110"), true);
}
