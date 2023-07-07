//use actix_web::{get, web, App, HttpServer, Responder};
//use actix_web::{get, web, App, HttpServer, Responder};
use serde::Serialize;
use serde::Deserialize;
use actix_web::web::Data;
use url::Url;
//use futures::StreamExt;
//

//struct Config {
//    gitlab_token: Option<String>
//}
//static CONFIG: once_cell::sync::Lazy<Config> = once_cell::sync::Lazy::new(|| Config {
//    gitlab_token: match std::env::var("GITLAB_TOKEN") {
//        Ok(val) => Some(val),
//        Err(_) => None
//    }
//});


#[async_trait::async_trait(?Send)]
trait BackendAPI {
    async fn add_file(&self, client: &awc::Client, id: &str, path: &str) -> Result<actix_web::HttpResponse,Box<dyn std::error::Error>>;
    async fn get_file(&self, client: &awc::Client, id: &str, path: &str) -> Result<actix_web::HttpResponse,Box<dyn std::error::Error>>;
    //async fn get_file(&self, id: &str, path: &str) -> Result<bytes::Bytes,Box<dyn std::error::Error>>;
}

#[derive(Clone,Debug, Serialize, Deserialize)]
struct Config {
    //client: awc::Client,
    backend: Backend
}

impl Config {
    fn load(path: &str) -> Result<Self,Box<dyn std::error::Error>> {
        let f = std::fs::File::open(path)?;
        let mut cfg : Config = serde_yaml::from_reader(f)?;

        //TODO: figure out cleaner solution (to take env var and value to conditionally override)
        //  - either setter function or custom deserialize for backends
        let env_gitlab_api = std::env::var("GITLAB_API_V4_URL").unwrap_or("".to_owned());
        if !env_gitlab_api.is_empty() {
            match cfg.backend {
                Backend::GitLab(ref mut backend) => {
                    backend.api = Url::parse(env_gitlab_api.as_str())?;
                }
            }
        }
        let env_gitlab_token = std::env::var("GITLAB_TOKEN").unwrap_or("".to_owned());
        if !env_gitlab_token.is_empty() {
            match cfg.backend {
                Backend::GitLab(ref mut backend) => {
                    backend.token = env_gitlab_token;
                }
            }
        }

        Ok(cfg)
    }
    async fn add_file(&self, client: &awc::Client, id: &str, path: &str) -> Result<actix_web::HttpResponse,Box<dyn std::error::Error>> {
        self.backend.add_file(client,id,path).await
    }
    async fn get_file(&self, client: &awc::Client, id: &str, path: &str) -> Result<actix_web::HttpResponse,Box<dyn std::error::Error>> {
        self.backend.get_file(client,id,path).await
    }
}

#[derive(Clone,Debug, Serialize, Deserialize)]
#[serde(tag = "driver")] //TODO: clean solution to support multiple backends (which may use the same or different Backend)
enum Backend {
    GitLab(GitLabAPI)
}

//TODO: map/list of Backends in Config (also implement proper serialization/deserialization ignoring client)
#[async_trait::async_trait(?Send)]
impl BackendAPI for Backend {
    async fn add_file(&self, client: &awc::Client, id: &str, path: &str) -> Result<actix_web::HttpResponse,Box<dyn std::error::Error>> {
        match self {
            Backend::GitLab(backend) => backend.add_file(client,id,path).await
        }
    }
    async fn get_file(&self, client: &awc::Client, id: &str, path: &str) -> Result<actix_web::HttpResponse,Box<dyn std::error::Error>> {
        match self {
            Backend::GitLab(backend) => backend.get_file(client,id,path).await
        }
    }
}

#[derive(Clone,Debug, Serialize, Deserialize)]
struct GitLabAPI {
    #[serde(alias = "api_v4_url")]
    api: Url,
    #[serde(default)]
    token: String,
}

//impl Clone for GitLabAPI {
//    fn clone(&self) -> Self {
//        GitL
//    }
//}

impl GitLabAPI {
    //pub fn new() -> Self {
    //    GitLabAPI {
    //        //FIXME: error check url (and pull from config
    //        api: Url::parse(std::env::var("GITLAB_API_V4_URL").unwrap().as_str()).unwrap(),
    //        token: std::env::var("GITLAB_TOKEN").unwrap_or("".to_owned()),
    //        //client: awc::Client::new()
    //    }
    //}
}

#[async_trait::async_trait(?Send)]
impl BackendAPI for GitLabAPI {
    async fn add_file(&self, client: &awc::Client, id: &str, path: &str) -> Result<actix_web::HttpResponse,Box<dyn std::error::Error>> {
        //TODO: figure out api for specifying args
        let request = serde_json::json!({
            "branch": "main",
            "content": "This is a test file\nand a second line",
            "commit_message": "test create file from rust"
        });
        let mut url = self.api.clone();
        url.path_segments_mut().map_err(|_| "Bad API Url")?
            .extend(&["projects",id,"repository","files",path]);
        //let path = url_escape::encode_component(path);
        //let mut result = self.client.post(format!("{}/projects/{}/repository/files/{}",self.api_url,id,path))
        let mut result = client.post(url.as_str())
            .insert_header(("User-Agent", "staticimp/0.1"))
            .insert_header(("PRIVATE-TOKEN", self.token.as_str()))
            //.send()
            //.content_type("application/json")
            .send_json(&request)
            .await?;
        let rbody = result.body().await?;
        //TODO: don't send full gitlab response (or have debug flag to enable)
        //  - normally should send back higher level error
        match result.status() {
            awc::http::StatusCode::OK => Ok(actix_web::HttpResponse::Ok().body(rbody)),
            _ => Ok(actix_web::HttpResponseBuilder::new(result.status()).body(rbody))
        }
    }
    async fn get_file(&self, client: &awc::Client, id: &str, path: &str) -> Result<actix_web::HttpResponse,Box<dyn std::error::Error>> {
        let mut url = self.api.clone();
        url.path_segments_mut().map_err(|_| "Bad API Url")?
            .extend(&["projects",id,"repository","files",path,"raw"]);
        //let path = url_escape::encode_component(path);
        //let mut result = self.client.get(format!("{}/projects/{}/repository/files/{}/raw",self.api_url,id,path))
        let mut result = client.get(url.as_str())
            .insert_header(("User-Agent", "staticimp/0.1"))
            .insert_header(("PRIVATE-TOKEN", self.token.as_str()))
            .send()
            //.content_type("application/json")
            //.send_json(&request)
            .await?;
        let rbody = result.body().await?;
        //TODO: don't send full gitlab response (or have debug flag to enable)
        //  - normally should send back higher level error
        match result.status() {
            awc::http::StatusCode::OK => Ok(actix_web::HttpResponse::Ok().body(rbody)),
            _ => Ok(actix_web::HttpResponseBuilder::new(result.status()).body(rbody))
        }
    }
}

