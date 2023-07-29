//! Simple module for sending sets of fields to backend APIs
//!
//! staticimp takes [Entry]s with fields, performs validation and transformations,
//! and then sends the entry to a backend (currently just gitlab or the debug backend).
//!
//! All the code was written by me (Michael Agun), but this project was inspired by
//! [Staticman](https://staticman.net/).
//! - this was originally written because staticman was too heavy for some serverless websites I am
//! building, but it is an awesome project and you should check it out too, especially if you are
//! already using node and/or have plenty of server resources
//! - This is an active work in progress (I am currently actively developing this for my own use)
//!
//! Features:
//! - clean implementation intended to be flexible and extensible
//! - configuration can use placeholders to fill in/transform entries
//!   - uses rendertemplate (in this crate) for rendering placeholders
//! - loads configuration from `staticman.yml`
//!   - doesn't yet support project-specific config or json
//! - entries are validated by checking for allowed/required fields
//!   - doesn't yet support any formatting validation
//! - extra fields generated from config
//! - has code to load/handle field transformations (but doesn't have any implemented yet)
//! - supports gitlab and debug backends currently
//!
//!
//!
//! # Work In Progress
//!
//! staticimp is a work-in-progress. The features above all work, but test code isn't included yet
//! and there are some missing important features that I am still implementing
//!
//! **Features still to implement**
//! - review branches
//! - thorough test code
//! - create and cache clients per-thread (rather than creating a new client for each request)
//! - load project/branch-specific config files
//!   - right now just loads the global conf at startup
//! - implement field transformations
//! - more documentation
//! - logging
//! - spam protection (probably reCAPTCHA)
//! - github as a second backend
//! - I might include a filesystem backend for easier configuration
//! - specify allowed hosts for a backend
//!
//!
//! # Implemented Backends:
//! 
//! **[Debug]**
//!
//! the Debug backend just returns ImpError::Debug with the processed Entry
//!
//! This is mostly just for development and testing config files
//!
//! **[Gitlab]**
//!
//! uses [crate@gitlab] to send files to gitlab
//!
//! - doesn't yet support review entries (i.e. placing entries in new branches), but the structure
//!   is in place and it should be implemented soon

//use actix_web::http::header::ContentType;
use actix_web::HttpResponse;
use actix_web::http::StatusCode;
use serde::Serialize;
use serde::Deserialize;
use gitlab::api::AsyncQuery;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Display;
use uuid::Uuid;
use chrono::prelude::*;
use crate::rendertemplate;
use rendertemplate::render_str;
use rendertemplate::Render;
use std::borrow::Cow;
//use std::cell::RefCell;
//use std::ops::Deref;

type BoxError = Box<dyn std::error::Error>;

