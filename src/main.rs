//use actix_web::{get, web, App, HttpServer, Responder};
//use actix_web::{get, web, App, HttpServer, Responder};
use serde::Serialize;
use serde::Deserialize;
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
pub trait GitAPI {
    async fn add_file(&self, id: &str, path: &str) -> Result<actix_web::HttpResponse,Box<dyn std::error::Error>>;
    async fn get_file(&self, id: &str, path: &str) -> Result<actix_web::HttpResponse,Box<dyn std::error::Error>>;
    //async fn get_file(&self, id: &str, path: &str) -> Result<bytes::Bytes,Box<dyn std::error::Error>>;
}

pub struct GitLabAPI {
    api_url: String,
    token: String,
    client: awc::Client
}

impl GitLabAPI {
    pub fn new() -> Self {
        GitLabAPI {
            api_url: std::env::var("GITLAB_API_V4_URL").unwrap_or("".to_owned()),
            token: std::env::var("GITLAB_TOKEN").unwrap_or("".to_owned()),
            client: awc::Client::new()
        }
    }
}

#[async_trait::async_trait(?Send)]
impl GitAPI for GitLabAPI {
    async fn add_file(&self, id: &str, path: &str) -> Result<actix_web::HttpResponse,Box<dyn std::error::Error>> {
        //TODO: figure out api for specifying args
        let request = serde_json::json!({
            "branch": "main",
            "content": "This is a test file\nand a second line",
            "commit_message": "test create file from rust"
        });
        let path = url_escape::encode_component(path);
        let mut result = self.client.post(format!("{}/projects/{}/repository/files/{}",self.api_url,id,path))
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
    async fn get_file(&self, id: &str, path: &str) -> Result<actix_web::HttpResponse,Box<dyn std::error::Error>> {
    //async fn get_file(&self, id: &str, path: &str) -> Result<bytes::Bytes,Box<dyn std::error::Error>> {
        let path = url_escape::encode_component(path);
        let mut result = self.client.get(format!("{}/projects/{}/repository/files/{}/raw",self.api_url,id,path))
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
async fn getfile(pathargs: actix_web::web::Path<(String,String)>) -> impl actix_web::Responder {
    let pathargs = pathargs.into_inner();
    let (id,path) = (pathargs.0, pathargs.1);
    
    GitLabAPI::new().get_file(id.as_str(),path.as_str()).await
}

#[actix_web::post("/addfile/{id}/{path:.*}")]
async fn addfile(pathargs: actix_web::web::Path<(String,String)>) -> impl actix_web::Responder {
    let pathargs = pathargs.into_inner();
    let (id,path) = (pathargs.0, pathargs.1);
    GitLabAPI::new().add_file(id.as_str(),path.as_str()).await
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
//fn main() {
    actix_web::HttpServer::new(|| actix_web::App::new()
        .service(index)
        .service(hello)
        .service(getfile)
        .service(addfile)
        .service(comment_json)
        .service(comment_form)
        .service(comment_query)
        .service(comment_yaml)
        )
        .bind(("127.0.0.1", 8080))?
        .run()
        .await
}

