use std::{collections::HashMap, io::{BufReader, BufRead, Read}, borrow::Borrow, cell::RefCell, rc::Rc, sync::{RwLock, Arc}, ops::DerefMut, fmt::Debug};

use futures::{FutureExt, future::join_all, StreamExt};
use quick_xml::Reader;
use redis::{Commands, AsyncCommands, RedisResult, aio::MultiplexedConnection, RedisError};
use serde::{Deserialize, Serialize};
use tokio::{sync::Mutex, time::Instant};
use tracing::{instrument, debug_span, error_span, info_span, Level, event, info, debug, error};

#[instrument(
    name = "populate_redis",
    skip_all,
)]
pub async fn populate_redis<T>(reader: T,mut con:MultiplexedConnection) -> () where T: Read + Debug {

    let mut r = Reader::from_reader(BufReader::new(reader));
    r.trim_text(true);
    let mut buf = Vec::new();

    let client = redis::Client::open("redis://127.0.0.1/").unwrap();
    
    redis::cmd("FT.CREATE").arg("anidbidx").arg("on").arg("json")
        .arg("prefix").arg("1").arg("anidb:").arg("schema")
        .arg("$..titles").arg("as").arg("titles").arg("text").query_async::<_,()>(&mut con).await;


    let mut current_entry = TempEntry{titles:String::new(),aid:0};
    let mut tasks : RefCell<Vec<tokio::task::JoinHandle<()>>> = RefCell::new(vec![]);

    // let con =con;
    debug!("this **WILL** take a second");
    let inst = Instant::now();
    loop {
        match r.read_event(&mut buf) {
            Ok(event) => {
                match event {
                    quick_xml::events::Event::Start(s) => {
                        let tag_name = String::from_utf8_lossy(s.name());
                         for attr in s.html_attributes() {
                            if let Ok(attr) = attr {
                                let (key,value) = (String::from_utf8_lossy(attr.key), String::from_utf8_lossy(&attr.value));
                                if tag_name == "anime" && key == "aid" && current_entry.titles.len() > 0 {
                                    let jsn = serde_json::to_string(&current_entry).unwrap();
                                    let mut con = con.clone();
                                    let task = tokio::spawn(async move {
                                        redis::cmd("JSON.SET").arg(format!("anidb:{}",current_entry.aid)).arg("$")
                                        .arg(jsn).query_async::<_,()>(&mut con).await;
                                    });
                                    tasks.borrow_mut().push(task);
                                    current_entry.titles.clear();
                                    current_entry.aid = value.parse::<u32>().unwrap();
                                    break;
                                } else if tag_name == "title" {
                                    break;
                                }
                            }
                        }
                        
                    },
                    quick_xml::events::Event::End(_) => {
                        // println!("end tag");
                    },
                    quick_xml::events::Event::Empty(_) => {},
                    quick_xml::events::Event::Text(t) => {
                        let text = t.unescaped().unwrap();
                        let text = String::from_utf8_lossy(&text);
                        if !current_entry.titles.is_empty() {
                            current_entry.titles.push(',');
                        }
                        current_entry.titles.push_str(&text);
                        // println!("text {text:?}");
                    },
                    quick_xml::events::Event::Comment(_) => {
                        // println!("comment");
                    },
                    quick_xml::events::Event::CData(_) => todo!(),
                    quick_xml::events::Event::Decl(_) => {
                        debug!("xml declaration (start!)");
                    },
                    quick_xml::events::Event::PI(_) => todo!(),
                    quick_xml::events::Event::DocType(_) => todo!(),
                    quick_xml::events::Event::Eof => {
                        debug!("end of file");
                        break;
                    },
                }
            },
            Err(e) => {
                error!("Error: {:?}", e);
                return ()
            },
        }
        buf.clear();
    }

    debug!("catching up on tasks {}",tasks.borrow().len());
    join_all(tasks.take());

    debug!("completed in {time:?}",time = inst.elapsed());
    // vec![]
}
#[derive(Serialize, Deserialize, Debug)]
pub struct TempEntry {
    titles: String,
    aid: u32,
}
#[derive(Serialize, Deserialize, Debug)]
pub struct AnidbEntry {
    titles: Vec<String>,
    aid: u32,
}
#[instrument(
    skip(con)
)]
pub async fn query_redis(query:&str,mut con:MultiplexedConnection) -> Result<Vec<AnidbEntry>,RedisError> {
    // let safe_query = query.chars().map(|c| if !c.is_alphanumeric() {format!("\\{}",c)} else {c.to_string()}).collect::<String>();
    let mut aniresults= redis::cmd("FT.SEARCH").arg("anidbidx").arg(format!("@titles:'{}*'",query)).query_async::<_,Vec<String>>(&mut con).await?;
    let mut results = vec![];
    debug!("first query results : {:?}",aniresults);
    for id in aniresults {
        let mut con = con.clone();
        let resultsa = redis::cmd("JSON.GET").arg(&[&id,"$..titles"]).query_async::<_,String>(&mut con).await.unwrap();
        let resultsa = serde_json::from_str::<Vec<String>>(&resultsa).unwrap();
        let resultsa = resultsa.first().unwrap();
        let resultsa = resultsa.split(",");
        results.push(AnidbEntry{aid:id.split(":").last().unwrap().parse::<u32>().unwrap(),titles:resultsa.into_iter().map(|s|s.to_string()).collect()});
    }
    // let results = results.iter().map(async |s| 
    // ).collect::<Vec<String>>();
    Ok(results)
}