/// Module error
///
/// Implements [actix::ResponseError] so it can be returned directly from actix request handler
#[derive(Debug)]
pub enum ImpError {
    //TODO: don't just Box everything
    /// BadRequest with message and child error
    BadRequest(&'static str,BoxError),
    /// InternalServerError with message and child error
    InternalError(&'static str,BoxError),
    /// Debugging error
    ///
    /// create using `ImpError::debug*` functions
    Debug(&'static str,BoxError),
}

/// ImpError constructors
#[allow(dead_code)]
impl ImpError {
    /// returns string for debugging (as an ImpError)
    fn debug<T>(msg : &'static str, val : T) -> Self where T : std::fmt::Display {
        ImpError::Debug(msg,val.to_string().into())
    }
    /// returns debug-print of object for debugging
    fn debug_dbg<T>(msg : &'static str, val : T) -> Self where T : std::fmt::Debug {
        ImpError::Debug(msg,format!("{:?}",val).into())
    }
    /// returns pretty-printed json object for debugging
    ///
    /// If serialization fails it returns the debug-print of the object
    fn debug_pretty<T>(msg : &'static str, val : T) -> Self where T : std::fmt::Debug+Serialize {
        let val_str = match serde_json::to_string_pretty(&val) {
            Ok(s) => s,
            Err(e) => format!("{:?}\n+Serialize err: {:?}",val,e)
        };
        ImpError::Debug(msg,val_str.into())
    }
}

/// Display formatting for ImpError
impl Display for ImpError {
    /// format error to string
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let fmt_msg = |s : &str| {
            let s = s.to_string();
            if s.is_empty() {
                s
            } else {
                s + ": "
            }
        };
        use ImpError::*;
        match self {
            BadRequest(s,e) =>
                write!(f,"{}{}", fmt_msg(s), e.to_string()),
            InternalError(s,e) =>
                write!(f,"{}{}", fmt_msg(s), e.to_string()),
            Debug(s,e) =>
                write!(f,"{}{}", fmt_msg(s), e)
        }
    }
}

/// actix ResponseError implementation so ImpErrors can be directly returned to actix handler
///
/// returns [actix_web::HttpResponse] containing error string
/// - status code based on variant (most of the variant names are obvious)
///
/// [ResponseError] : actix_web::ResponseError
/// [HttpResponse] : actix_web
impl actix_web::ResponseError for ImpError {
    /// returns self.to_string() as HttpResponse
    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code())
            //.insert_header(ContentType::html())
            .body(self.to_string())
    }
    /// status code for ImpError variant
    fn status_code(&self) -> StatusCode {
        use ImpError::*;
        match self {
            BadRequest(_,_) => StatusCode::BAD_REQUEST,
            InternalError(_,_) => StatusCode::INTERNAL_SERVER_ERROR,
            Debug(_,_) => StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

/// trait for converting other [Result] types into [ImpResult]
pub trait OrImpResult<T> {
    /// returns Ok or [ImpError::BadRequest]
    fn or_bad_request(self, message : &'static str) -> ImpResult<T>;

    /// returns Ok or [ImpError::InternalError]
    fn or_internal_error(self, message : &'static str) -> ImpResult<T>;
}

/// converts any [Result]<T,E> into [ImpError]<T>
///
/// E must implement error [Error] (per ImpError)
///
/// [Error] : std::error::Error
impl<T,E> OrImpResult<T> for Result<T,E> where E : std::error::Error+'static {
    fn or_bad_request(self, message : &'static str) -> ImpResult<T> {
        self.or_else(|e| -> Result<T, ImpError> {Err(ImpError::BadRequest(message,Box::new(e)))})
    }

    fn or_internal_error(self, message : &'static str) -> ImpResult<T> {
        self.or_else(|e| Err(ImpError::InternalError(message,e.into())))
    }
}

/// module Result
///
/// all Result-returning functions return ImpError
/// - this helps with cleaner code in actix handler since we can use ?
pub type ImpResult<R> = Result<R, ImpError>;

/// Transformation to apply to a field
#[derive(Clone,Debug, Serialize, Deserialize)]
struct FieldTransform {
    field : String
}

/// Field to generate
///
/// This also acts as the builder for generated fields (using [GeneratedField::render])
#[derive(Clone,Debug, Serialize, Deserialize)]
struct GeneratedField {
    value : String
}

/// Renders a generated field
impl Render<&NewEntry,ImpResult<String>> for GeneratedField {
    /// create generated field for NewEntry
    ///
    /// currently just replaces placeholders in self.value
    fn render(&self, entry : &NewEntry) -> ImpResult<String> {
        Ok(render_str(&self.value,entry))
    }
}

/// Field validation and mutation rules for entry
///
/// - `allowed` - list of fields that are allowed to be in a Entry
/// - `required` - fields that must exist in the Entry
/// - `transforms` - transformations to apply to entry fields
/// - `extra` - fields to generate and add to entry
#[derive(Default,Clone,Debug, Serialize, Deserialize)]
pub struct FieldConfig {
    allowed : HashSet<String>,
    #[serde(default)]
    required : HashSet<String>,
    #[serde(default)]
    transforms : Vec<FieldTransform>,
    #[serde(default)]
    extra : HashMap<String,GeneratedField>
}

/// Serialization format for entry
#[derive(Default,Copy,Clone,Debug, Serialize, Deserialize)]
enum SerializationType {
    /// json serialization (using serde_json)
    #[serde(rename = "json")]
    #[default]
    Json,

    /// yaml serialization (using serde_yaml)
    #[serde(rename = "yaml",alias = "yml")]
    Yaml
}

/// Git-specific entry config
///
/// placeholders are allowed so configuration values can be pulled from entry fields and query
/// options
#[derive(Clone,Debug, Serialize, Deserialize)]
pub struct GitEntryConfig {
    /// Directory path to store entry under
    path : String,
    /// Filename to use for entry
    filename : String,
    /// Branch to send entries to (or submit merge request for)
    branch : String,
    /// When review is enabled, name of review branch for commit
    #[serde(default = "GitEntryConfig::default_review_branch")]
    review_branch : String,
    /// Git commit message
    commit_message : String
}

impl GitEntryConfig {
    /// the default review branch ( "staticimp_" + entry uid )
    fn default_review_branch() -> String {
        "staticimp_{@id}".to_string()
    }
}

/// configuration for new entry
///
/// This also acts as the builder for [NewEntry]s (using [EntryConfig::render])
#[derive(Default,Clone,Debug, Serialize, Deserialize)]
pub struct EntryConfig {
    /// Configuration for entry fields
    fields : FieldConfig,
    /// Whether moderation is enabled
    review : bool,
    /// Entry serialization format
    format : SerializationType,
    /// Git-specific entry config
    ///
    /// - its an option so a single entry type can support multiple backends
    git : Option<GitEntryConfig>
}

impl EntryConfig {
    /// get the configuration for entry fields
    pub fn field_config(&self) -> &FieldConfig {
        &self.fields
    }
}

/// BackendAPI is interface staticimp uses to talk to backends
#[async_trait::async_trait(?Send)]
pub trait BackendAPI {
    /// send a processed entry to the backend
    async fn new_entry(&mut self, entry_conf : &EntryConfig, entry : NewEntry) -> ImpResult<()>;
    //async fn get_entry(&self, id: &str, path: &str) -> ImpResult<Entry>;
}

/// Gitlab backend configuration
#[derive(Clone,Debug, Serialize, Deserialize)]
pub struct GitlabConfig {
    /// gitlab host url (without leading https://)
    host: String,
    /// token to authenticate with gitlab
    #[serde(default)]
    token: String
}

impl GitlabConfig {
    /// create a new api client
    async fn new_client(&self) -> ImpResult<GitlabAPI> {
        let client = gitlab::GitlabBuilder::new(self.host.as_str(),self.token.as_str()).build_async().await
            .or_internal_error("Failed to open client")?;
        Ok(GitlabAPI::new(client))
    }
}

/// backend for debugging staticimp and config (returns debug info to client)
#[derive(Clone,Debug, Serialize, Deserialize)]
pub struct DebugConfig {
}

#[async_trait::async_trait(?Send)]
impl BackendAPI for DebugConfig {
    /// debug new_entry -- just returns entry_conf and processed entry fields to client
    async fn new_entry(&mut self, entry_conf : &EntryConfig, entry : NewEntry) -> ImpResult<()> {
        Err(ImpError::debug_pretty("",(entry_conf,entry)))
    }
}

/// enum of backend configuration variants
#[derive(Clone,Debug, Serialize, Deserialize)]
#[serde(tag = "driver")]
pub enum BackendConfig {
    /// gitlab backend configuration
    #[serde(rename = "gitlab")]
    Gitlab(GitlabConfig),
    /// debug backend configuration
    #[serde(rename = "debug")]
    Debug(DebugConfig)
}

impl BackendConfig {
    /// creates a new client from the backend configuration
    ///
    /// for Gitlab it creates a new api client
    ///
    /// for Debug it just clones the debug config
    pub async fn new_client(&self) -> ImpResult<Backend> {
        match self {
            BackendConfig::Gitlab(conf) => {
                let client = conf.new_client().await?;
                Ok(Backend::Gitlab(client))
            },
            BackendConfig::Debug(conf) => Ok(Backend::Debug(conf.clone()))
        }
    }
}


/// Config - staticimp configuration
///
/// Also acts as the builder for [NewEntry] via [`Config::new_entry`]
///
/// - Configuration override order:
///   - service staticimp.yml
///   - environment variables
///   - site staticman.yml (if allow_repo_override set)
///     - not implemented yet
///
#[derive(Clone,Debug, Serialize, Deserialize)]
pub struct Config {
    /// set of backend configurations
    pub backends : HashMap<String,BackendConfig>,
    /// host to listen on
    pub host : String,
    /// port to listen on
    pub port : u16,
    /// path to project-specific configuration file
    #[serde(default)]
    project_config_path : String, //empty -- global service conf only
    #[serde(default = "Config::default_timestamp_format")]
    timestamp_format : String, //allow setting format of {@timestamp}
    /// configuration for each entry type
    pub entries : HashMap<String,EntryConfig>
}

impl Config {
    /// Load configuration file
    ///
    /// deserializes Config using serde_yaml
    pub fn load(path: &str) -> ImpResult<Self> {
        let f = std::fs::File::open(path)
            .or_internal_error("Couldn't open config file")?;
        Ok(serde_yaml::from_reader(f).or_internal_error("Bad config yaml")?)
    }

    /// override config parameters from environment variables
    ///
    /// takes and returns self, since we currently always call it after loading
    /// - makes for clean code (see examples)
    ///
    /// Supported overrides:
    /// - `timestamp_format` - default timestamp format
    /// - `<backend>_host` - hostname for the specified backend
    /// - `<backend>_token` - authentication token for the specified backend
    ///
    /// # Examples
    ///
    /// load config and then environment variables (or return error)
    /// ```
    /// let backend = Config::load(cfgpath)?.env_override();
    /// ```
    ///
    /// load config and match if you need more complex error handling
    /// ```
    /// let cfg = match Config::load(cfgpath) {
    ///     Ok(cfg) => cfg.env_override(),
    ///     Err(e) => {
    ///         //eprintln!("Error loading config: {:#?}",e);
    ///         eprintln!("Error loading {}: {}",cfgpath,e);
    ///         std::process::exit(1);
    ///     }
    /// };
    /// ```
    ///
    pub fn env_override(mut self) -> Self {
        let env_override = |var : &mut String,varname : &str| {
            let val = std::env::var(varname).unwrap_or("".to_owned());
            if !val.is_empty() {
                *var = val;
            }
        };

        env_override(&mut self.timestamp_format, "timestamp_format");

        for (name,backend) in self.backends.iter_mut() {
            match backend {
                BackendConfig::Gitlab(gitlab) => {
                    env_override(&mut gitlab.host,&(name.clone() + "_host"));
                    env_override(&mut gitlab.token,&(name.clone() + "_token"));
                },
                BackendConfig::Debug(_) => {}
            }
        }
        self
    }
    /// default Timestamp format (compact ISO8601 with milliseconds)
    fn default_timestamp_format() -> String {
        "%Y%m%dT%H%M%S%.3fZ".to_string()
    }

    /// build a [NewEntry]
    ///
    /// takes path and query paramters plus entry fields
    pub fn new_entry(&self,
        project_id: String,
        branch: String,
        entry : Entry,
        options : HashMap<String,String>
        ) -> NewEntry {
        NewEntry::new(self,project_id,branch,entry,options)
    }
}

/// staticimp Entry content
#[derive(Default,Clone,Debug, Serialize, Deserialize)]
pub struct Entry {
    /// Entry Fields
    #[serde(flatten)]
    fields : HashMap<String,String>
}

impl Entry {
    /// create new entry from HashMap
    pub fn _new(fields : HashMap<String,String> ) -> Self {
        Entry { fields }
    }
    /// set entry fields (and returns self)
    fn _field(mut self, key : String, value : String) -> Self {
        self.fields.insert(key,value);
        self
    }

    /// serialize entry for sending to backend
    fn serialize(&self, serializer : SerializationType) -> ImpResult<Vec<u8>> {
        use SerializationType::*;
        match serializer {
            Json => Ok(
                serde_json::to_string(&self.fields)
                .or_internal_error("Bad json output")?
                .as_bytes().into()
            ),
            Yaml => Ok(
                serde_yaml::to_string(&self.fields)
                .or_internal_error("Bad yaml output")?
                .as_bytes().into()
            )
        }
    }
}

/// builder for sending a new entry to the backend
#[derive(Clone,Debug)]
pub struct GitEntry {
    /// id for the project to send this entry to
    project_id : String,
    /// branch to write the entry to
    branch : String,
    /// path to write the entry to
    file_path : String,
    /// entry content
    entry : Entry,
    /// commit message for entry
    commit_message : String,
    /// whether to create new branch for entry
    review : bool,
    /// serializer to use
    serializer : SerializationType
}

impl GitEntry {
    /// serialize entry fields per entry config
    fn serialize(&self) -> ImpResult<Vec<u8>> {
        self.entry.serialize(self.serializer)
    }
}

/// Context for expanding placeholders while processing an entry
#[derive(Default,Clone,Debug,Serialize)]
pub struct NewEntry {
    /// uuid for entry
    uid : String,
    /// timetamp for entry
    timestamp : DateTime<Utc>,
    /// prerendered default timestamp string
    timestamp_str : String,
    /// project id (for gitlab could be name/path OR numeric id)
    project_id : String,
    /// project branch
    branch : String,
    /// entry fields
    entry : Entry,
    /// options attached to request (HTTP query options)
    options : HashMap<String,String>,
    //special : &'a HashMap<&'a str, String>,
}

impl NewEntry {
    /// build new entry context to fill in placeholders
    fn new(config : &Config, project_id: String, branch: String, entry : Entry, options : HashMap<String,String>) -> Self {
        let uid = Uuid::new_v4().to_string();
        let timestamp = Utc::now();
        let timestamp_str = format!("{}",timestamp.format(&config.timestamp_format));
        NewEntry {
            uid,
            timestamp,
            timestamp_str,
            project_id,
            branch,
            entry,
            options,
            //special : HashMap::from([
            //    ( "@id", uid )
            //])
        }
    }

    /// render a formatted data (from `{date:format}` placeholders)
    fn render_date(&self, fmt : &str) -> String {
        format!("{}",self.timestamp.format(fmt))
    }

    /// validate fields in entry
    fn validate_fields(self, conf : &FieldConfig) -> ImpResult<Self> {
        let keys : HashSet<String> = self.entry.fields.keys().map(|s| s.to_string()).collect();
        if !conf.required.is_subset(&keys) {
            Err(ImpError::BadRequest("","Missing field(s)".into()))
        } else if !keys.is_subset(&conf.allowed) {
            Err(ImpError::BadRequest("","Unknown field(s)".into()))
        } else {
            // passed all validation requests, return self
            Ok(self)
        }
    }

    /// Generate extra fields
    fn generate_fields<'a,I>(mut self, fields : I) -> ImpResult<Self> where I : Iterator<Item = (&'a String,&'a GeneratedField)> {
        for (key,gen) in fields {
            let val = gen.render(&self)?;
            self.entry.fields.insert(key.to_string(),val);
        }
        Ok(self)
    }

    /// Transform fields
    fn transform_fields<'a,I>(mut self, transforms : I) -> ImpResult<Self> where I : Iterator<Item = &'a FieldTransform> {
        for t in transforms {
            if let Some(_field) = self.entry.fields.get_mut(&t.field) {
                todo!("Transform fields")
            }
        }
        Ok(self)
    }

    /// Process entry fields
    ///
    /// Processing Order:
    /// 1. validation
    /// 2. extra fields
    /// 3. transformations
    pub fn process_fields(self, conf : &FieldConfig) -> ImpResult<Self> {
        self
            .validate_fields(&conf)?
            .generate_fields(conf.extra.iter())?
            .transform_fields(conf.transforms.iter())
    }
}

/// placeholder rendering for entry processing
///
/// renders entry processing placeholders to a [Cow]
///
/// returns [Cow::Borrowed] for everything but formatted dates
/// timestamp is prerendered, 
impl<'a> Render<&str,Option<Cow<'a,str>>> for &'a NewEntry {
    /// renders a Entry field or config value for a NewEntry
    ///
    /// return value is Option<Cow> borrowed from entry whenever possible
    ///
    /// - `placeholder` - the placeholder to render
    fn render(&self, placeholder : &str) -> Option<Cow<'a,str>> {
        //TODO: should we return empty string or placeholder name on no match?
        if placeholder.starts_with('@') { //special generated vars
            //self.special.get(placeholder).map_or(&"",|v| &v)
            Some(
                if placeholder == "@id" {
                    Cow::Borrowed(&self.uid)
                } else if placeholder == "@timestamp" {
                    Cow::Borrowed(&self.timestamp_str)
                } else if placeholder.starts_with("@date:") {
                    let fmt = &placeholder[6..];
                    Cow::Owned(self.render_date(fmt))
                } else if placeholder.starts_with("@branch") {
                    Cow::Borrowed(&self.branch)
                } else {
                    Cow::Borrowed("".into())
                }
            )
        } else {
            if let Some((lhs,rhs)) = placeholder.split_once('.') {
                if lhs == "fields" {
                    self.entry.fields.get(rhs).and_then(
                        |v| Some(Cow::Borrowed(v.as_str()))
                    )
                } else if lhs == "options" {
                    self.options.get(rhs).and_then(
                        |val| Some(Cow::Borrowed(val.as_str()))
                    )
                } else {
                    None
                }
            } else {
                None
            }
        }
    }
}

//NOTE: this is the old placeholder rendering code using microtemplate
// - microtemplate only allows &str return values and readonly context, so it's now been replaced
// by rendertemplate so we can return generated values for placeholders
// - I'll probably delete this code in the next commit -- just for posterity
//impl microtemplate::Context for &NewEntry {
//    //TODO: microtemplate doesn't have a good way to return expansion errors, so we just return
//    //  empty string if no match -- track errors
//    fn get_field(&self, field_name: &str) -> &str {
//        //TODO: should we return empty string or field name on no match?
//        if field_name.starts_with('@') { //special generated vars
//            //self.special.get(field_name).map_or(&"",|v| &v)
//            if field_name == "@id" {
//                &self.uid
//            } else if field_name == "@timestamp" {
//                //&self.timestamp
//                //self.render_date("%Y")
//                todo!("@timestamp")
//            } else if field_name.starts_with("@date:") {
//                //let fmt = &field_name[6..];
//                //self.render_date(fmt)
//                todo!("@date:")
//            } else if field_name.starts_with("@branch") {
//                &self.branch
//            } else {
//                &""
//            }
//        } else {
//            if let Some((lhs,rhs)) = field_name.split_once('.') {
//                if lhs == "fields" {
//                    self.entry.fields.get(rhs).map_or(&"",|v| &v)
//                } else if lhs == "options" {
//                    match self.options.get(rhs) {
//                        Some(val) => &val,
//                        None => ""
//                    }
//                } else {
//                    &""
//                }
//            } else {
//                &""
//            }
//        }
//    }
//}

/// Builder for [GitEntry]s from [NewEntry]s
///
/// builds git-specific entry from config and NewEntry
impl Render<NewEntry,ImpResult<GitEntry>> for EntryConfig {
    /// build GitEntry from NewEntry context
    fn render(&self, entry : NewEntry) -> ImpResult<GitEntry> {
        if entry.branch.is_empty() {
            Err(ImpError::BadRequest("","Must specify branch".into()))
        } else if let Some(gitconf) = self.git.as_ref() {
            let branch : String = render_str(&gitconf.branch,&entry);
            if !branch.is_empty() && branch != entry.branch {
                Err(ImpError::BadRequest("","Branch not allowed".into()))
            } else {
                use std::path::Path;
                let file_path : String = render_str(&gitconf.path,&entry);
                let filename : String = render_str(&gitconf.filename,&entry);
                let file_path = Path::new(&file_path).join(&filename).to_str()
                    .ok_or_else(|| ImpError::BadRequest("","Bad Entry Path".to_string().into()))?.to_string();
                let branch = render_str(&gitconf.branch,&entry);
                let commit_message = render_str(&gitconf.commit_message,&entry);
                // destructure entry so we can move instead of cloning fields
                let NewEntry { project_id, entry, .. } = entry;
                Ok(GitEntry {
                    project_id,
                    branch,
                    //FIXME: build proper path (e.g. strip dup '/')
                    file_path,
                    entry,
                    commit_message,
                    review : self.review,
                    serializer : self.format
                })
            }
        } else {
            Err(ImpError::BadRequest("","missing git entry configuration".into()))
        }
    }
}

/// Backend enum (variants represent the supported backends)
pub enum Backend {
    Gitlab(GitlabAPI),
    Debug(DebugConfig)
}

/// Backend enum dispatch to backend type
#[async_trait::async_trait(?Send)]
impl BackendAPI for Backend {
    /// send a new entry to the backend
    async fn new_entry(&mut self, entry_conf : &EntryConfig, entry : NewEntry) -> ImpResult<()> {
        match self {
            Backend::Gitlab(api) => api.new_entry(&entry_conf,entry),
            Backend::Debug(conf) => conf.new_entry(&entry_conf,entry)
        }.await
    }
}


/// represents git commit from backend api
///
/// it only includes the fields we actually care about, not all available
#[derive(Clone,Debug, Serialize, Deserialize)]
struct GitCommit {
    id : String
}

/// represents git branch from backend api
///
/// it only includes the fields we actually care about, not all available
#[derive(Clone,Debug, Serialize, Deserialize)]
pub struct GitBranch {
    name : String,
    commit : GitCommit
}

/// represents git project from backend api
///
/// it only includes the fields we actually care about, not all available
#[derive(Clone,Debug, Serialize, Deserialize)]
pub struct GitProject {
    id : u32,
    name : String,
    path : String,
    #[serde(rename = "path_with_namespace")]
    full_path : String
}


/// git-specific backend api
#[async_trait::async_trait(?Send)]
pub trait GitAPI {
    /// get repo file contents for given ref
    async fn get_file(&self, project: &str, ref_: &str, path: &str) -> ImpResult<Vec<u8>>;
    /// commit a new file to the repo
    async fn new_file(&self, project: &str, branch: &str, path: &str, content: &Vec<u8>, commit_message: &str) -> ImpResult<()>;
    /// get git project information
    async fn get_project(&self, id: &str) -> ImpResult<GitProject>;
    /// get information about a specific project branch
    async fn get_branch(&self, id: &str, branch: &str) -> ImpResult<GitBranch>;

