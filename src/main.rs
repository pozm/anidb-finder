use std::{fs::File, io::BufReader};

use anidb_finder::{populate_redis, query_redis};
use tokio::io;
use std::io::prelude::*;
use flate2::{Decompress, bufread::GzDecoder};
#[tokio::main]
async fn main() {
    // let xml_data;
    tracing_subscriber::fmt::init();
    
    let f = File::open("./animetitles.xml.gz").unwrap();
    let b = BufReader::new(f);
    let mut gz_reader = GzDecoder::new(b);

    // let mut s = String::new();
    // gz_reader.read_to_string(&mut s).unwrap();
    // println!("{}", s);
    let client = redis::Client::open("redis://127.0.0.1/").expect("u need redis");
    let mut con = client.get_multiplexed_tokio_connection().await.unwrap();

    populate_redis(gz_reader,con.clone()).await;

    let mut input = String::new();
    let stdin = std::io::stdin();
    println!("search for: ");
    while let Ok(_) = stdin.read_line(&mut input) {

        
        let found = query_redis(input.trim(), con.clone()).await;
        input.clear();
        println!("{:?}", found);
        println!("");
        println!("search for: ");
    }

    
}
