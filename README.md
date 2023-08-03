# staticimp

# Introduction
staticimp (static imp) is a rust-based web service that receives user-generated content and uploads it to a backend (currently just GitLab, GitHub planned).
The main goal of staticimp is to support dynamic content (e.g. blog post comments) on a fully static website using automatic build+deployment.

staticimp consists of a small web service which handles POST requests from HTML forms (it also accepts json and yaml),
performs validation and transformations, then uploads them using a backend api. staticman also supports moderation,
where the file is commited to a new branch and a merge request is created, instead of commiting the files directly to the main branch.

The actual rendering of the comments (or whatever you are using staticimp for) is up to your static site generator (SSG),
staticimp is just concerned with pushing the user generated content to the repo.

staticimp, like Staticman, can create yaml/json files in a repository, so if for example you are using a hugo theme that already supports staticman,
you should be able to support staticimp with minimal changes (staticimp.yml and changing the POST url).


# Inspiration
staticimp was inspired by the awesome project [Staticman](https://github.com/eduardoboucas/staticman).
If you already use Node.js and/or have a webserver with plenty of RAM, Staticman is a great tool and you should check it out.

While staticman is great, the memory consumption is a little heavy for running in stateless/serverless environments (e.g. [Google Cloud Run](https://cloud.google.com/run)) or on a tiny VPS.

In my use and testing:
- RAM Usage is around 70-110+MB RAM idling, and >120MB during startup, which makes it a little tight for a VPS with 128MB total.
- startup takes 2-3 seconds
- docker image is around 1.4GB (for pulling in all the dependencies), which isn't a big deal but does slow down docker pull / run

_NOTE: none of the above may matter if you have a large webserver that is always running, has 100s of MB free memory, and lots of disk space,
but might very much matter on a small VPS_

While the above numbers could probably be reduced (maybe even significantly), Node isn't known for for being lightweight, so I wrote staticimp to solve the same static-site/dynamic-content problem with a much smaller footprint.


# Resources
staticimp is a lightweight solution to the static-site/dynamic-content problem:
- relatively low RAM usage (4-8MB)_\*_
- startup is fast (10-40ms to first HTTP response, 400ms including docker run on my machine)_\*_
- small docker image (under 30MB)

_\* the RAM/startup time numbers are based on informal benchmarking on my dev machine_


# Features:
- can support multiple backends simultaneously
 - the supported backend drivers are compiled in, but you can set up multiple backends (e.g. gitlab1,gitlab2) with different configs
 - current backend drivers: gitlab, debug
- configuration supports placeholders to pull config values from requests
  - e.g. `{@id}` in entry config gets replaced with entry uid
- loads server config from `staticman.yml`
- project-specific config can be stored in project repo
- entry validation checks for allowed/required fields
- extra fields can be generated from config
  - e.g. to add uid/timestamp to stored entry
- can transform fields in config
  - current transforms: slugify, md5, sha256
- moderated comments
  - commits entries to new branch and creates merge request to accept comment


# Work In Progress

staticimp is a work-in-progress. The basic features are stable, but thorough test code is still
needed and there are some missing important features that I am still implementing.

**Features still to implement**
- thorough test code
- logging
- spam protection (probably reCAPTCHA)
- github as a second backend
- I might include a filesystem backend for easier configuration testing
- specify allowed hosts for a backend


# Building and Running

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
sudo docker run -d --restart=always --name=staticimp --hostname=staticimp -p 8080:8080 -v "$(pwd)/staticimp.yml:/staticimp.yml:ro" -e gitlab_token=XXXXXX staticimp:latest
```

## cargo

staticimp uses rust [cargo](https://doc.rust-lang.org/cargo/) for building

To build staticimp and run it through cargo:
```bash
cp staticimp.sample.yml staticimp.yml # edit per your setup

gitlab_token=XXXXXXXXXX cargo run --release
# -- OR --
cargo build --release
gitlab_token=XXXXXXXXXX /target/release/staticimp
```

# Testing staticimp

Below are some useful oneliners for testing if staticimp is up and working.

the examples using `curl` are probably the ones you want, but for testing on minimal systems without installing curl there are `wget` and `nc` (netcat) examples too.

### test reachability
```bash
#using curl
curl 127.0.0.1:8080/

#using wget
wget -q -S -O - localhost:8080/ || { echo "connection failed"; }

#using nc
printf 'GET / HTTP/1.1\r\nConnection: close\r\n\r\n' | nc -v -w3 127.0.0.1 8080
```

### add a new entry
```bash
#note that project id can be numeric or the full path to the project
#using curl
curl -H "Content-Type: application/json" -X POST --data '{"name":"Michael","email":"mikeagun@gmail.com","comment":"this is a test"}' '127.0.0.1:8080/v1/entry/debug/42/main/comment?slug=staticimp-test'

#using wget
#NOTE: this fails with internal server error in current version on debug backend (I haven't debugged why yet)
wget -q -S -O - --header "Content-Type: application/json" --post-data '{"name":"John Doe","email":"johndoe@example.com","comment":"this is a test"}' 'http://127.0.0.1:8080/v1/entry/gitlab/mygroup/myproject/main/comment?slug=staticimp-test'

#using nc
sh -c 'printf "POST /v1/entry/gitlab/42/main/comment?slug=staticimp-test HTTP/1.1\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: $(echo -n "$0"|wc -c)\r\n\r\n$0"' '{"name":"John Doe","email":"johndoe@example.com","comment":"this is a test"}'
```


# Site Repo vs Comments Repo
The easiest way to use staticimp is have it commit files directly to your website content repository.
Your CI/CD job that builds the static site stays the same,
and you just need to configure your SSG to display the staticimp content.

Steps to get comment live (with comments in website repo):
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

Steps to get comment live (with separate comments repo):
1. HTML Form POSTs to staticimp
1. staticimp commits file to separate comments repo
1. comments repo CI/CD triggers website repo build+deploy pipeline
1. website repo CI/CD automatically rebuilds site with new comments and deploys to live site
  - added to the build step above is cloning the comments repo or making it a git submodule

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
  - `staticimp.yml` for server, configurable for project (`staticimp.yml` is a good choice)
  - the configuration options are similar, but the config format is different. see [staticimp.sample.yml]
- entry submission URL
  - `/v1/entry/{backend}/{project:.*}/{branch}/{entry_type}`

# Setting up Hugo


# Examples

See staticimp.sample.yml for a basic example, more to be added here ...

set `project_config_path: "staticimp.yml"` in config to keep staticimp project config in repo as "staticimp.yml"


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