    //async fn new_git_entry(&self, conf : &EntryConfig, entry : Entry) -> ImpResult<()> {
    //    let _entry = entry.finalize(&conf.fields)?;
    //    todo!("Not Implemented")
    //}
}

/// gitlab api client
#[derive(Clone,Debug)]
pub struct GitlabAPI {
    client : gitlab::AsyncGitlab
    //host: String,
    //#[serde(default)]
    //token: String,
}

impl GitlabAPI {
    /// constructor for gitlab client
    fn new(gitlab_api : gitlab::AsyncGitlab ) -> Self {
        Self { client : gitlab_api }
    }
}

/// gitlab backend api
#[async_trait::async_trait(?Send)]
impl BackendAPI for GitlabAPI {
    async fn new_entry(&mut self, entry_conf : &EntryConfig, entry : NewEntry) -> ImpResult<()> {
        let git_entry = entry_conf.render(entry)?;
        if git_entry.review {
            todo!("moderated git entry")
        } else {
            //return Err(ImpError::InternalError(("Debug Return",format!("{:?}",git_entry).into())))
            self.new_file(
                &git_entry.project_id,
                &git_entry.branch,
                &git_entry.file_path,
                &git_entry.serialize()?,
                &git_entry.commit_message
            ).await
        }
    }
}

impl From<gitlab::AsyncGitlab> for GitlabAPI {
    /// Create a new GitlabAPI from a [crate@gitlab] client
    fn from(client: gitlab::AsyncGitlab) -> Self {
        Self::new(client)
    }
}

/// gitlab git backend api
#[async_trait::async_trait(?Send)]
impl GitAPI for GitlabAPI {
    /// get the contents of a repo file
    async fn get_file(&self, project: &str, ref_: &str, path: &str) -> ImpResult<Vec<u8>> {
        let endpoint = gitlab::api::projects::repository::files::FileRaw::builder()
            .project(project)
            .ref_(ref_)
            .file_path(path)
            .build()
            .or_bad_request("Bad file spec")?;
        let file : Vec<u8> = gitlab::api::raw(endpoint).query_async(&self.client).await
            .or_bad_request("Gitlab get_file failed")?;
        Ok(file)
    }

