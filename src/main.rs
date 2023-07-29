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


//staticimp:

mod rendertemplate;
mod staticimp;

use staticimp::*;
use std::sync::Arc;
use std::collections::HashMap;
use actix_web::FromRequest;
use actix_web::web;
use actix_web::web::Data;
//use actix_web::web::Header;
use actix_web::http::header;
use actix_web::http::header::ContentType;

type ConfigData = Data<Arc<Config>>;
type _BackendsData = Data<Box<HashMap<String, Backend>>>;

#[actix_web::get("/")]
async fn index() -> impl actix_web::Responder {
    "Hello from staticimp"
}

//TODO: HttpResult to simplify HTTP handler error handling (HTTP equivalent of ?)
//TODO: proper error types for cleaner code
//  - provide Responder we can directly return them to as http response

//use staticimp::ImpResult;
//use staticimp::ImpError;
//use staticimp::OrImpResult;

/// Handle POST to create new entry
///
/// takes backend,project,branch, and entry type from path
///
#[actix_web::post("/v1/entry/{backend}/{project:.*}/{branch}/{entry_type}")]
async fn post_comment_handler(
    cfg : ConfigData,
    pathargs: web::Path<(String,String,String,String)>,
    content_type: web::Header<header::ContentType>,
    req: actix_web::HttpRequest,
    body: actix_web::web::Payload
) -> impl actix_web::Responder {
    let pathargs = pathargs.into_inner();
    let backend = pathargs.0;
    let project_id = pathargs.1;
    let branch = pathargs.2;
    let entry_type = pathargs.3;

    let mut body = body.into_inner();

    let content_type = content_type.0;

    //parse entry from request
    // currently supports:
    // - html form
    // - json
    // - yaml (using application/yaml content-type)
    let entry = if content_type == ContentType::form_url_encoded() {
        web::Form::<Entry>::from_request(&req,&mut body).await
            .or_bad_request("Bad Form entry")?.into_inner()
    } else if content_type == ContentType::json() {
        let body = web::Bytes::from_request(&req,&mut body).await
            .or_bad_request("Bad payload")?;
        serde_json::from_slice(&body)
            .or_bad_request("Bad json entry")?
    } else if content_type.to_string() == "application/yaml" {
        let body = web::Bytes::from_request(&req,&mut body).await
            .or_bad_request("Bad payload")?;
        serde_yaml::from_slice::<Entry>(&body)
            .or_bad_request("Bad yaml entry")?
    } else {
        return Err(ImpError::BadRequest("","Bad Content-Type".into()))
    };

    let query = web::Query::<HashMap<String,String>>::from_query(req.query_string())
        .or_bad_request("Bad query args")?.into_inner();

    let newentry = cfg.new_entry(project_id,branch,entry,query);

    if let (Some(backend),Some(conf)) = (
        //backends.get(&backend),
        cfg.backends.get(&backend),
        cfg.entries.get(&entry_type)
    ) {
        let newentry = newentry.process_fields(conf.field_config())?;
        //create new client from backend (TODO: per-thread client)
        let mut backend = backend.new_client().await?;

        //create new entry
        backend.new_entry(conf,newentry).await
            .and_then(|_| Ok(actix_web::HttpResponse::Ok().finish()))
    } else {
        Err(ImpError::BadRequest("","Unknown backend or entry type".into()))
    }
}

