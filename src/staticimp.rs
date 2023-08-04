//! Simple module for sending sets of fields to backend APIs
//!
//! staticimp takes entrys with fields, performs validation and transformations,
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
//! - configuration can use placeholders to fill in/transform entries
//!   - uses rendertemplate (in this crate) for rendering placeholders
//! - loads configuration from `staticman.yml`
//! - entries are validated by checking for allowed/required fields
//!   - doesn't yet support any formatting validation
//! - moderation (by submiting entry to review branch with MR)
//! - extra fields generated from config
//! - field transformations
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
//! - thorough test code
//! - logging
//! - spam protection (probably reCAPTCHA)
//! - github as a second backend
//! - I might include a filesystem backend for easier configuration
//! - specify allowed hosts for a backend
//!
//!
//! # Implemented Backends:
//!
//! **Debug**
//!
//! - [DebugConfig]
//! the Debug backend just returns ImpError::Debug with the processed entry
//!
//! This is mostly just for development and testing config files
//!
//! **Gitlab**
//!
//! - [GitlabAPI]
//! uses [gitlab::AsyncGitlab] to send files to gitlab
//!
//! - doesn't yet support review entries (i.e. placing entries in new branches), but the structure
//!   is in place and it should be implemented soon

//use actix_web::http::header::ContentType;
use crate::rendertemplate;
use actix_web::http::StatusCode;
use actix_web::HttpResponse;
use chrono::prelude::*;
use gitlab::api::projects::merge_requests::CreateMergeRequest;
use gitlab::api::projects::repository::branches::CreateBranch;
use gitlab::api::projects::repository::files::CreateFile;
use gitlab::api::AsyncQuery;
use markdown_table::MarkdownTable;
use md5;
use rendertemplate::render_str;
use rendertemplate::Render;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde::Serialize;
use sha256;
use slug::slugify;
use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Display;
use std::io;
use std::ops::Deref;
use uuid::Uuid;
//use std::cell::RefCell;
//use std::ops::Deref;
use SerializationFormat::{Json, Yaml};

type BoxError = Box<dyn std::error::Error>;

/// Module error
///
/// Implements [actix_web::ResponseError] so it can be returned directly from actix request handler
#[derive(Debug)]
pub enum ImpError {
    /// BadRequest with message and child error
    BadRequest(&'static str, BoxError),
    /// InternalServerError with message and child error
    InternalError(&'static str, BoxError),
    /// Debugging info (returns 200 OK)
    ///
    Debug(String),
}

/// ImpError constructors
#[allow(dead_code)]
impl ImpError {
    /// returns string for debugging (as an ImpError)
    pub fn debug<T>(val: T) -> Self
    where
        T: std::fmt::Display,
    {
        ImpError::Debug(val.to_string())
    }
    /// returns debug-print of object for debugging
    pub fn debug_dbg<T>(val: T) -> Self
    where
        T: std::fmt::Debug,
    {
        ImpError::Debug(format!("{:?}", val))
    }
    /// returns pretty-printed json object for debugging
    ///
    /// If serialization fails it returns the debug-print of the object
    pub fn debug_json<T>(val: T) -> Self
    where
        T: std::fmt::Debug + Serialize,
    {
        Json.serialize(&val)
            .and_then(|s| Ok(ImpError::Debug(s)))
            .unwrap_or_else(|e| e)
    }
    /// returns yaml object for debugging info
    ///
    /// If serialization fails it returns the debug-print of the object
    pub fn debug_yaml<T>(val: T) -> Self
    where
        T: std::fmt::Debug + Serialize,
    {
        Yaml.serialize(&val)
            .and_then(|s| Ok(ImpError::Debug(s)))
            .unwrap_or_else(|e| e)
    }
}

/// Display formatting for ImpError
impl Display for ImpError {
    /// format error to string
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let fmt_msg = |s: &str| {
            let s = s.to_string();
            if s.is_empty() {
                s
            } else {
                s + ": "
            }
        };
        use ImpError::*;
        match self {
            BadRequest(s, e) => write!(f, "{}{}", fmt_msg(s), e.to_string()),
            InternalError(s, e) => write!(f, "{}{}", fmt_msg(s), e.to_string()),
            Debug(s) => write!(f, "{}", s),
        }
    }
}

/// [actix_web::ResponseError] implementation so ImpErrors can be directly returned to actix handler
///
/// returns [actix_web::HttpResponse] containing error string
/// - status code based on variant (most of the variant names are obvious)
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
            BadRequest(_, _) => StatusCode::BAD_REQUEST,
            InternalError(_, _) => StatusCode::INTERNAL_SERVER_ERROR,
            Debug(_) => StatusCode::OK,
        }
    }
}

