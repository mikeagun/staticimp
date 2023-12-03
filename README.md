# staticimp

# Introduction
staticimp (static imp) is a rust-based web service that receives user-generated content and uploads it to a backend (currently just GitLab, GitHub planned).
The main goal of staticimp is to support dynamic content (e.g. blog post comments) on a fully static website using automatic build+deployment.

staticimp consists of a small web service which handles POST requests from HTML forms (it also accepts json and yaml),
performs validation and transformations, then uploads them using a backend api. staticimp also supports moderation,
where the file is commited to a new branch and a merge request is created, instead of commiting the files directly to the main branch.

The actual rendering of the comments (or whatever you are using staticimp for) is up to your static site generator (SSG),
staticimp is just concerned with pushing the user generated content to the repo.

staticimp, like Staticman, can create yaml/json files in a repository, so if for example you are using a hugo theme that already supports staticman,
you should be able to support staticimp with minimal changes (staticimp.yml and changing the POST url).

# Features:

The basic staticimp features are stable, but thorough test code is still
needed and reCAPTCHA support (which is needed for practical use on public websites) is only mostly complete.

**Features Implemented**
- can support multiple backends simultaneously
 - the supported backend drivers are compiled in, but you can set up multiple backends (e.g. gitlab1,gitlab2) with different configs
 - current backend drivers: gitlab, debug
- flexible configuration support with both server config and project config
  - can take sensitive configuration values (e.g. gitlab token) from environment variables
  - supports placeholders to pull config values from requests
    - e.g. `{@id}` in entry config gets replaced with entry uid
    - uses rendertemplate (in this crate) for rendering placeholders
  - loads server config from `staticimp.yml`
  - project-specific config can be stored in project repo
  - entry validation checks for allowed/required fields
  - generated fields
    - e.g. to add uid/timestamp to stored entry
  - field transforms
    - current transforms: slugify, md5, sha256, to/from base85
- encrypted project secrets
  - public-key encrypt short project secrets, where only the staticimp server has the private key to decrypt
  - useful for storing project-specific secrets in public/shared project repos, e.g. reCAPTCHA secret
- moderated comments
  - commits entries to new branch and creates merge request instead of commiting directly to target branch

**Features still to implement**
- thorough test code
- logging
- specify allowed hosts for a backend (**WIP**)
- specify trusted relay hosts (**WIP**)
- reCAPTCHA (**mostly finished**)
- github as a second backend
- field format validation
- local git/filesystem backend



