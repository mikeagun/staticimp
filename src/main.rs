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
//
//TODO: make errors more specific where reasonable

//struct Config {
//    gitlab_token: Option<String>
//}
//static CONFIG: once_cell::sync::Lazy<Config> = once_cell::sync::Lazy::new(|| Config {
//    gitlab_token: match std::env::var("GITLAB_TOKEN") {
//        Ok(val) => Some(val),
//        Err(_) => None
//    }
//});

mod staticimp {
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
    //use microtemplate::Substitutions;

    //use actix_web::http::header;

    type BoxError = Box<dyn std::error::Error>;
    #[derive(Debug)]
    pub enum ImpError {
        //TODO: don't just Box everything
        BadRequest((&'static str,BoxError)),
        InternalError((&'static str,BoxError)),
    }

    impl Display for ImpError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            use ImpError::*;
            match self {
                BadRequest((s,e)) =>
                    if s.is_empty() {
                        write!(f,"{}", e.to_string())
                    } else {
                        write!(f,"{}: {}", s, e.to_string())
                    }
                InternalError((s,e)) =>
                    if s.is_empty() {
                        write!(f,"{}", e.to_string())
                    } else {
                        write!(f,"{}: {}", s, e.to_string())
                    }
            }
        }
    }

    impl actix_web::ResponseError for ImpError {
        fn error_response(&self) -> HttpResponse {
            HttpResponse::build(self.status_code())
                //.insert_header(ContentType::html())
                .body(self.to_string())
        }
        fn status_code(&self) -> StatusCode {
            use ImpError::*;
            match self {
                BadRequest(_) => StatusCode::BAD_REQUEST,
                InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }

    pub trait OrImpResult<T> {
        fn or_bad_request(self, message : &'static str) -> ImpResult<T>;
        fn or_internal_error(self, message : &'static str) -> ImpResult<T>;
    }

    impl<T,E> OrImpResult<T> for Result<T,E> where E : std::error::Error+'static {
        fn or_bad_request(self, message : &'static str) -> ImpResult<T> {
            self.or_else(|e| -> Result<T, ImpError> {Err(ImpError::BadRequest((message,Box::new(e))))})
        }

        fn or_internal_error(self, message : &'static str) -> ImpResult<T> {
            self.or_else(|e| Err(ImpError::InternalError((message,e.into()))))
        }
    }

    //pub type ImpResult<R> = Result<R, Box<dyn std::error::Error>>;
    pub type ImpResult<R> = Result<R, ImpError>;

    /// Transformation to apply to a field
    #[derive(Clone,Debug, Serialize, Deserialize)]
    struct FieldTransform {
        field : String
    }
    /// Field to generate
    #[derive(Clone,Debug, Serialize, Deserialize)]
    struct GeneratedField {
        name : String
    }

    /// Field validation and mutation rules for entry
    #[derive(Clone,Debug, Serialize, Deserialize)]
    pub struct FieldConfig {
        allowed : HashSet<String>,
        #[serde(default)]
        required : HashSet<String>,
        #[serde(default)]
        transforms : Vec<FieldTransform>,
        #[serde(default)]
        extra : Vec<GeneratedField>
    }

    /// Serialization format for entry
    #[derive(Copy,Clone,Debug, Serialize, Deserialize)]
    enum SerializationType {
        #[serde(rename = "json")]
        Json,
        #[serde(rename = "yaml",alias = "yml")]
        Yaml
    }

    /// Git-specific entry config
    #[derive(Clone,Debug, Serialize, Deserialize)]
    pub struct GitEntryConfig {
        /// Directory path to store entry under
        path : String,
        /// Filename to use for entry
        filename : String,
        /// Branch to send entries to (or submit merge request for)
        branch : String,
        /// Git commit message
        commit_message : String
    }

    /// configuration for new entry
    #[derive(Clone,Debug, Serialize, Deserialize)]
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
        pub fn field_config(&self) -> &FieldConfig {
            &self.fields
        }
    }

    /// Gitlab backend configuration
    #[derive(Clone,Debug, Serialize, Deserialize)]
    pub struct GitlabConfig {
        host: String,
        #[serde(default)]
        token: String
    }

    impl GitlabConfig {
        async fn new_client(&self) -> ImpResult<GitlabAPI> {
            let client = gitlab::GitlabBuilder::new(self.host.as_str(),self.token.as_str()).build_async().await
                .or_internal_error("Failed to open client")?;
            Ok(GitlabAPI::new(client))
        }
    }

    /// Backend config is an enum of backend configuration variants
    #[derive(Clone,Debug, Serialize, Deserialize)]
    #[serde(tag = "driver")]
    pub enum BackendConfig {
        #[serde(rename = "gitlab")]
        Gitlab(GitlabConfig)
    }

    impl BackendConfig {
        pub async fn new_client(&self) -> ImpResult<Backend> {
            match self {
                BackendConfig::Gitlab(conf) => {
                    let client = conf.new_client().await?;
                    Ok(Backend::Gitlab(client))
                }
            }
        }
    }


    //Config - staticimp configuration
    // - Configuration override order:
    //   - service staticimp.yml
    //   - environment variables
    //   - site staticman.yml (if allow_repo_override set)
    // - right now only supports a single backend (gitlab)
    //   - TODO: clean solution to support multiple backends (which may use the same or different Backend)
    // - currently also implements BackendAPI (passes through to backend), but that will probably change with multiple backend support
    #[derive(Clone,Debug, Serialize, Deserialize)]
    pub struct Config {
        pub backends : HashMap<String,BackendConfig>,
        pub host : String,
        pub port : u16,
        #[serde(default)]
        project_config_path : String, //empty -- global service conf only
        pub entries : HashMap<String,EntryConfig>
    }

    impl Config {
        pub fn load(path: &str) -> ImpResult<Self> {
            let f = std::fs::File::open(path)
                .or_internal_error("Couldn't open config file")?;
            let mut cfg : Config = serde_yaml::from_reader(f)
                .or_internal_error("Bad config yaml")?;

            let env_override = |var : &mut String,varname : &str| {
                let val = std::env::var(varname).unwrap_or("".to_owned());
                if !val.is_empty() {
                    *var = val;
                }
            };

            for (name,backend) in cfg.backends.iter_mut() {
                match backend {
                    BackendConfig::Gitlab(gitlab) => {
                        env_override(&mut gitlab.host,&(name.clone() + "_host"));
                        env_override(&mut gitlab.token,&(name.clone() + "_token"));
                    }
                }
            }

            //println!("Config: {:?}", cfg);
            Ok(cfg)
        }
    }

    /// staticimp Entry content
    #[derive(Clone,Debug, Serialize, Deserialize)]
    pub struct Entry {
        /// Entry Fields
        #[serde(flatten)]
        fields : HashMap<String,String>
    }

    impl Entry {
        pub fn new(fields : HashMap<String,String> ) -> Self {
            Entry { fields }
        }
        ///// set entry fields (and returns self)
        //fn field(mut self, key : String, value : String) -> Self {
        //    self.fields.insert(key,value);
        //    self
        //}
        /// validate fields in entry
        fn validate(self, allowed : &HashSet<String>, required : &HashSet<String>) -> ImpResult<Self> {
            let keys : HashSet<String> = self.fields.keys().map(|s| s.to_string()).collect();
            if !required.is_subset(&keys) {
                Err(ImpError::BadRequest(("","Missing field(s)".into())))
            } else if !keys.is_subset(&allowed) {
                Err(ImpError::BadRequest(("","Unknown field(s)".into())))
            } else {
                // passed all validation requests, return self
                Ok(self)
            }
        }
        /// Transform fields
        fn transform(mut self, transforms : &Vec<FieldTransform>) -> ImpResult<Self> {
            for t in transforms.iter() {
                if let Some(_field) = self.fields.get_mut(&t.field) {
                    todo!("Transform not implemented")
                }
            }
            Ok(self)
        }
        /// Generate extra fields
        fn generate(self, extra : &Vec<GeneratedField>) -> ImpResult<Self> {
            for _t in extra.iter() {
                todo!("Generate not implemented")
            }
            Ok(self)
        }

        /// produce final entry (validation,extra fields,transformations)
        pub fn finalize(self, conf : &FieldConfig) -> ImpResult<Self> {
            self
                .validate(&conf.allowed,&conf.required)?
                .generate(&conf.extra)?
                .transform(&conf.transforms)
        }

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
    /// 
    /// We avoid copies by 
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
        fn serialize(&self) -> ImpResult<Vec<u8>> {
            self.entry.serialize(self.serializer)
        }
    }

    //impl GitEntry {
    //    fn new() -> Self {
    //        GitEntry {
    //            project_id : "".to_string(),
    //            branch : "".to_string(),
    //            file_path : "".to_string(),
    //            entry : Entry::new(),
    //            commit_message : "".to_string(),
    //            review : false,
    //            serializer : SerializationType::Json
    //        }
    //    }
    //    fn project(mut self, id : String) -> Self {
    //        self.project_id = id;
    //        self
    //    }
    //    fn branch(mut self, branch : String) -> Self {
    //        self.branch = branch;
    //        self
    //    }
    //    fn path(mut self, path : String) -> Self {
    //        self.file_path = path;
    //        self
    //    }
    //    fn content(mut self, entry : Entry) -> Self {
    //        self.entry = entry;
    //        self
    //    }
    //    fn message(mut self, message : String) -> Self {
    //        self.commit_message = message;
    //        self
    //    }
    //    fn review(mut self, review : bool) -> Self {
    //        self.review = review;
    //        self
    //    }
    //    //fn to_bytes(mut self) -> Vec<u8> {
    //    //    todo!("Not Implemented")
    //    //}
    //    //async fn post<T:GitAPI>(self, api : &T) -> ImpResult<()> {
    //    //    //FIXME: Validate
    //    //    if self.review {
    //    //        todo!("implement review branches")
    //    //    } else {
    //    //        api.new_file(&self.project_id,&self.branch,&self.file_path,&self.entry.to_bytes(self.serializer)?,&self.commit_message).await
    //    //    }
    //    //}
    //}

    /// Context for expanding placeholders while processing an entry
    #[derive(Clone,Debug)]
    pub struct NewEntry {
        /// uuid for entry
        uid : String,
        /// timetamp for entry
        timestamp : String,
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
        pub fn new(project_id: String, branch: String, entry : Entry, options : HashMap<String,String>) -> Self {
            let uid = Uuid::new_v4().to_string();
            //FIXME: proper timestamps and time formatting
            //let timestamp = format!("{:?}",std::time::SystemTime::now());
            let timestamp = match std::time::SystemTime::now().duration_since(std::time::SystemTime::UNIX_EPOCH) {
                Ok(n) => n.as_secs().to_string(),
                Err(_) => panic!("SystemTime before UNIX EPOCH!"),
            };
            NewEntry {
                uid,
                timestamp,
                project_id,
                branch,
                entry,
                options,
                //special : HashMap::from([
                //    ( "@id", uid )
                //])
            }
        }

        pub fn process_fields(mut self, fields : &FieldConfig) -> ImpResult<Self> {
            self.entry = self.entry.finalize(fields)?;
            Ok(self)
        }
    }

    impl microtemplate::Context for &NewEntry {
        //TODO: microtemplate doesn't have a good way to return expansion errors, so we just return
        //  empty string if no match -- do something better
        fn get_field(&self, field_name: &str) -> &str {
            //TODO: should we return empty string or field name on no match?
            if field_name.starts_with('@') { //special generated vars
                //self.special.get(field_name).map_or(&"",|v| &v)
                if field_name == "@id" {
                    &self.uid
                } else if field_name == "@timestamp" {
                    &self.timestamp
                } else if field_name.starts_with("@date:") {
                    todo!("formatted date")
                } else if field_name.starts_with("@branch") {
                    &self.branch
                } else {
                    &""
                }
            } else {
                if let Some((lhs,rhs)) = field_name.split_once('.') {
                    if lhs == "fields" {
                        self.entry.fields.get(rhs).map_or(&"",|v| &v)
                    } else if lhs == "options" {
                        match self.options.get(rhs) {
                            Some(val) => &val,
                            None => ""
                        }
                    } else {
                        &""
                    }
                } else {
                    &""
                }
            }
        }
    }

    trait ExpandTemplate<T> {
        fn expand(&self, context : &NewEntry) -> ImpResult<T>;
    }

    impl ExpandTemplate<GitEntry> for EntryConfig {
        /// build new GitEntry
        fn expand(&self, entry : &NewEntry) -> ImpResult<GitEntry> {
            use microtemplate::render;
            if entry.branch.is_empty() {
                Err(ImpError::BadRequest(("","Must specify branch".into())))
            } else if let Some(gitconf) = self.git.as_ref() {
                let branch = render(&gitconf.branch,entry);
                if !branch.is_empty() && branch != entry.branch {
                    Err(ImpError::BadRequest(("","Branch not allowed".into())))
                } else {
                    use std::path::Path;
                    let file_path = Path::new(&render(&gitconf.path,entry)).join(&render(&gitconf.filename,entry)).to_str()
                        .ok_or_else(|| ImpError::BadRequest(("","Bad Entry Path".to_string().into())))?.to_string();
                    Ok(GitEntry {
                        project_id : render(&entry.project_id,entry),
                        branch : render(&gitconf.branch,entry),
                        //FIXME: build proper path (e.g. strip dup '/')
                        file_path,
                        entry : entry.entry.clone(),
                        commit_message : render(&gitconf.commit_message,entry),
                        review : self.review,
                        serializer : self.format
                    })
                }
            } else {
                Err(ImpError::BadRequest(("","missing git entry configuration".into())))
            }
        }
    }

    #[async_trait::async_trait(?Send)]
    pub trait BackendAPI {
        async fn new_entry(&mut self, entry_conf : &EntryConfig, entry : NewEntry) -> ImpResult<()>;
        //async fn get_entry(&self, id: &str, path: &str) -> ImpResult<Entry>;
    }

    pub enum Backend {
        Gitlab(GitlabAPI)
    }

    #[async_trait::async_trait(?Send)]
    impl BackendAPI for Backend {
        async fn new_entry(&mut self, entry_conf : &EntryConfig, entry : NewEntry) -> ImpResult<()> {
            match self {
                Backend::Gitlab(api) => api.new_entry(&entry_conf,entry).await
            }
        }
    }


    #[derive(Clone,Debug, Serialize, Deserialize)]
    struct GitCommit {
        id : String
    }

    #[derive(Clone,Debug, Serialize, Deserialize)]
    pub struct GitBranch {
        name : String,
        commit : GitCommit
    }

    #[derive(Clone,Debug, Serialize, Deserialize)]
    pub struct GitProject {
        id : u32,
        name : String,
        path : String,
        #[serde(rename = "path_with_namespace")]
        full_path : String
    }


    #[async_trait::async_trait(?Send)]
    pub trait GitAPI {
        async fn get_file(&self, project: &str, ref_: &str, path: &str) -> ImpResult<Vec<u8>>;
        async fn new_file(&self, project: &str, branch: &str, path: &str, content: &Vec<u8>, commit_message: &str) -> ImpResult<()>;
        async fn get_project(&self, id: &str) -> ImpResult<GitProject>;
        async fn get_branch(&self, id: &str, branch: &str) -> ImpResult<GitBranch>;

        //async fn new_git_entry(&self, conf : &EntryConfig, entry : Entry) -> ImpResult<()> {
        //    let _entry = entry.finalize(&conf.fields)?;
        //    todo!("Not Implemented")
        //}
    }

    #[derive(Clone,Debug)]
    pub struct GitlabAPI {
        client : gitlab::AsyncGitlab
        //host: String,
        //#[serde(default)]
        //token: String,
    }
    impl GitlabAPI {
        fn new(gitlab_api : gitlab::AsyncGitlab ) -> Self {
            Self { client : gitlab_api }
        }
    }

    #[async_trait::async_trait(?Send)]
    impl BackendAPI for GitlabAPI {
        async fn new_entry(&mut self, entry_conf : &EntryConfig, entry : NewEntry) -> ImpResult<()> {
            let git_entry = entry_conf.expand(&entry)?;
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
        fn from(client: gitlab::AsyncGitlab) -> Self {
            Self::new(client)
        }
    }

    #[async_trait::async_trait(?Send)]
    impl GitAPI for GitlabAPI {
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

        async fn get_project(&self, _id: &str) -> ImpResult<GitProject> {
            todo!("Not Implemented")
        }

        async fn get_branch(&self, _id: &str, _branch: &str) -> ImpResult<GitBranch> {
            todo!("Not Implemented")
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
} //mod staticimp

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

//async fn post_comment(
//    cfg : ConfigData,
//    //entry : web::Json<Entry>,
//    pathargs: web::Path<(String,String,String,String)>,
//    content_type: web::Header<header::ContentType>,
//    //query: web::Query<HashMap<String,String>>
//    req: actix_web::HttpRequest,
//    //body: web::Bytes
//    mut body: actix_web::dev::Payload
////) -> ImpResult<actix_web::HttpResponse> {
//) -> ImpResult<impl actix_web::Responder> {
//    let pathargs = pathargs.into_inner();
//    let backend = pathargs.0;
//    let project_id = pathargs.1;
//    let branch = pathargs.2;
//    let entry_type = pathargs.3;
//
//    use actix_web::FromRequest;
//    //use actix_web::web;
//    use header::ContentType;
//    //parse entry from request
//    // currently supports:
//    // - html form
//    // - json
//    // - yaml (using application/yaml content-type)
//    let entry = if content_type.0 == ContentType::form_url_encoded() {
//        web::Form::<Entry>::from_request(&req,&mut body).await
//            .or_else(|e| Err(format!("Bad Form entry: {}",e.to_string())))?.0
//    } else if content_type.0 == ContentType::json() {
//        let body = web::Bytes::from_request(&req,&mut body).await?;
//        serde_json::from_slice::<Entry>(&body)
//            .or_else(|e| Err(format!("Bad json entry: {}",e.to_string())))?
//    } else if content_type.0.to_string() == "application/yaml" {
//        let body = web::Bytes::from_request(&req,&mut body).await?;
//        serde_yaml::from_slice::<Entry>(&body)
//            .or_else(|e| Err(format!("Bad yaml entry: {}",e.to_string())))?
//    } else {
//        return Err("Bad Content-Type".into())
//    };
//
//    let query = web::Query::<HashMap<String,String>>::from_query(req.query_string())
//        .or_else(|e| Err(format!("Bad query arguments: {}",e.to_string())))?.0;
//
//    ////return processed args for debugging
//    //return actix_web::HttpResponse::Ok().body(
//    //    format!(
//    //        "entry args:\n  backend: {}\n  proj: {}\n  branch: {}\n  type: {}\n  options: {:?}\n  fields: {:?}\n",
//    //        backend,
//    //        project_id,
//    //        branch,
//    //        entry_type,
//    //        query,
//    //        entry
//    //    )
//    //);
//
//    let newentry = NewEntry::new(project_id,branch,entry,query);
//
//    //return NewEntry for debugging
//    //return Ok(actix_web::HttpResponse::Ok().body(format!("NewEntry: {:?}\n",newentry)));
//
//    if let (Some(backend),Some(conf)) = (
//        //backends.get(&backend),
//        cfg.backends.get(&backend),
//        cfg.entries.get(&entry_type)
//    ) {
//        //create new client from backend (TODO: per-thread client)
//        let mut backend = backend.new_client().await
//            //TODO: return InternalServerError
//            .or_else(|_|->ImpResult<_> { Err(format!("Failed to open client").into())})?;
//
//        //create new entry
//        backend.new_entry(conf,newentry).await
//            .and_then(|_| Ok(actix_web::HttpResponse::Ok().finish()))
//    } else {
//        Err("Unknown backend or entry type".into())
//    }
//
//    //format!("Not Implemented")
//
//    //match serde_yaml::to_string(&comment) {
//    //    Ok(comment) => actix_web::HttpResponse::Ok().body(comment),
//    //    Err(_) => actix_web::HttpResponse::BadRequest().body(format!("Invalid request format")),
//    //}
//    ////format!("name: {}\nmessage: {}\n",comment.name,comment.message)
//}

//use actix_web::HttpMessage;

#[actix_web::post("/v1/entry/{backend}/{project:.*}/{branch}/{entry_type}")]
async fn post_comment_handler(
    cfg : ConfigData,
    pathargs: web::Path<(String,String,String,String)>,
    content_type: web::Header<header::ContentType>,
    //query: web::Query<HashMap<String,String>>
    req: actix_web::HttpRequest,
    //body: web::Bytes
    body: actix_web::web::Payload
    //mut body: actix_web::dev::Payload,
    //form_entry : Option<web::Form::<Entry>>
) -> impl actix_web::Responder {
    //match post_comment(
    //    cfg,
    //    pathargs,
    //    content_type,
    //    req,
    //    body
    ////).await.unwrap_or_else(|e| actix_web::HttpResponse::BadRequest().body(format!("Bad Entry: {}",e.to_string())))
    //).await {
    //    Ok(response) => response,
    //    Err(e) => actix_web::HttpResponse::BadRequest().body(format!("Bad Entry: {}",e.to_string()))
    //}
    let pathargs = pathargs.into_inner();
    let backend = pathargs.0;
    let project_id = pathargs.1;
    let branch = pathargs.2;
    let entry_type = pathargs.3;

    let mut body = body.into_inner();

    let content_type = content_type.0;
    //let content_type : ContentType = Header::extract(&req).await?.0;

    //use actix_web::HttpMessage;
    //let mut body = req.take_payload();

    //use actix_web::web;
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
        return Err(ImpError::BadRequest(("","Bad Content-Type".into())))
    };


    let query = web::Query::<HashMap<String,String>>::from_query(req.query_string())
        .or_bad_request("Bad query args")?.into_inner();

    ////return processed args for debugging
    //return actix_web::HttpResponse::Ok().body(
    //    format!(
    //        "entry args:\n  backend: {}\n  proj: {}\n  branch: {}\n  type: {}\n  options: {:?}\n  fields: {:?}\n",
    //        backend,
    //        project_id,
    //        branch,
    //        entry_type,
    //        query,
    //        entry
    //    )
    //);

    let newentry = NewEntry::new(project_id,branch,entry,query);

    //return NewEntry for debugging
    //return Ok(actix_web::HttpResponse::Ok().body(format!("NewEntry: {:?}\n",newentry)));

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
        Err(ImpError::BadRequest(("","Unknown backend or entry type".into())))
    }

    //format!("Not Implemented")

    //match serde_yaml::to_string(&comment) {
    //    Ok(comment) => actix_web::HttpResponse::Ok().body(comment),
    //    Err(_) => actix_web::HttpResponse::BadRequest().body(format!("Invalid request format")),
    //}
    ////format!("name: {}\nmessage: {}\n",comment.name,comment.message)
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
    let cfg = Config::load(cfgpath);
    if let Err(e) = cfg {
        //eprintln!("Error loading config: {:#?}",e);
        eprintln!("Error loading {}: {}",cfgpath,e);
        std::process::exit(1);
    }
    let cfg = cfg.unwrap();
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
                //.service(hello)
                //.service(getfile)
                //.service(addfile)
                //.service(getproject)
                //.service(getbranch)
                //.service(comment_json)
                //.service(comment_form)
                //.service(comment_query)
                //.service(comment_yaml)
        })
        .bind((host.as_str(), port))?
        .run()
        .await
}