/// trait for converting other [Result] types into [ImpResult]
pub trait OrImpResult<T> {
    /// returns Ok or [ImpError::BadRequest]
    fn or_bad_request(self, message: &'static str) -> ImpResult<T>;

    /// returns Ok or [ImpError::InternalError]
    fn or_internal_error(self, message: &'static str) -> ImpResult<T>;
}

/// converts any [`Result<T,E>`] into [`ImpError<T>`]
///
/// E must implement [std::error::Error] (per ImpError)
impl<T, E> OrImpResult<T> for Result<T, E>
where
    E: std::error::Error + 'static,
{
    fn or_bad_request(self, message: &'static str) -> ImpResult<T> {
        self.or_else(|e| -> Result<T, ImpError> { Err(ImpError::BadRequest(message, Box::new(e))) })
    }

    fn or_internal_error(self, message: &'static str) -> ImpResult<T> {
        self.or_else(|e| Err(ImpError::InternalError(message, e.into())))
    }
}

/// module Result
///
/// all Result-returning functions return ImpError
/// - this helps with cleaner code in actix handler since we can use ?
pub type ImpResult<R> = Result<R, ImpError>;

/// Transformation to apply to a field
#[derive(Clone, Debug, Serialize, Deserialize)]
struct FieldTransform {
    field: String,
    transform: FieldTransformType,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
enum FieldTransformType {
    #[serde(rename = "slugify")]
    Slugify,
    #[serde(rename = "md5")]
    Md5,
    #[serde(rename = "sha256")]
    Sha256,
}

/// Field to generate
///
/// This also acts as the builder for generated fields (using [GeneratedField::render])
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum GeneratedField {
    Value(String),
}

/// Renders a generated field
impl Render<&NewEntry, ImpResult<String>> for GeneratedField {
    /// create generated field for NewEntry
    ///
    /// currently just replaces placeholders in self.value
    fn render(&self, entry: &NewEntry) -> ImpResult<String> {
        match self {
            GeneratedField::Value(val) => Ok(render_str(&val, entry)),
        }
    }
}

/// Field validation and mutation rules for entry
///
/// - `allowed` - list of fields that are allowed to be in an entry
/// - `required` - fields that must exist in the entry
/// - `extra` - fields to generate and add to entry
/// - `transforms` - transformations to apply to entry fields
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct FieldConfig {
    #[serde(default)]
    allowed: HashSet<String>,
    #[serde(default)]
    required: HashSet<String>,
    #[serde(default)]
    extra: HashMap<String, GeneratedField>,
    #[serde(default)]
    transforms: Vec<FieldTransform>,
}

/// Serialization format
///
/// defaults to yaml
///
/// the member functions on SerializationFormat support convenient (de)serialization functions
#[derive(Default, Copy, Clone, Debug, Serialize, Deserialize)]
pub enum SerializationFormat {
    /// json serialization (using serde_json)
    #[serde(rename = "json")]
    Json,

    /// yaml serialization (using serde_yaml)
    #[serde(rename = "yaml", alias = "yml")]
    #[default]
    Yaml,
}

/// serialization functions
impl SerializationFormat {
    /// serialize object to string
    pub fn serialize<T>(&self, val: &T) -> ImpResult<String>
    where
        T: Serialize,
    {
        let serialized = match self {
            Json => serde_json::to_string(&val).or_bad_request("Bad json output")?,
            Yaml => serde_yaml::to_string(&val).or_bad_request("Bad yaml output")?,
        };
        Ok(serialized)
    }

    /// serialize object to string
    pub fn serialize_pretty<T>(&self, val: &T) -> ImpResult<String>
    where
        T: Serialize,
    {
        let serialized = match self {
            Json => serde_json::to_string_pretty(&val).or_bad_request("Bad json output")?,
            Yaml => serde_yaml::to_string(&val).or_bad_request("Bad yaml output")?,
        };
        Ok(serialized)
    }

    ///// deserialize object from &str
    //fn from_str<'a,T>(&self, serialized : &'a str) -> ImpResult<T>
    //where
    //    T : Deserialize<'a>
    //{
    //    let val = match self {
    //        Json => serde_json::from_str(&serialized)
    //            .or_internal_error("Bad json input")?,
    //        Yaml => serde_yaml::from_str(&serialized)
    //            .or_internal_error("Bad yaml input")?
    //    };
    //    Ok(val)
    //}
    /// deserialize object from slice
    pub fn deserialize_slice<'a, T>(&self, serialized: &'a [u8]) -> ImpResult<T>
    where
        T: Deserialize<'a>,
    {
        let val = match self {
            Json => serde_json::from_slice(&serialized).or_internal_error("Bad json input")?,
            Yaml => serde_yaml::from_slice(&serialized).or_internal_error("Bad yaml input")?,
        };
        Ok(val)
    }
    /// deserialize object from reader
    pub fn deserialize_reader<T, R>(&self, rdr: R) -> ImpResult<T>
    where
        R: io::Read,
        T: DeserializeOwned,
    {
        match self {
            Json => serde_json::from_reader(rdr).or_internal_error("Bad json input"),
            Yaml => serde_yaml::from_reader(rdr).or_internal_error("Bad yaml input"),
        }
    }
    /// determine [SerializationFormat] based on path
    ///
    /// rules:
    /// - if path ends in ".json", assume json
    /// - else assume/default to yaml
    pub fn from_path(path: &str) -> Self {
        if path.ends_with(".json") {
            Json
        } else {
            Yaml
        }
    }
}

/// Git-specific entry config
///
/// placeholders are allowed so configuration values can be pulled from entry fields and query
/// parameters
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GitEntryConfig {
    /// Directory path to store entry under
    #[serde(default = "GitEntryConfig::default_path")]
    path: String,
    /// Filename to use for entry
    #[serde(default = "GitEntryConfig::default_filename")]
    filename: String,
    /// Branch to send entries to (or submit merge request for)
    #[serde(default = "GitEntryConfig::default_branch")]
    branch: String,
    /// name of review branch for commit (when review enabled)
    #[serde(default = "GitEntryConfig::default_review_branch")]
    review_branch: String,
    /// merge request description (when review enabled)
    #[serde(default = "GitEntryConfig::default_mr_description")]
    mr_description: String,
    /// Git commit message
    #[serde(default = "GitEntryConfig::default_commit_message")]
    commit_message: String,
}

impl GitEntryConfig {
    /// default entry path ( "data/entries")
    fn default_path() -> String {
        "data/entries".to_string()
    }
    /// default entry filename ( "comment-{@timestamp}.yml" )
    fn default_filename() -> String {
        "entry-{@timestamp}.yml".to_string()
    }
    /// default branch to send files to ( "main" )
    fn default_branch() -> String {
        "main".to_string()
    }
    /// default review branch ( "staticimp_{@id}" )
    fn default_review_branch() -> String {
        "staticimp_{@id}".to_string()
    }
    /// default merge request description
    fn default_mr_description() -> String {
        "new staticimp entry awaiting approval\n\nMerge the pull request to accept it, or close it"
            .to_string()
    }
    /// default commit message
    fn default_commit_message() -> String {
        "New staticimp entry".to_string()
    }
}

/// configuration for new entry
///
/// This also acts as the builder for [NewEntry]s (using [EntryConfig::render])
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct EntryConfig {
    /// Whether this entry type is disabled
    #[serde(default)]
    pub disabled: bool,
    /// return processed entry instead of sending to backend
    #[serde(default)]
    pub debug: bool,
    /// Configuration for entry fields
    #[serde(default)]
    fields: FieldConfig,
    /// Whether moderation is enabled
    #[serde(default)]
    review: bool,
    ///// Whether recaptcha is enabled
    //#[serde(default)]
    //recaptcha: bool,
    /// entry serialization format
    #[serde(default)]
    format: SerializationFormat,
    /// Git-specific entry config
    ///
    /// - its an option so a single entry type can support multiple backends
    git: Option<GitEntryConfig>,
}

impl EntryConfig {
    /// get the configuration for entry fields
    pub fn field_config(&self) -> &FieldConfig {
        &self.fields
    }
    pub fn validate_branch(&self, branch: &str) -> bool {
        if let Some(git_config) = &self.git {
            git_config.branch.is_empty() || git_config.branch == branch
        } else {
            true
        }
    }
}

/// BackendAPI is interface staticimp uses to talk to backends
#[async_trait::async_trait(?Send)]
pub trait BackendAPI {
    /// send a processed entry to the backend
    ///
    /// - `entry_conf` - entry conf to use
    /// - `entry` - entry to send to backend
    async fn new_entry(&mut self, entry_conf: &EntryConfig, entry: NewEntry) -> ImpResult<()>;
    /// get project-specific entry config
    ///
    /// - `config` - global config
    /// - `project_id` - backend project to get conf for
    /// - `ref_` - backend ref (e.g. branch)
    async fn get_conf(
        &mut self,
        config: &BackendConfig,
        project_id: &str,
        ref_: &str,
    ) -> ImpResult<Option<ProjectConfig>>;
    //async fn get_entry(&self, id: &str, path: &str) -> ImpResult<EntryFields>;
}

/// Gitlab backend configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GitlabConfig {
    /// gitlab host url (without leading https://)
    host: String,
    /// token to authenticate with gitlab
    #[serde(default)]
    token: String,
}

impl GitlabConfig {
    /// create a new api client
    async fn new_client(&self) -> ImpResult<GitlabAPI> {
        let client = gitlab::GitlabBuilder::new(self.host.as_str(), self.token.as_str())
            .build_async()
            .await
            .or_internal_error("Failed to open client")?;
        Ok(GitlabAPI::new(client))
    }
}

/// backend for debugging staticimp and config (returns debug info to client)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DebugConfig {}

#[async_trait::async_trait(?Send)]
impl BackendAPI for DebugConfig {
    /// debug new_entry -- just returns entry_conf and processed entry fields to client
    async fn new_entry(&mut self, entry_conf: &EntryConfig, entry: NewEntry) -> ImpResult<()> {
        Err(ImpError::debug(format!(
            "# Entry Config:\n{}\n\n# Processed Entry:\n{}\n",
            Yaml.serialize(&entry_conf)?,
            entry_conf.format.serialize_pretty(&entry)?
        )))
    }
    async fn get_conf(
        &mut self,
        _config: &BackendConfig,
        _project_id: &str,
        _ref_: &str,
    ) -> ImpResult<Option<ProjectConfig>> {
        //Err(ImpError::debug_json((config, project_id, ref_)))
        Ok(None)
    }
}

/// enum of backend specific configuration variants
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "driver")]
pub enum DriverConfig {
    /// gitlab backend configuration
    #[serde(rename = "gitlab")]
    Gitlab(GitlabConfig),
    /// debug backend configuration
    #[serde(rename = "debug")]
    Debug(DebugConfig),
}