////
////#[actix_web::get("/hello/{name}")]
////async fn hello(name: web::Path<String>) -> impl actix_web::Responder {
////    format!("Hello {}!", &name)
////}
//
//#[actix_web::get("/getfile/{id}/{path:.*}")]
//async fn getfile(cfg: Data<Config>, client: Data<awc::Client>, pathargs: web::Path<(String,String)>) -> impl actix_web::Responder {
//    todo!("Not Implemented")
//    //let pathargs = pathargs.into_inner();
//    //let (id,path) = (pathargs.0, pathargs.1);
//    //
//    //cfg.get_file(&client,id.as_str(),path.as_str()).await
//}
//
//#[actix_web::post("/addfile/{id}/{path:.*}")]
//async fn addfile(cfg: Data<Config>, client: Data<awc::Client>, pathargs: web::Path<(String,String)>) -> impl actix_web::Responder {
//    todo!("Not Implemented")
//    //let pathargs = pathargs.into_inner();
//    //let (id,path) = (pathargs.0, pathargs.1);
//    //cfg.add_file(&client,id.as_str(),path.as_str()).await
//}
//
//#[actix_web::get("/getproject/{id}")]
//async fn getproject(cfg: Data<Config>, client: Data<awc::Client>, project_id: web::Path<String>) -> impl actix_web::Responder {
//    todo!("Not Implemented")
//    //cfg.get_project(&client,project_id.as_str()).await
//}
//
//#[actix_web::get("/getbranch/{id}/{branch}")]
//async fn getbranch(cfg: Data<Config>, client: Data<awc::Client>, pathargs: web::Path<(String,String)>) -> impl actix_web::Responder {
//    todo!("Not Implemented")
//    //let pathargs = pathargs.into_inner();
//    //let (id,branch) = (pathargs.0, pathargs.1);
//    //cfg.get_branch(&client,id.as_str(),branch.as_str()).await
//}
//
//#[actix_web::post("/comment/form/")]
//async fn comment_form(web::Form(form): web::Form<Entry>) -> impl actix_web::Responder {
//    todo!("Not Implemented")
//    //match serde_yaml::to_string(&form) {
//    //    Ok(comment) => actix_web::HttpResponse::Ok().body(comment),
//    //    Err(_) => actix_web::HttpResponse::BadRequest().body(format!("Invalid request format")),
//    //}
//    ////format!("name: {}\nmessage: {}\n", form.name,form.message)
//}
//
//#[actix_web::post("/comment/query/")]
//async fn comment_query(comment : web::Query<Entry>) -> impl actix_web::Responder {
//    todo!("Not Implemented")
//    //match serde_yaml::to_string(&comment.into_inner()) {
//    //    Ok(comment) => actix_web::HttpResponse::Ok().body(comment),
//    //    Err(_) => actix_web::HttpResponse::BadRequest().body(format!("Invalid request format")),
//    //}
//    ////format!("name: {}\nmessage: {}\n",comment.name,comment.message)
//}
//
//#[actix_web::post("/comment/json/")]
//async fn comment_json(comment : web::Json<Entry>) -> impl actix_web::Responder {
//    todo!("Not Implemented")
//    //match serde_yaml::to_string(&comment) {
//    //    Ok(comment) => actix_web::HttpResponse::Ok().body(comment),
//    //    Err(_) => actix_web::HttpResponse::BadRequest().body(format!("Invalid request format")),
//    //}
//    ////format!("name: {}\nmessage: {}\n",comment.name,comment.message)
//}
//
//#[actix_web::post("/comment/yaml/")]
//async fn comment_yaml(comment: web::Bytes) -> impl actix_web::Responder {
//    todo!("Not Implemented")
//    //match serde_yaml::from_slice::<Comment>(&comment).and_then(|comment| serde_yaml::to_string(&comment)) {
//    //    Ok(comment) => actix_web::HttpResponse::Ok().body(comment),
//    //    Err(_) => actix_web::HttpResponse::BadRequest().body(format!("Invalid request format"))
//    //}
//}


//main - load config and start HttpServer
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let cfgpath = "staticimp.yml";
    let cfg = match Config::load(cfgpath) {
        Ok(cfg) => cfg.env_override(),
        Err(e) => {
            //eprintln!("Error loading config: {:#?}",e);
            eprintln!("Error loading {}: {}",cfgpath,e);
            std::process::exit(1);
        }
    };
    //let backends : HashMap<String,Backend> = cfg.backends.iter().map(|(k,v)| (k,v.new_client().await?)).collect();
    let cfg = ConfigData::new(Arc::new(cfg));
    //let backends = BackendsData::new(Box::new(backends));
    //let backends = cfg.backends.iter().map(|b| 
    let host = cfg.host.clone();
    let port = cfg.port;

    actix_web::HttpServer::new(
        move || {
            actix_web::App::new()
                .app_data(cfg.clone())
                //.app_data(backends.clone())
                //.app_data(Data::new(awc::Client::new()))
                .service(index)
                .service(post_comment_handler)
        })
        .bind((host.as_str(), port))?
        .run()
        .await
}

