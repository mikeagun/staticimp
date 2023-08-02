# staticimp

# Introduction
staticimp (static imp) is a rust-based web service that receives user-generated content and uploads it to a backend (currently just GitLab, GitHub planned).
The main goal of staticimp is to support dynamic content (e.g. blog post comments) on a fully static website using automatic build+deployment.

staticimp consists of a small web service which handles POST requests from HTML forms (it also accepts json and yaml),
performs some validation and transformations, then uploads them using the GitLab REST API. staticman also supports moderation,
where the file is commited to a new branch and a merge request is created, instead of commiting the files directly to the main branch.

The actual rendering of the comments (or whatever you are using staticimp for) is up to your static site generator (SSG),
staticimp is just concerned with pushing the user generated content to the repo.

staticimp, like Staticman, can create yaml/json files in a repository, so if for example you are using a hugo theme that already supports staticman,
you should be able to support staticimp with minimal changes (staticimp.yml and changing the POST url).

# Inspiration
staticimp was inspired by the awesome project [Staticman](https://github.com/eduardoboucas/staticman).
If you already use Node.js and/or have a webserver with plenty of RAM, Staticman is a great tool and you should check it out.

While staticman is great, the memory consumption is a little heavy for running in stateless/serverless environments (e.g. [Google Cloud Run](https://cloud.google.com/run)) or on a tiny VPS.
In my use and testing it takes around 70-90+MB RAM idling, and >120MB during startup (which takes a second or two), which makes it a little tight for a VPS with 128MB total.
While that could probably be reduced some, Node isn't known for for being lightweight on RAM usage.

staticimp is another solution to the static-site-dynamic-content problem. startup takes milliseconds, and RAM usage is reasonable (under 10MB resident memory in testing)
It doesn't (yet) support all the features of staticman, but I welcome pull requests and have a few more features planned (like GitHub support).


# Features:
- clean implementation intended to be flexible and extensible
- configuration supports placeholders to fill in/transform entries
  - uses rendertemplate (in this crate) for rendering placeholders
- loads configuration from `staticman.yml`
  - doesn't yet support project-specific config or json
- entries are validated by checking for allowed/required fields
  - doesn't yet support any formatting validation
- extra fields generated from config
- has code to load/process field transformations (but doesn't have any implemented yet)
- can send processed entries to gitlab/debug backends
- moderated comments - commit to new branch and create merge request


# Work In Progress

staticimp is a work-in-progress. The features above all work, but thorough test code is still
needed and there are some missing important features that I am still implementing.

**Features still to implement**
- review branches
- thorough test code
- create and cache clients per-thread (rather than creating a new client for each request)
- load project/branch-specific config files
  - right now just loads the global conf at startup
- implement field transformations
- more documentation
- logging
- spam protection (probably reCAPTCHA)
- github as a second backend
- I might include a filesystem backend for easier configuration testing
- specify allowed hosts for a backend


# Requirements
- authentication token for the GitLab instance hosting your content repo
- docker (and/or a rust build environment)


# Site Repo vs Comments Repo
The easiest way to use staticimp is have it commit files directly to your website content repository.
Your CI/CD job that builds the static site stays the same,
and you just need to configure your SSG to display the staticimp content.

Steps to get comment live (with comments in website repo):
- HTML Form POSTs to staticimp
- staticimp commits files to website repo
- CI/CD pipeline automatically rebuild site with new comments and deploy to live site

The downside to the above approach is that your git history gets cluttered with staticimp commits, and if there is lots of traffic
you can get stuck in a loop of push, fail to push because your local repo is out of data, pull, repeat.
You can work in another branch and then use GitLab merge requests to merge the changes into your main branch,
but in the best case your git history will still be thoroughly cluttered.

Since I like keeping a relatively clean git history, I have staticimp push files to a separate repo, and then have the main content repo pull the latest
changes on build.

Steps to get comment live (with separate comments repo):
- HTML Form POSTs to staticimp
- staticimp commits files to separate comments repo
- comments repo CI/CD triggers website repo build+deploy pipeline
- CI/CD pipeline automatically rebuild site with new comments and deploy to live site
  - added to the build step above is cloning the comments repo or making it a git submodule

# Migrating from Staticman to staticimp

# Setting up Hugo


# Examples

See staticimp.sample.yml for a basic example, more to be added here ...

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