    /// commit a new file to a repo branch
    async fn new_file(&self, project: &str, branch: &str, path: &str, content: &Vec<u8>, commit_message: &str) -> ImpResult<()> {
        let endpoint = gitlab::api::projects::repository::files::CreateFile::builder()
            .project(project)
            .branch(branch)
            .file_path(path)
            .content(content)
            .commit_message(commit_message)
            .build()
            .or_bad_request("Bad file spec")?;

        // Now we send the Query.
        //endpoint.query_async(&self.client).await
        gitlab::api::raw(endpoint).query_async(&self.client).await
            .or_bad_request("Gitlab new_file failed")?;
        Ok(())

        //test code -- so we can see the raw format
        //let response : Vec<u8> = gitlab::api::raw(endpoint).query_async(&client).await?;
    }

    /// get project information
    ///
    /// **TODO:** there is old commented code below for doing this, needs to be migrated here
    async fn get_project(&self, _id: &str) -> ImpResult<GitProject> {
        todo!("get_project")
    }

    /// get branch information
    ///
    /// **TODO:** there is old commented code below for doing this, needs to be migrated here
    async fn get_branch(&self, _id: &str, _branch: &str) -> ImpResult<GitBranch> {
        todo!("get_branch")
    }
}

//struct _Handlers {
//}
//impl _Handlers {
//    fn new_entry(config : &Config, entry_type : &str, backend : &str, entry : Entry) -> ImpResult<()> {

//        if let (Some(_backend),Some(conf)) = (
//            config.backends.get(backend),
//            config.entry_types.get(entry_type)
//        ) {
//            let _entry = entry.finalize(&conf.fields)?;
//            //backend.new_entry(entry)
//            todo!("Not Implemented")
//        } else {
//            Err("Unknown backend or type".into())
//        }
//    }
//}



/////////////////////////////////////////////////////////////////////////////////////////////////////////





//#[async_trait::async_trait(?Send)]
//trait _Backend {
//    async fn new_entry<T>(&self) -> T where T:NewEntry;
//}

////Backend - backend api trait for staticimp
//// - each backend api should implement this interface
//// - TODO: implement higher-level helper functions (e.g. commit file to new branch and create MR)
//// - TODO: instead of returning HttpResponse for the client, return the appropriate data
////   - then calling function can use the data
//// - TODO: refactor into BackendAPI and GitAPI (which implements BackendAPI)
////   - then we have the flexibility to support non-git backends in the future (e.g. database or web service)
//#[async_trait::async_trait(?Send)]
//trait _BackendAPI {
//    async fn add_file(&self, client: &awc::Client, id: &str, path: &str) -> ImpResult<actix_web::HttpResponse>;
//    async fn get_file(&self, client: &awc::Client, id: &str, path: &str) -> ImpResult<actix_web::HttpResponse>;
//    //async fn get_file(&self, id: &str, path: &str) -> ImpResult<bytes::Bytes>;
//    async fn get_project(&self, client: &awc::Client, id: &str) -> ImpResult<actix_web::HttpResponse>;
//    async fn get_branch(&self, client: &awc::Client, id: &str, branch: &str) -> ImpResult<actix_web::HttpResponse>;
//}


//#[async_trait::async_trait(?Send)]
//impl BackendAPI for Config {
//    async fn add_file(&self, client: &awc::Client, id: &str, path: &str) -> ImpResult<actix_web::HttpResponse> {
//        self.backend.add_file(client,id,path).await
//    }
//    async fn get_file(&self, client: &awc::Client, id: &str, path: &str) -> ImpResult<actix_web::HttpResponse> {
//        self.backend.get_file(client,id,path).await
//    }
//    async fn get_project(&self, client: &awc::Client, id: &str) -> ImpResult<actix_web::HttpResponse> {
//        self.backend.get_project(client,id).await
//    }
//    async fn get_branch(&self, client: &awc::Client, id: &str, branch: &str) -> ImpResult<actix_web::HttpResponse> {
//        self.backend.get_branch(client,id,branch).await
//    }
//}

////Backend - enum for backend apis
////  - variants are the supported backend apis
////  - BackendAPI implementation just passes through to the current variant
//#[derive(Clone,Debug, Serialize, Deserialize)]
//#[serde(tag = "driver")]
//enum Backend {
//    Gitlab(GitlabAPI)
//}

////TODO: map/list of Backends in Config (also implement proper serialization/deserialization ignoring client)
//#[async_trait::async_trait(?Send)]
//impl BackendAPI for Backend {
//    async fn add_file(&self, client: &awc::Client, id: &str, path: &str) -> ImpResult<actix_web::HttpResponse> {
//        match self {
//            Backend::Gitlab(backend) => backend.add_file(client,id,path).await
//        }
//    }
//    async fn get_file(&self, client: &awc::Client, id: &str, path: &str) -> ImpResult<actix_web::HttpResponse> {
//        match self {
//            Backend::Gitlab(backend) => backend.get_file(client,id,path).await
//        }
//    }
//    async fn get_project(&self, client: &awc::Client, id: &str) -> ImpResult<actix_web::HttpResponse> {
//        match self {
//            Backend::Gitlab(backend) => backend.get_project(client,id).await
//        }
//    }
//    async fn get_branch(&self, client: &awc::Client, id: &str, branch: &str) -> ImpResult<actix_web::HttpResponse> {
//        match self {
//            Backend::Gitlab(backend) => backend.get_branch(client,id,branch).await
//        }
//    }
//}

////GitlabAPI - implementation of the Gitlab REST api
////  - TODO: support oauth
////  - TODO: return appropriate data instead of client response (see BackendAPI)
//#[derive(Clone,Debug, Serialize, Deserialize)]
//struct _GitlabAPI {
//    host: String,
//    #[serde(default)]
//    token: String,
//}

////impl Clone for GitlabAPI {
////    fn clone(&self) -> Self {
////        GitL
////    }
////}

//impl GitlabAPI {
//    //pub fn new() -> Self {
//    //}
//}

////TODO: don't send full gitlab response (or have debug flag to enable)
////  - normally should send back higher level error
//#[async_trait::async_trait(?Send)]
//impl _BackendAPI for _GitlabAPI {
//    async fn add_file(&self, _client: &awc::Client, id: &str, path: &str) -> ImpResult<actix_web::HttpResponse> {
//        let client = gitlab::GitlabBuilder::new(self.host.as_str(),self.token.as_str()).build_async().await?;
//        let branch = "main";
//        let content : &[u8] = b"This is a test file\nand a second line";
//        let commit_message = "test create file from rust";
//        let endpoint = gitlab::api::projects::repository::files::CreateFile::builder()
//            .project(id)
//            .branch(branch)
//            .file_path(path)
//            .content(content)
//            .commit_message(commit_message).build()?;

//        //// Now we send the Query.
//        //endpoint.query_async(&client).await?;
//        //Ok(actix_web::HttpResponse::Ok().finish())

//        //FIXME: test code -- so we can see the raw format
//        let response : Vec<u8> = gitlab::api::raw(endpoint).query_async(&client).await?;
//        Ok(actix_web::HttpResponse::Ok().body(response))

//        //match result.status() {
//        //    awc::http::StatusCode::OK => Ok(actix_web::HttpResponse::Ok().body(rbody)),
//        //    _ => Ok(actix_web::HttpResponseBuilder::new(result.status()).body(rbody))
//        //}
//    }
//    async fn get_file(&self, _client: &awc::Client, id: &str, path: &str) -> ImpResult<actix_web::HttpResponse> {
//        let ref_ = "main";
//        let client = gitlab::GitlabBuilder::new(self.host.as_str(),self.token.as_str()).build_async().await?;
//        let endpoint = gitlab::api::projects::repository::files::FileRaw::builder()
//            .project(id)
//            .file_path(path)
//            .ref_(ref_).build()?;
//        let file : Vec<u8> = gitlab::api::raw(endpoint).query_async(&client).await?;
//        
//        Ok(actix_web::HttpResponse::Ok().body(file))
//        //match result.status() {
//        //    awc::http::StatusCode::OK => Ok(actix_web::HttpResponse::Ok().body(rbody)),
//        //    _ => Ok(actix_web::HttpResponseBuilder::new(result.status()).body(rbody))
//        //}
//    }
//    async fn get_project(&self, _client: &awc::Client, id: &str) -> ImpResult<actix_web::HttpResponse> {
//        let client = gitlab::GitlabBuilder::new(self.host.as_str(),self.token.as_str()).build_async().await?;
//        let endpoint = gitlab::api::projects::Project::builder()
//            .project(id).build()?;
//        let p : GitProject = endpoint.query_async(&client).await?;
//        let json = serde_json::to_string_pretty(&p)?;
//        Ok(actix_web::HttpResponse::Ok().body(json))
//        //match result.status() {
//        //    awc::http::StatusCode::OK => Ok(actix_web::HttpResponse::Ok().body(rbody)),
//        //    _ => Ok(actix_web::HttpResponseBuilder::new(result.status()).body(rbody))
//        //}
//    }
//    async fn get_branch(&self, _client: &awc::Client, id: &str, branch: &str) -> ImpResult<actix_web::HttpResponse> {
//        let client = gitlab::GitlabBuilder::new(self.host.as_str(),self.token.as_str()).build_async().await?;
//        let endpoint = gitlab::api::projects::repository::branches::Branch::builder()
//            .project(id)
//            .branch(branch).build()?;
//        let b : GitBranch = endpoint.query_async(&client).await?;
//        let json = serde_json::to_string_pretty(&b)?;
//        Ok(actix_web::HttpResponse::Ok().body(json))
//        //match result.status() {
//        //    awc::http::StatusCode::OK => Ok(actix_web::HttpResponse::Ok().body(rbody)),
//        //    _ => Ok(actix_web::HttpResponseBuilder::new(result.status()).body(rbody))
//        //}
//    }
//}

////#[derive(Debug, derive_more::Display, derive_more::Error)]
////enum CommentError {
////    #[display(fmt = "Bad comment request format")]
////    BadRequest,
////}
////impl actix_web::ResponseError for CommentError {
////    fn error_response(&self) -> actix_web::HttpResponse {
////	actix_web::HttpResponse::build(self.status_code())
////	    .insert_header(actix_web::http::header::ContentType::html())
////	    .body(self.to_string())
////    }
////
////    fn status_code(&self) -> actix_web::http::StatusCode {
////	match *self {
////	    CommentError::BadRequest => actix_web::http::StatusCode::BAD_REQUEST,
////	}
////    }
////}

////Comment - struct for holding a comment
//// - TODO: make this generic, so that fields can be customized at config level
//#[derive(Serialize,Deserialize)]
//pub struct Comment {
//    name: String,
//    //email: String,
//    message: String
//}