/// Backend configuration
/// - contains both shared config values and backend-specific values
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BackendConfig {
    /// path to project-specific configuration file
    #[serde(default)]
    project_config_path: String, //empty -- global server conf only

    /// serialization type for project config (default: None)
    ///
    /// if None, guesses based on project_config_path (defaulting to yaml)
    #[serde(default)]
    project_config_format: Option<SerializationFormat>,

    /// Driver specific config settings
    ///
    /// In config file these get flattened into the backend (since they shouldn't overlap with
    /// driver specific settings)
    #[serde(flatten)]
    driver: DriverConfig,
}

impl BackendConfig {
    /// creates a new client from the backend configuration
    ///
    /// for Gitlab it creates a new api client
    ///
    /// for Debug it just clones the debug config
    pub async fn new_client(&self) -> ImpResult<Backend> {
        match &self.driver {
            DriverConfig::Gitlab(conf) => {
                let client = conf.new_client().await?;
                Ok(Backend::Gitlab(client))
            }
            DriverConfig::Debug(conf) => Ok(Backend::Debug(conf.clone())),
        }
    }

    fn format(&self) -> SerializationFormat {
        if let Some(format) = self.project_config_format {
            format
        } else {
            SerializationFormat::from_path(&self.project_config_path)
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
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    /// set of backend configurations
    #[serde(default)]
    pub backends: HashMap<String, BackendConfig>,
    /// host to listen on
    #[serde(default = "Config::default_host")]
    pub host: String,
    /// port to listen on
    #[serde(default = "Config::default_port")]
    pub port: u16,
    /// format used for `{@timestamp}` placeholders
    /// - this gets stored in [NewEntry.timestamp_str] at creation
    #[serde(default = "Config::default_timestamp_format")]
    timestamp_format: String,
    /// configuration for each entry type
    #[serde(default)]
    pub entries: HashMap<String, EntryConfig>,
}

impl Config {
    /// Load configuration file
    ///
    /// deserializes Config using serde_yaml
    pub fn load(path: &str, format: SerializationFormat) -> ImpResult<Self> {
        let f = std::fs::File::open(path).or_internal_error("Couldn't open config file")?;

        format.deserialize_reader(f)
    }

    /// override config parameters from environment variables
    ///
    /// takes and returns self, since we currently always call it after loading
    /// - makes for clean code (see examples)
    ///
    /// Supported overrides:
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
        let env_override = |var: &mut String, varname: &str| {
            let val = std::env::var(varname).unwrap_or("".to_owned());
            if !val.is_empty() {
                *var = val;
            }
        };

        //env_override(&mut self.timestamp_format, "timestamp_format");

        for (name, backend) in self.backends.iter_mut() {
            match &mut backend.driver {
                DriverConfig::Gitlab(gitlab) => {
                    env_override(&mut gitlab.host, &(name.clone() + "_host"));
                    env_override(&mut gitlab.token, &(name.clone() + "_token"));
                }
                DriverConfig::Debug(_) => {}
            }
        }
        self
    }

    /// default host (interface) to listen on
    fn default_host() -> String {
        "127.0.0.1".to_string()
    }

    /// default port to listen on
    fn default_port() -> u16 {
        8080
    }

    /// default Timestamp format (compact ISO8601 with milliseconds)
    fn default_timestamp_format() -> String {
        "%Y%m%dT%H%M%S%.3fZ".to_string()
    }

    /// build a [NewEntry]
    ///
    /// takes path and query paramters plus entry fields
    pub fn new_entry(
        &self,
        project_id: String,
        branch: String,
        fields: EntryFields,
        params: HashMap<String, String>,
    ) -> NewEntry {
        NewEntry::new(self, project_id, branch, fields, params)
    }
}

/// Project-specific config (project entry types)
///
/// This is loaded from project_config_path for each project (if the config value is set)
///
/// project config consists of a list of entry types.
/// - these override same-name global entries
/// - global entry types (that haven't been overriden) are still available
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectConfig {
    /// project-specific entry types
    ///
    /// project entries override global config entry types
    pub entries: HashMap<String, EntryConfig>,
}

/// staticimp entry fields
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct EntryFields {
    /// entry eields
    #[serde(flatten)]
    fields: HashMap<String, String>,
}

impl Deref for EntryFields {
    type Target = HashMap<String, String>;