//#[derive(Debug, derive_more::Display, derive_more::Error)]
//enum CommentError {
//    #[display(fmt = "Bad comment request format")]
//    BadRequest,
//}
//impl actix_web::ResponseError for CommentError {
//    fn error_response(&self) -> actix_web::HttpResponse {
//	actix_web::HttpResponse::build(self.status_code())
//	    .insert_header(actix_web::http::header::ContentType::html())
//	    .body(self.to_string())
//    }
//
//    fn status_code(&self) -> actix_web::http::StatusCode {
//	match *self {
//	    CommentError::BadRequest => actix_web::http::StatusCode::BAD_REQUEST,
//	}
//    }
//}

#[derive(Serialize,Deserialize)]
struct Comment {
    name: String,
    //email: String,
    message: String
}

#[actix_web::get("/")]
async fn index() -> impl actix_web::Responder {
    "Hello, World!"
}

#[actix_web::get("/hello/{name}")]
async fn hello(name: actix_web::web::Path<String>) -> impl actix_web::Responder {
    format!("Hello {}!", &name)
}

#[actix_web::get("/getfile/{id}/{path:.*}")]
async fn getfile(cfg: Data<Config>, client: Data<awc::Client>, pathargs: actix_web::web::Path<(String,String)>) -> impl actix_web::Responder {
    let pathargs = pathargs.into_inner();
    let (id,path) = (pathargs.0, pathargs.1);
    
    cfg.get_file(&client,id.as_str(),path.as_str()).await
}

#[actix_web::post("/addfile/{id}/{path:.*}")]
async fn addfile(cfg: Data<Config>, client: Data<awc::Client>, pathargs: actix_web::web::Path<(String,String)>) -> impl actix_web::Responder {
    let pathargs = pathargs.into_inner();
    let (id,path) = (pathargs.0, pathargs.1);
    cfg.add_file(&client,id.as_str(),path.as_str()).await
}

#[actix_web::post("/comment/form/")]
async fn comment_form(actix_web::web::Form(form): actix_web::web::Form<Comment>) -> impl actix_web::Responder {
    match serde_yaml::to_string(&form) {
        Ok(comment) => actix_web::HttpResponse::Ok().body(comment),
        Err(_) => actix_web::HttpResponse::BadRequest().body(format!("Invalid request format")),
    }
    //format!("name: {}\nmessage: {}\n", form.name,form.message)
}

#[actix_web::post("/comment/query/")]
async fn comment_query(comment : actix_web::web::Query<Comment>) -> impl actix_web::Responder {
    match serde_yaml::to_string(&comment.into_inner()) {
        Ok(comment) => actix_web::HttpResponse::Ok().body(comment),
        Err(_) => actix_web::HttpResponse::BadRequest().body(format!("Invalid request format")),
    }
    //format!("name: {}\nmessage: {}\n",comment.name,comment.message)
}

#[actix_web::post("/comment/json/")]
async fn comment_json(comment : actix_web::web::Json<Comment>) -> impl actix_web::Responder {
    match serde_yaml::to_string(&comment) {
        Ok(comment) => actix_web::HttpResponse::Ok().body(comment),
        Err(_) => actix_web::HttpResponse::BadRequest().body(format!("Invalid request format")),
    }
    //format!("name: {}\nmessage: {}\n",comment.name,comment.message)
}

#[actix_web::post("/comment/yaml/")]
async fn comment_yaml(comment: actix_web::web::Bytes) -> impl actix_web::Responder {
    match serde_yaml::from_slice::<Comment>(&comment).and_then(|comment| serde_yaml::to_string(&comment)) {
        Ok(comment) => actix_web::HttpResponse::Ok().body(comment),
        Err(_) => actix_web::HttpResponse::BadRequest().body(format!("Invalid request format"))
    }
}


#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let cfgpath = "staticimp.yml";
    let cfg = Config::load(cfgpath);
    if let Err(e) = cfg {
        //eprintln!("Error loading config: {:#?}",e);
        eprintln!("Error loading {}: {}",cfgpath,e);
        std::process::exit(1);
    }
    let cfg = Data::new(cfg.unwrap());
    actix_web::HttpServer::new(
        move || {
            actix_web::App::new()
                .app_data(cfg.clone())
                .app_data(Data::new(awc::Client::new()))
                .service(index)
                .service(hello)
                .service(getfile)
                .service(addfile)
                .service(comment_json)
                .service(comment_form)
                .service(comment_query)
                .service(comment_yaml)
        })
        .bind(("127.0.0.1", 8080))?
        .run()
        .await
}

