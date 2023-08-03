//#![feature(entry_insert)]
//use actix_web::{get, web, App, HttpServer, Responder};
//use actix_web::{get, web, App, HttpServer, Responder};
//use serde::Serialize;
//use serde::Deserialize;
//use actix_web::web::Data;
//use url::Url;
//use gitlab::api::AsyncQuery;
//use futures::StreamExt;
//

const VERSION: &str = env!("CARGO_PKG_VERSION");

//staticimp:

mod rendertemplate;
mod staticimp;

use actix_web::web;
use actix_web::web::Data;
use actix_web::FromRequest;
use staticimp::*;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::{Mutex, RwLock, RwLockWriteGuard};
//use actix_web::web::Header;
use actix_web::http::header;
use actix_web::http::header::ContentType;

type ConfigData = Data<Arc<Config>>;
// cached backends -- since these are thread-specific, we don't technically need RwLock+Mutex
// - just a Box around the HashMap could be enough
// - there is probably a smarter way to do this, but for now I use RwLock to be able to update the
//   map, and Mutex so we can drop to a readlock on the map and lock on the mutex
type BackendsData = Data<RwLock<HashMap<String, Mutex<Backend>>>>;


#[actix_web::get("/")]
async fn index() -> impl actix_web::Responder {
    format!("Hello from staticimp version {}!\n", &VERSION)
}

//use staticimp::ImpResult;
//use staticimp::ImpError;
//use staticimp::OrImpResult;

/// Handle POST to create new entry -- this is the main handler for staticimp
///
/// takes backend,project,branch, and entry type from path
///
/// entry fields taken from request body (based on ContentType)
///
/// params taken from request query parameters
#[actix_web::post("/v1/entry/{backend}/{project:.*}/{branch}/{entry_type}")]
async fn post_comment_handler(
    cfg: ConfigData,
    backends: BackendsData,
    pathargs: web::Path<(String, String, String, String)>,
    content_type: web::Header<header::ContentType>,
    req: actix_web::HttpRequest,
    body: actix_web::web::Payload,
) -> impl actix_web::Responder {
    //get path args
    let pathargs = pathargs.into_inner();
    let backend_name = pathargs.0;
    let project_id = pathargs.1;
    let branch = pathargs.2;
    let entry_type = pathargs.3;

    //unwrap body and content_type
    let mut body = body.into_inner();
    let content_type = content_type.0;

    let query_params = web::Query::<HashMap<String, String>>::from_query(req.query_string())
        .or_bad_request("Bad query args")?
        .into_inner();

    //parse entry from request
    // supports:
    // - html form
    // - json
    // - yaml (using application/yaml content-type)
    let entry_fields = if content_type == ContentType::form_url_encoded() {
        web::Form::<Entry>::from_request(&req, &mut body)
            .await
            .or_bad_request("Bad Form entry")?
            .into_inner()
    } else if content_type == ContentType::json() {
        let body = web::Bytes::from_request(&req, &mut body)
            .await
            .or_bad_request("Bad payload")?;
        serde_json::from_slice(&body).or_bad_request("Bad json entry")?
    } else if content_type.to_string() == "application/yaml" {
        let body = web::Bytes::from_request(&req, &mut body)
            .await
            .or_bad_request("Bad payload")?;
        serde_yaml::from_slice::<Entry>(&body).or_bad_request("Bad yaml entry")?
    } else {
        return Err(ImpError::BadRequest("", "Bad Content-Type".into()));
    };


    // get backend to use
    // - first we get a read lock on backends
    //   - we hold the read lock for the rest of this function to keep the client mutex borrow valid
    // - if we already have a backend client for backend_name, return the mutex for it
    // - else if we don't already have the client, but we do have a backend config for it
    //   - get a write lock (dropping the read lock)
    //   - create a new client and insert it
    //   - acquire a new read lock (inside the write lock to avoid blocking)
    //   - drop write lock (by leaving scope)
    //   - return backend client mutex from map
    // - else return an error (unknown backend)
    let mut lock = backends.read();
    let backend = if let Some(backend) = lock.get(&backend_name) {
        backend
    } else if let Some(backend_conf) = cfg.backends.get(&backend_name) {
        drop(lock); //drop read lock (so we can acquire write lock)
        lock = {
            //acquire write lock
            let mut write = backends.write();
            //confirm no-one just added the client before we relocked
            if !write.contains_key(&backend_name) {
                //insert new backend client using write lock
                write.insert(backend_name.clone(),Mutex::from(backend_conf.new_client().await?));
                //return new readlock (obtained inside write lock), dropping write lock
            }
            RwLockWriteGuard::downgrade(write)
        };
        //return the newly added backend client (using the read lock)
        lock.get(&backend_name).unwrap()
    } else {
        return Err(ImpError::BadRequest("", "Unknown backend".into()));
    };

    // get entry conf to use (from project if enabled)
    // - if project_config_path set we try project config first
    //   - if we can't load project config we error
    // - if project doesn't have entry type, try global conf entry types
    // - if we never found entry conf it is an error
    // - entry conf in Cow so we don't need to clone global entry conf
    //   - borrowed from global conf or owned from project conf
    let entry_conf = if !cfg.project_config_path.is_empty() {
        //project-conf enabled -- get project conf file
        // - error if we can't load project conf
        backend
            .lock()
            .get_conf(&cfg, &project_id, &branch)
            .await?
            .entries
            .remove(&entry_type) //all we care about is the current entry type
            .and_then(|conf| Some(Cow::Owned(conf)))
    } else {
        //project conf disabled
        None
    }
    //if we didn't get conf from project, try global conf entry types
    .or_else(
        ||
        cfg.entries.get(&entry_type) //get global conf entry config
            .and_then(|conf| Some(Cow::Borrowed(conf))), //wrap global conf entry type in Cow::Borrowed
    )
    //error if we couldn't find entry type (got None)
    .ok_or(ImpError::BadRequest("", "Unknown entry type".into()))?; //return error if we couldn't find entry type

    //create the NewEntry and process the entry fields
    let newentry = cfg
        .new_entry(project_id, branch, entry_fields, query_params)
        .process_fields(entry_conf.field_config())?;

    //create new entry
    backend
        .lock()
        .new_entry(&entry_conf, newentry)
        .await?;

    Ok(actix_web::HttpResponse::Ok().finish())
}

//main - load config and start HttpServer
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let cfgpath = "staticimp.yml";
    let cfg = match Config::load(cfgpath) {
        Ok(cfg) => cfg.env_override(),
        Err(e) => {
            //eprintln!("Error loading config: {:#?}",e);
            eprintln!("Error loading {}: {}", cfgpath, e);
            std::process::exit(1);
        }
    };
    let cfg = ConfigData::new(Arc::new(cfg));
    //let backends : HashMap<String,Backend> = cfg.backends.iter().map(|(k,v)| (k,v.new_client().await?)).collect();
    //let backends = BackendsData::new(Box::new(backends));
    let backends = BackendsData::new(RwLock::from(HashMap::new())); //let threads create clients as-needed
    let host = cfg.host.clone();
    let port = cfg.port;

    actix_web::HttpServer::new(move || {
        actix_web::App::new()
            .app_data(cfg.clone())
            .app_data(backends.clone())
            //.app_data(Data::new(awc::Client::new()))
            .service(index)
            .service(post_comment_handler)
    })
    .bind((host.as_str(), port))?
    .run()
    .await
}