    fn deref(&self) -> &Self::Target {
        &self.fields
    }
}

impl EntryFields {
    /// serialize entry for sending to backend
    fn serialize(&self, format: SerializationFormat) -> ImpResult<Vec<u8>> {
        Ok(format.serialize(&self)?.as_bytes().into())
    }
}

/// builder for sending a new entry to the backend
#[derive(Clone, Debug, Serialize)]
pub struct GitEntry {
    /// id for the project to send this entry to
    project_id: String,
    /// branch to write the entry to (or submit MR for)
    branch: String,
    /// path to write the entry to
    file_path: String,
    /// entry content
    fields: EntryFields,
    /// commit message for entry
    commit_message: String,
    /// review branch name (if review enabled)
    review_branch: Option<String>,
    /// merge request description (if review enabled)
    mr_description: Option<String>,
    /// serialization format to use
    format: SerializationFormat,
}

impl GitEntry {
    /// serialize entry fields per entry config
    fn serialize(&self) -> ImpResult<Vec<u8>> {
        self.fields.serialize(self.format)
    }
}

/// new entry with all context needed for processing placeholders
#[derive(Default, Clone, Debug, Serialize)]
pub struct NewEntry {
    /// uuid for entry
    uid: String,
    /// timetamp for entry
    timestamp: DateTime<Utc>,
    /// prerendered default timestamp string
    timestamp_str: String,
    /// project id (for gitlab could be name/path OR numeric id)
    project_id: String,
    /// project branch
    branch: String,
    /// entry fields
    fields: EntryFields,
    /// params attached to request (HTTP query parameterss)
    params: HashMap<String, String>,
    //special : &'a HashMap<&'a str, String>,
}

impl NewEntry {
    /// build new entry context to fill in placeholders
    fn new(
        config: &Config,
        project_id: String,
        branch: String,
        fields: EntryFields,
        params: HashMap<String, String>,
    ) -> Self {
        let uid = Uuid::new_v4().to_string();
        let timestamp = Utc::now();
        let timestamp_str = format!("{}", timestamp.format(&config.timestamp_format));
        NewEntry {
            uid,
            timestamp,
            timestamp_str,
            project_id,
            branch,
            fields,
            params,
            //special : HashMap::from([
            //    ( "@id", uid )
            //])
        }
    }

