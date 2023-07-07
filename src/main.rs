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


//Backend - backend api trait for staticimp
// - each backend api should implement this interface
// - TODO: implement higher-level helper functions (e.g. commit file to new branch and create MR)
// - TODO: instead of returning HttpResponse for the client, return the appropriate data
//   - then calling function can use the data
// - TODO: refactor into BackendAPI and GitAPI (which implements BackendAPI)
//   - then we have the flexibility to support non-git backends in the future (e.g. database or web service)
#[async_trait::async_trait(?Send)]
trait BackendAPI {
    async fn add_file(&self, client: &awc::Client, id: &str, path: &str) -> Result<actix_web::HttpResponse,Box<dyn std::error::Error>>;
    async fn get_file(&self, client: &awc::Client, id: &str, path: &str) -> Result<actix_web::HttpResponse,Box<dyn std::error::Error>>;
    //async fn get_file(&self, id: &str, path: &str) -> Result<bytes::Bytes,Box<dyn std::error::Error>>;
    async fn get_project(&self, client: &awc::Client, id: &str) -> Result<actix_web::HttpResponse,Box<dyn std::error::Error>>;
    async fn get_branch(&self, client: &awc::Client, id: &str, branch: &str) -> Result<actix_web::HttpResponse,Box<dyn std::error::Error>>;
}

//Config - staticimp configuration
// - loaded from staticimp.yml, then overridden from environment vars
// - right now only supports a single backend (gitlab)
//   - TODO: clean solution to support multiple backends (which may use the same or different Backend)
// - currently also implements BackendAPI (passes through to backend), but that will probably change with multiple backend support
#[derive(Clone,Debug, Serialize, Deserialize)]
struct Config {
    backend: Backend,
    host:String,
    port:u16
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
}

#[async_trait::async_trait(?Send)]
impl BackendAPI for Config {
    async fn add_file(&self, client: &awc::Client, id: &str, path: &str) -> Result<actix_web::HttpResponse,Box<dyn std::error::Error>> {
        self.backend.add_file(client,id,path).await
    }
    async fn get_file(&self, client: &awc::Client, id: &str, path: &str) -> Result<actix_web::HttpResponse,Box<dyn std::error::Error>> {
        self.backend.get_file(client,id,path).await
    }
    async fn get_project(&self, client: &awc::Client, id: &str) -> Result<actix_web::HttpResponse,Box<dyn std::error::Error>> {
        self.backend.get_project(client,id).await
    }
    async fn get_branch(&self, client: &awc::Client, id: &str, branch: &str) -> Result<actix_web::HttpResponse,Box<dyn std::error::Error>> {
        self.backend.get_branch(client,id,branch).await
    }
}

//Backend - enum for backend apis
//  - variants are the supported backend apis
//  - BackendAPI implementation just passes through to the current variant
#[derive(Clone,Debug, Serialize, Deserialize)]
#[serde(tag = "driver")]
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
    async fn get_project(&self, client: &awc::Client, id: &str) -> Result<actix_web::HttpResponse,Box<dyn std::error::Error>> {
        match self {
            Backend::GitLab(backend) => backend.get_project(client,id).await
        }
    }
    async fn get_branch(&self, client: &awc::Client, id: &str, branch: &str) -> Result<actix_web::HttpResponse,Box<dyn std::error::Error>> {
        match self {
            Backend::GitLab(backend) => backend.get_branch(client,id,branch).await
        }
    }
}

//GitLabAPI - implementation of the GitLab REST api
//  - TODO: support oauth
//  - TODO: return appropriate data instead of client response (see BackendAPI)
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
    async fn get_project(&self, client: &awc::Client, id: &str) -> Result<actix_web::HttpResponse,Box<dyn std::error::Error>> {
        let mut url = self.api.clone();
        url.path_segments_mut().map_err(|_| "Bad API Url")?
            .extend(&["projects",id]);
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
    async fn get_branch(&self, client: &awc::Client, id: &str, branch: &str) -> Result<actix_web::HttpResponse,Box<dyn std::error::Error>> {
        let mut url = self.api.clone();
        url.path_segments_mut().map_err(|_| "Bad API Url")?
            .extend(&["projects",id,"repository","branches",branch]);
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

//Comment - struct for holding a comment
// - TODO: make this generic, so that fields can be customized at config level
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

#[actix_web::get("/getproject/{id}")]
async fn getproject(cfg: Data<Config>, client: Data<awc::Client>, project_id: actix_web::web::Path<String>) -> impl actix_web::Responder {
    cfg.get_project(&client,project_id.as_str()).await
}

#[actix_web::get("/getbranch/{id}/{branch}")]
async fn getbranch(cfg: Data<Config>, client: Data<awc::Client>, pathargs: actix_web::web::Path<(String,String)>) -> impl actix_web::Responder {
    let pathargs = pathargs.into_inner();
    let (id,branch) = (pathargs.0, pathargs.1);
    cfg.get_branch(&client,id.as_str(),branch.as_str()).await
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


//main - load config and start HttpServer
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
    let host = cfg.host.clone();
    let port = cfg.port;

    actix_web::HttpServer::new(
        move || {
            actix_web::App::new()
                .app_data(cfg.clone())
                .app_data(Data::new(awc::Client::new()))
                .service(index)
                .service(hello)
                .service(getfile)
                .service(addfile)
                .service(getproject)
                .service(getbranch)
                .service(comment_json)
                .service(comment_form)
                .service(comment_query)
                .service(comment_yaml)
        })
        .bind((host.as_str(), port))?
        .run()
        .await
}