# Inspiration
staticimp was inspired by the awesome project [Staticman](https://github.com/eduardoboucas/staticman).
If you already use Node.js and/or have a webserver with plenty of RAM, Staticman is a great tool and you should check it out.

While staticman is great, the memory consumption is a little heavy for running in stateless/serverless environments (e.g. [Google Cloud Run](https://cloud.google.com/run)) or on a tiny VPS.


In my use and testing staticman takes:
- RAM Usage is around 70-110+MB RAM idling, and >120MB during startup, which makes it a little tight for a VPS with 128MB total.
- startup takes 2-3 seconds (from docker compose up to first HTTP response)
- docker image is around 1.4GB (for pulling in all the dependencies), which isn't a big deal but does slow down docker pull / run

_NOTE: none of the above may matter if you have a large webserver that is always running, has 100s of MB free memory, and has lots of disk space,
but might very much matter on a small VPS_

While the above numbers could probably be reduced (maybe even significantly), Node isn't known for for being lightweight, so I wrote staticimp as a static-site/dynamic-content solution with a much smaller footprint.


# Resources
staticimp is a lightweight solution to the static-site/dynamic-content problem:
- relatively low RAM usage (4-8MB)_\*_
- startup is fast (10-40ms to first HTTP response, 400ms including docker run on my machine)_\*_
- small docker image (under 30MB)

_\* the RAM/startup time numbers are based on informal benchmarking on my dev machine_



# Building and Running staticimp

## Docker

The easiest way to run staticimp is using docker compose:
```bash
cp env.example .env #edit per your setup
cp staticimp.sample.yml staticimp.yml #edit per your setup

sudo docker compose up -d
```

To build and run a docker container manually:
```bash
cp staticimp.sample.yml staticimp.yml # edit per your setup

sudo docker build -t staticimp:latest .
sudo docker run -d --restart=always \
  --name=staticimp --hostname=staticimp -p 8080:8080 \
  -v "$(pwd)/staticimp.yml:/staticimp.yml:ro" \
  -e gitlab_token=XXXXXX \
  staticimp:latest
```

## cargo

staticimp uses rust [cargo](https://doc.rust-lang.org/cargo/) for building

To build staticimp and run it through cargo:
```bash
cp staticimp.sample.yml staticimp.yml # edit per your setup

gitlab_token=XXXXXXXXXX cargo run --release
# -- OR --
cargo build --release
gitlab_token=XXXXXXXXXX target/release/staticimp
```

## Program Arguments

By default staticimp reads the server config from `staticimp.yml`.

To change this pass arguments to staticimp on the command line:
- `-f <path>` - read local config file from `<path>`
- `-f -` - read config from stdin (this also disables environment variable processing)
- `--yaml` or `--yml` - read config as yaml (this is the default unless `<path>` ends in `.json`)
- `--json` - read config as json

You can pass `--print-config` to print the server config and exit
- the config gets printed in the same format as the input config
- you can use this to strip comments from yaml config or to expand default fields

# Testing staticimp

Below are some useful oneliners for testing if staticimp is up and working.

the examples using `curl` are probably the ones you want, but there are also `wget` and `nc` (netcat) examples if needed (e.g. in minimal alpine docker container)

### Test reachability
```bash
#using curl
curl 127.0.0.1:8080/

#using wget
wget -q -S -O - localhost:8080/ || { echo "connection failed"; }

#using nc
printf 'GET / HTTP/1.1\r\nConnection: close\r\n\r\n' | nc -v -w3 127.0.0.1 8080
```

### Add a new entry

- **NOTE:** for testing you may want to set `debug: true` on the entry type

```bash
#note that project id can be numeric or the full path to the project
# - e.g. 42 or "myusername/myproject"
#using curl
curl -H "Content-Type: application/json" -X POST --data '{"name":"John Doe","email":"johndoe@example.com","comment":"this is a test"}' '127.0.0.1:8080/v1/entry/debug/42/main/comment?slug=staticimp-test'

#using wget
wget -q -S -O - --header "Content-Type: application/json" --post-data '{"name":"John Doe","email":"johndoe@example.com","comment":"this is a test"}' 'http://127.0.0.1:8080/v1/entry/gitlab/mygroup/myproject/main/comment?slug=staticimp-test'

#using nc
sh -c 'printf "POST /v1/entry/gitlab/42/main/comment?slug=staticimp-test HTTP/1.1\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: $(echo -n "$0"|wc -c)\r\n\r\n$0"' '{"name":"John Doe","email":"johndoe@example.com","comment":"this is a test"}'
```


# Site Repo vs Comments Repo
The easiest way to use staticimp is have it commit files directly to your website content repository.
Your CI/CD job that builds the static site stays the same,
and you just need to configure your SSG to display the staticimp content.

### Comments in Website Repo

Steps to get comment live:
1. HTML Form POSTs to staticimp
1. staticimp commits file to website repo
1. website repo CI/CD automatically rebuilds site with new comment and deploys to live site

The downside to the above approach is that your git history gets cluttered with staticimp commits, and if there is lots of traffic
you can get stuck in a loop:
1. push
1. fail to push because your local repo is out of data
1. pull
1. repeat (if a comment goes through before your next push attempt)

You can work in another branch and then use merge requests to merge the changes into your main branch,
but in the best case your git history will still be thoroughly cluttered.

Since I like keeping a relatively clean git history, I have staticimp push files to a separate repo, and then have the main content repo pull the latest
changes on build.

### Separate Comments Repo

Steps to get comment live:
1. HTML Form POSTs to staticimp
1. staticimp commits file to separate comments repo
1. comments repo CI/CD triggers website repo build+deploy pipeline
1. website repo CI/CD automatically rebuilds site with new comments and deploys to live site
  - added to the build step above is cloning the comments repo or making it a git submodule

With this approach the comments are kept separate until website build time (then pulled from comments repo).
To build on comment commit, a CI job from the comments repo triggers rebuilds of the main website repo.

# Moderation
staticimp supports moderated entries when `review: true` is set in the entry config.

When review is enabled, instead of commiting comments directly to the repo:
1. new branch is created for entry
1. entry is commited to review branch
1. merge request from review branch to target branch is created

This lets you merge/close the MR to accept/ignore the comment

# Migrating from Staticman to staticimp
The main practical differences between running staticimp and staticman:
- server/project config files
  - though the basic configuration options are similar, the config format is different
    - see [staticimp.sample.yml](staticimp.sample.yml) for a sample staticimp server config
  - the staticimp server/project config format is the same, but only `entries:` is used from the project conf
- entry submission URL
  - `/v1/entry/{backend}/{project:.*}/{branch}/{entry_type}`
- staticimp is an active work-in-progress, so it is possible some features will change, but mostly I'll be filling out the feature set

# Setting up Hugo

**TO BE WRITTEN**

# Configuration

The staticimp server configration file is "staticimp.yml"

The project configuration format is exactly the same as server config, except that only the `entries:` are used.

See the sample [server configuration](staticimp.sample.yml) and [project configuration](staticimp.project.yml) files
for commented example files to start from

### staticimp config Structure

**Config Format:**
- `host:` - host to listen on (default: `"127.0.0.1"`)
- `port:` - port to listen on (default: `8080`)
- `timestamp_format:` - format for `{@timestamp}` placeholders (default: `"%Y%m%dT%H%M%S%.3fZ"`)
- `backends:` - server backends
  - _... backends to support ..._
- `entries:` - global entry configurations
  - _... entry types to support ..._

**Example:**
```yaml
# server settings
host: "0.0.0.0" # host to listen on (default: "127.0.0.1")
port: 8080 # port to listen on (default: 8080)

#verbose iso8601 (with microseconds)
timestamp_format: "%+" # format for "{@timestamp}" placeholders (default: "%Y%m%dT%H%M%S%.3fZ")

backends:
  gitlab:
    project_config_path: "staticimp.yml"
    driver: gitlab
    host: git.example.com
    #token=... #get from env

entries:
  comment: # entry type name (in this case `comment`)
    fields: # entry field processing
      allowed: [ "name", "email", "url", "message" ]
      required: [ "name", "email","message: ]
      extra:
        _id: "{@id}"
        date: "{@date:%+}"
        email_md5: "{field.email}"
      transforms:
        - field: email_md5
          transform: md5
    git:
      path: "data/comments"
```


### Backend Configuration

- contains backend configuration settings
- there are shared settings and driver-specific settings

`mybackend:` - backend name (in this case `mybackend`)
- `project_config_path:` - project-specific config path (default: "")
- `project_config_format:` - project-specific config path (default: yaml)
- `driver:` - which backend driver to use for this backend (required)
  - current options: `gitlab`, `debug`
- **gitlab specific**
- `host:` - hostname for gitlab server, with no leading https://
  - **NOTE:** host and token can be overriden by the `<backend>_<var>` environment variables (e.g. `mybackend_token`)
- `token:` - gitlab auth token, recommend to load from env var instead to keep out of repo
- **debug specific**
  - _currently no options for debug backend_

**Example:**
```yaml
gitlab:
  project_config_path: "staticimp.yml"

  driver: gitlab
  host: git.example.com
  #token=... #get from env (or set here)
```

### Entry Type Configuration

contains default entry types to support. entry types in the server conf are overriden by
project conf entry types of the same name.

`comment:` - entry type name (in this case `comment`)
- `disabled:` - disables entry type (default: `false`)
- `debug:` - return entry debugging info instead of commiting new entry (default: `false`)
  - with `debug: true`, staticimp does all entry processing, then returns config and entry details instead of sending via backend client
- `fields:` - entry field processing configuration
  - `allowed:` - allowed entry fields (default: `[ ]`)
  - `required:` - required entry fields (default: `[ ]`)
  - `extra:`
    - _... extra fields to generate ..._
  - transforms:
    - _... transforms to apply ..._
- `review:` - whether to moderate comments (default: `false`)
  - with `review: true`, entries get created in a new review branch
- `format:` - serialization format for entries (default: `json`)
- `git:` - _optional_ - git specific entry configuration (these all support placeholders)
  - `path:` - directory path to place entries in (default: `"data/entries"`)
  - `filename:` - entry file name (default: `"entry-{@timestamp}.yml"`)
  - `branch:` - branch to commit entries to (default: `"main"`)
    - if `review: true`, commits entry to `review_branch` with MR to `branch`
  - `commit_message:` - entry commit message (default: `"New staticimp entry"`)
  - `review_branch:` - entry review branch name (default: `"staticimp_{@id}"`)
  - `mr_description:` - merge request description
    - default: `"new staticimp entry awaiting approval\n\nMerge the pull request to accept it, or close it"`
 
**Example:**
```yaml
comment:
  fields:
    #allowed: [ "name", "email", "url", "message" ]
    allowed: ["name", "email", "website", "comment", "replyThread", "replyName", "replyID"]
    required: ["name", "email", "comment"]
    extra:
      # add comment uid as '_id' field
      _id: "{@id}"
      # add entry timestamp (verbose ISO8601)
      # for format syntax see: https://docs.rs/chrono/latest/chrono/format/strftime/index.html
      date: "{@date:%+}"
    transforms:
      - field: email
        transform: md5
  #review: false
  #format: yaml
  git:
    path: "data/comments/{params.slug}" #default: "data/comments"
    #filename: "comment-{@timestamp}.yml"
    #branch: main
    #commit_message: "New staticimp entry"
    #review_branch: "staticimp_{@id}"
    #mr_description: "new staticimp entry awaiting approval\n\nMerge the pull request to accept it, or close it to deny the entry"
```

### Extra Fields

- `extra:` fields are generated after `allowed`/`required` validation
- field transformations are applied after extra fields are generated

**Example:**
```yaml
- field: email
  transform: md5
```

### Field Transformations

- `transforms:` are applied after `extra:` fields are generated

**Example:**
- add entry uid and verbose iso8601 timestamp
```yaml
_id: "{@id}"
date: "{@date:%+}"
```


## Links

- [Staticman](https://github.com/eduardoboucas/staticman)

```
Copyright 2023 Michael Agun

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this project and code except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```