    /// render a formatted data (from `{date:format}` placeholders)
    ///
    /// - Uses [chrono::format::strftime] for formatting
    fn render_date(&self, fmt: &str) -> String {
        format!("{}", self.timestamp.format(fmt))
    }

    /// validate fields in entry
    fn validate_fields(self, conf: &FieldConfig) -> ImpResult<Self> {
        //collect field keys used in entry
        let keys: HashSet<String> = self.fields.keys().map(|s| s.to_string()).collect();
        if !conf.required.is_subset(&keys) {
            //make sure all required keys are in entry
            Err(ImpError::BadRequest("", "Missing field(s)".into()))
        } else if !keys.is_subset(&conf.allowed) {
            //make sure only allowed keys are used
            Err(ImpError::BadRequest("", "Unknown field(s)".into()))
        } else {
            // passed all validation requests, return self
            Ok(self)
        }
    }

    /// Generate extra fields
    fn generate_fields<'a, I>(mut self, fields: I) -> ImpResult<Self>
    where
        I: IntoIterator<Item = (&'a String, &'a GeneratedField)>,
    {
        for (key, gen) in fields {
            let val = gen.render(&self)?;
            self.fields.fields.insert(key.to_string(), val);
        }
        Ok(self)
    }

    /// Transform fields
    fn transform_fields<'a, I>(mut self, transforms: I) -> ImpResult<Self>
    where
        I: IntoIterator<Item = &'a FieldTransform>,
    {
        for t in transforms {
            if let Some(field) = self.fields.fields.get_mut(&t.field) {
                use FieldTransformType::*;
                *field = match t.transform {
                    Slugify => slugify(&field),
                    Md5 => format!("{:x}", md5::compute(&field)),
                    Sha256 => sha256::digest(field.as_str()),
                }
            }
        }
        Ok(self)
    }

    /// Process entry fields
    ///
    /// Processing Order:
    /// 1. validation
    ///   - make sure only allowed fields are used and all required fields are present
    /// 2. extra fields
    ///   - generated fields
    /// 3. transformations
    ///   - list of [FieldTransform]s
    pub fn process_fields(self, conf: &FieldConfig) -> ImpResult<Self> {
        self.validate_fields(&conf)?
            .generate_fields(&conf.extra)?
            .transform_fields(&conf.transforms)
    }
}

/// placeholder rendering for entry processing
///
/// renders entry processing placeholders to a [Cow]
///
/// returns [Cow::Borrowed] for everything but formatted dates
/// - `{@timestamp}` is prerenderedon entry creation
///
/// missing placeholders are collapsed (render to empty string)
impl<'a> Render<&str, Option<Cow<'a, str>>> for &'a NewEntry {
    /// renders an entry field or config value for a NewEntry
    ///
    /// return value is `Option<Cow>`
    /// - borrowed from entry for most placeholders
    /// - owned for formatted dates
    /// - returns empty string for unknown placeholders
    ///
    /// - `placeholder` - the placeholder to render
    fn render(&self, placeholder: &str) -> Option<Cow<'a, str>> {
        if placeholder.starts_with('@') {
            //special generated vars
            //self.special.get(placeholder).map_or(&"",|v| &v)
            Some(if placeholder == "@id" {
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
            })
        } else {
            if let Some((lhs, rhs)) = placeholder.split_once('.') {
                if lhs == "fields" {
                    self.fields
                        .get(rhs)
                        .and_then(|v| Some(Cow::Borrowed(v.as_str())))
                } else if lhs == "params" {
                    self.params
                        .get(rhs)
                        .and_then(|val| Some(Cow::Borrowed(val.as_str())))
                } else {
                    None
                }
            } else {
                None
            }
        }
    }
}

/// Builder for [GitEntry]s from [NewEntry]s
///
/// builds git-specific entry from config and NewEntry
impl Render<NewEntry, ImpResult<GitEntry>> for EntryConfig {
    /// build GitEntry from NewEntry context
    fn render(&self, entry: NewEntry) -> ImpResult<GitEntry> {
        if entry.branch.is_empty() {
            Err(ImpError::BadRequest("", "Must specify branch".into()))
        } else if let Some(gitconf) = self.git.as_ref() {
            let branch: String = render_str(&gitconf.branch, &entry);
            if !branch.is_empty() && branch != entry.branch {
                Err(ImpError::BadRequest("", "Branch not allowed".into()))
            } else {
                use std::path::Path;
                let file_path: String = render_str(&gitconf.path, &entry);
                let filename: String = render_str(&gitconf.filename, &entry);
                let file_path = Path::new(&file_path)
                    .join(&filename)
                    .to_str()
                    .ok_or_else(|| ImpError::BadRequest("", "Bad entry path".to_string().into()))?
                    .to_string();
                let branch = render_str(&gitconf.branch, &entry);
                let commit_message = render_str(&gitconf.commit_message, &entry);

                //if review is set,
                let (review_branch, mr_description) = if self.review {
                    // create markdown table with entry fields for mr description (to make review easier)
                    let entry_table = MarkdownTable::new(
                        entry
                            .fields
                            .iter()
                            .map(|(&ref k, &ref v)| vec![k, v])
                            .collect(),
                    )
                    .with_headings(vec![
                        markdown_table::Heading::new("Field".into(), None),
                        markdown_table::Heading::new("Content".into(), None),
                    ])
                    .as_markdown()
                    .or_else(|e| {
                        Err(ImpError::InternalError(
                            "failed to create markdown table",
                            e.to_string().into(),
                        ))
                    })?;

                    //get conf mr_description
                    let mr_description: String = render_str(&gitconf.mr_description, &entry);
                    //append entry table to mr_description
                    let mr_description = format!("{}\n\n{}", mr_description, entry_table);
                    (
                        Some(render_str(&gitconf.review_branch, &entry)),
                        Some(mr_description),
                    )
                } else {
                    (None, None)
                };

                // destructure entry so we can move instead of cloning fields
                let NewEntry {
                    project_id, fields, ..
                } = entry;

                Ok(GitEntry {
                    project_id,
                    branch,
                    file_path,
                    fields,
                    commit_message,
                    review_branch,
                    mr_description,
                    format: self.format,
                })
            }
        } else {
            Err(ImpError::BadRequest(
                "",
                "missing git entry configuration".into(),
            ))
        }
    }
}

/// Backend enum (variants represent the supported backends)
pub enum Backend {
    Gitlab(GitlabAPI),
    Debug(DebugConfig),
}

/// Backend enum dispatch to backend type
#[async_trait::async_trait(?Send)]
impl BackendAPI for Backend {
    /// send a new entry to the backend
    async fn new_entry(&mut self, entry_conf: &EntryConfig, entry: NewEntry) -> ImpResult<()> {
        match self {
            Backend::Gitlab(api) => api.new_entry(&entry_conf, entry),
            Backend::Debug(conf) => conf.new_entry(&entry_conf, entry),
        }
        .await
    }
    async fn get_conf(
        &mut self,
        config: &BackendConfig,
        project_id: &str,
        ref_: &str,
    ) -> ImpResult<Option<ProjectConfig>> {
        match self {
            Backend::Gitlab(api) => api.get_conf(config, project_id, ref_),
            Backend::Debug(conf) => conf.get_conf(config, project_id, ref_),
        }
        .await
    }
}

/// represents git commit from backend api
///
/// it only includes the fields we actually care about, not all available
#[derive(Clone, Debug, Serialize, Deserialize)]
struct GitCommit {
    id: String,
}

/// represents git branch from backend api
///
/// it only includes the fields we actually care about, not all available
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GitBranch {
    name: String,
    commit: GitCommit,
}

/// represents git project from backend api
///
/// it only includes the fields we actually care about, not all available
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GitProject {
    id: u32,
    name: String,
    path: String,
    #[serde(rename = "path_with_namespace")]
    full_path: String,
}

/// git-specific backend api
#[async_trait::async_trait(?Send)]
pub trait GitAPI {
    /// get repo file contents for given ref
    async fn get_file_raw(&self, project: &str, ref_: &str, path: &str) -> ImpResult<Vec<u8>>;
    /// commit a new file to the repo
    ///
    /// - `project` - git project id/path
    /// - `branch` - branch to create file in
    /// - `path` - path of file to create
    /// - `content` - contents of new file
    /// - `commit_message` - commit message for creating new file
    async fn new_file(
        &self,
        project: &str,
        branch: &str,
        path: &str,
        content: &Vec<u8>,
        commit_message: &str,
    ) -> ImpResult<()>;
    /// create a new branch
    ///
    /// - `project` - git project id/path
    /// - `branch` - branch to create
    /// - `ref_` - branch/ref to create new branch from
    async fn new_branch(&self, project: &str, branch: &str, ref_: &str) -> ImpResult<()>;
    /// create a merge request
    ///
    /// - `project` - git project id/path
    /// - `branch` - branch to merge into
    /// - `ref_` - branch/ref to merge from
    /// - `description` - MR description
    async fn new_merge_request(
        &self,
        project: &str,
        souce_branch: &str,
        target_branch: &str,
        title: &str,
        description: &str,
    ) -> ImpResult<()>;
    /// get git project information
    ///
    /// - `project` - git project id/path
    async fn get_project(&self, project: &str) -> ImpResult<GitProject>;

    /// get information about a specific project branch
    /// - `project` - git project id/path
    /// - `branch` - git branch to look up
    async fn get_branch(&self, project: &str, branch: &str) -> ImpResult<GitBranch>;

    /// Create file in a new branch and create merge request
    ///
    /// - `project` - git project id/path
    /// - `review_branch` - branch to create for new file
    /// - `target_branch` - target branch for merge request
    /// - `path` - path of file to create
    /// - `content` - content of new file
    /// - `commit_message` - commit message for adding new file
    /// - `mr_description` - merge request description
    async fn new_file_mr(
        &self,
        project: &str,
        branch: &str,
        review_branch: &str,
        path: &str,
        content: &Vec<u8>,
        commit_message: &str,
        mr_description: &str,
    ) -> ImpResult<()> {
        self.new_branch(&project, &review_branch, &branch).await?;
        self.new_file(&project, &review_branch, &path, &content, &commit_message)
            .await?;
        self.new_merge_request(
            &project,
            &review_branch,
            &branch,
            &commit_message,
            &mr_description,
        )
        .await?;
        Ok(())
    }

    /// get deserialized file
    async fn get_file<'a, T: 'a + DeserializeOwned>(
        &self,
        project: &str,
        ref_: &str,
        path: &str,
        format: SerializationFormat,
    ) -> ImpResult<T> {
        format.deserialize_slice(&self.get_file_raw(project, ref_, path).await?)
    }
}

/// gitlab api client
#[derive(Clone, Debug)]
pub struct GitlabAPI {
    client: gitlab::AsyncGitlab, //host: String,
                                 //#[serde(default)]
                                 //token: String,
}

impl GitlabAPI {
    /// constructor for gitlab client
    fn new(gitlab_api: gitlab::AsyncGitlab) -> Self {
        Self { client: gitlab_api }
    }
}

/// gitlab backend api
#[async_trait::async_trait(?Send)]
impl BackendAPI for GitlabAPI {
    /// create a new entry by commiting file to repo
    async fn new_entry(&mut self, entry_conf: &EntryConfig, entry: NewEntry) -> ImpResult<()> {
        let git_entry = entry_conf.render(entry)?; //create GitEntry from entry
        if entry_conf.debug {
            return Err(ImpError::debug(format!(
                "# Entry Config:\n{}\n\n# Processed Entry:\n{}\n",
                Yaml.serialize(&entry_conf)?,
                git_entry.format.serialize_pretty(&git_entry)?
            )));
        }
        if let Some(review_branch) = git_entry.review_branch.as_ref() {
            let mr_description = git_entry.mr_description.as_ref().unwrap();
            self.new_file_mr(
                &git_entry.project_id,
                &git_entry.branch,
                &review_branch,
                &git_entry.file_path,
                &git_entry.serialize()?,
                &git_entry.commit_message,
                &mr_description,
            )
            .await
        } else {
            //return Err(ImpError::InternalError(("Debug Return",format!("{:?}",git_entry).into())))
            self.new_file(
                &git_entry.project_id,
                &git_entry.branch,
                &git_entry.file_path,
                &git_entry.serialize()?,
                &git_entry.commit_message,
            )
            .await
        }
    }
    /// get project-specific gitlab backend config
    async fn get_conf(
        &mut self,
        config: &BackendConfig,
        project_id: &str,
        ref_: &str,
    ) -> ImpResult<Option<ProjectConfig>> {
        if config.project_config_path.is_empty() {
            Ok(None)
        } else {
            //get deserialized conf from backend
            self.get_file(
                project_id,
                ref_,
                &config.project_config_path,
                config.format(),
            )
            .await
            .and_then(|conf| Ok(Some(conf)))
        }
    }
}

impl From<gitlab::AsyncGitlab> for GitlabAPI {
    /// Create a new GitlabAPI from [gitlab::AsyncGitlab] client
    fn from(client: gitlab::AsyncGitlab) -> Self {
        Self::new(client)
    }
}

/// gitlab git backend api
#[async_trait::async_trait(?Send)]
impl GitAPI for GitlabAPI {
    /// get the contents of a repo file
    ///
    /// - `project` - git project id
    /// - `ref_` - branch / commit / tag
    /// - `path` - path of file to retrieve
    async fn get_file_raw(&self, project: &str, ref_: &str, path: &str) -> ImpResult<Vec<u8>> {
        let endpoint = gitlab::api::projects::repository::files::FileRaw::builder()
            .project(project)
            .ref_(ref_)
            .file_path(path)
            .build()
            .or_bad_request("Bad file spec")?;
        let file: Vec<u8> = gitlab::api::raw(endpoint)
            .query_async(&self.client)
            .await
            .or_bad_request("Gitlab get_file_raw failed")?;
        Ok(file)
    }

    /// commit a new file to the repo
    ///
    /// - `project` - git project id
    /// - `branch` - branch to commit file to
    /// - `path` - path to new file
    /// - `content` - content of new file (raw bytes)
    /// - `commit_message` - commit message for adding new file
    async fn new_file(
        &self,
        project: &str,
        branch: &str,
        path: &str,
        content: &Vec<u8>,
        commit_message: &str,
    ) -> ImpResult<()> {
        let endpoint = CreateFile::builder()
            .project(project)
            .branch(branch)
            .file_path(path)
            .content(content)
            .commit_message(commit_message)
            .build()
            .or_bad_request("Bad file spec")?;

        // Now we send the Query.
        //endpoint.query_async(&self.client).await
        gitlab::api::raw(endpoint)
            .query_async(&self.client)
            .await
            .or_bad_request("Gitlab new_file failed")?;
        Ok(())

        //test code -- so we can see the raw format
        //let response : Vec<u8> = gitlab::api::raw(endpoint).query_async(&client).await?;
    }

    /// create new branch
    ///
    async fn new_branch(&self, project: &str, branch: &str, ref_: &str) -> ImpResult<()> {
        //Err(ImpError::debug_json((project,branch,ref_)))
        let endpoint = CreateBranch::builder()
            .project(project)
            .branch(branch)
            .ref_(ref_)
            .build()
            .or_internal_error("Bad branch spec")?;

        gitlab::api::raw(endpoint)
            .query_async(&self.client)
            .await
            .or_bad_request("Gitlab new_branch failed")?;
        Ok(())
    }
    async fn new_merge_request(
        &self,
        project: &str,
        source_branch: &str,
        target_branch: &str,
        title: &str,
        description: &str,
    ) -> ImpResult<()> {
        let endpoint = CreateMergeRequest::builder()
            .project(project)
            .remove_source_branch(true)
            .source_branch(source_branch)
            .target_branch(target_branch)
            .title(title)
            .description(description)
            .build()
            .or_internal_error("Bad MR spec")?;

        gitlab::api::raw(endpoint)
            .query_async(&self.client)
            .await
            .or_bad_request("Gitlab new_merge_request failed")?;
        Ok(())
    }

    /// get project information
    ///
    /// - see [Gitlab Projects API](https://docs.gitlab.com/ee/api/projects.html) for other
    ///   response fields that could be collected
    async fn get_project(&self, project: &str) -> ImpResult<GitProject> {
        let endpoint = gitlab::api::projects::Project::builder()
            .project(project)
            .build()
            .or_internal_error("Bad project spec")?;
        endpoint
            .query_async(&self.client)
            .await
            .or_bad_request("Gitlab get_project failed")
    }

    /// get branch information
    ///
    /// - see [Gitlab Branches API](https://docs.gitlab.com/ee/api/branches.html) for other
    ///   response fields that could be collected
    async fn get_branch(&self, project: &str, branch: &str) -> ImpResult<GitBranch> {
        let endpoint = gitlab::api::projects::repository::branches::Branch::builder()
            .project(project)
            .branch(branch)
            .build()
            .or_internal_error("Bad branch spec")?;
        endpoint
            .query_async(&self.client)
            .await
            .or_bad_request("Gitlab get_branch failed")
    }
}
